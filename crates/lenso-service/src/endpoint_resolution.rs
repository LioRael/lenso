use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{Arc, RwLock},
};

/// Stable logical input for Service discovery. It intentionally contains no
/// instance, Workload, endpoint, host, or region identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServiceReference(String);

impl ServiceReference {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operating_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_domain: Option<String>,
}

impl Endpoint {
    #[must_use]
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            operating_region: None,
            failure_domain: None,
        }
    }

    #[must_use]
    pub fn in_region(mut self, region: impl Into<String>) -> Self {
        self.operating_region = Some(region.into());
        self
    }

    #[must_use]
    pub fn in_failure_domain(mut self, failure_domain: impl Into<String>) -> Self {
        self.failure_domain = Some(failure_domain.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointState {
    pub service: ServiceReference,
    pub endpoints: Vec<Endpoint>,
}

impl EndpointState {
    #[must_use]
    pub fn new(service: ServiceReference, endpoints: Vec<Endpoint>) -> Self {
        Self { service, endpoints }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointResolutionErrorCode {
    InvalidEndpointState,
    SourceUnavailable,
    NoUsableEndpointState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointResolutionError {
    pub code: EndpointResolutionErrorCode,
    pub message: String,
    pub next_action: String,
}

impl EndpointResolutionError {
    #[must_use]
    pub fn source_unavailable(service: &ServiceReference, message: impl Into<String>) -> Self {
        Self {
            code: EndpointResolutionErrorCode::SourceUnavailable,
            message: message.into(),
            next_action: format!(
                "Restore the endpoint source for Service Reference `{}`, then retry.",
                service.as_str()
            ),
        }
    }

    fn no_usable_state(service: &ServiceReference) -> Self {
        Self {
            code: EndpointResolutionErrorCode::NoUsableEndpointState,
            message: format!(
                "No usable endpoint state exists for Service Reference `{}`.",
                service.as_str()
            ),
            next_action: format!(
                "Configure or publish at least one endpoint for Service Reference `{}`, then retry.",
                service.as_str()
            ),
        }
    }

    fn invalid_state(message: impl Into<String>) -> Self {
        Self {
            code: EndpointResolutionErrorCode::InvalidEndpointState,
            message: message.into(),
            next_action: "Publish a non-empty endpoint state whose Service Reference matches its registry key."
                .to_owned(),
        }
    }
}

impl fmt::Display for EndpointResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for EndpointResolutionError {}

pub trait EndpointResolver {
    fn resolve(&self, service: &ServiceReference)
    -> Result<EndpointState, EndpointResolutionError>;
}

#[derive(Debug, Clone)]
pub struct StaticEndpointResolver {
    states: BTreeMap<ServiceReference, EndpointState>,
}

impl StaticEndpointResolver {
    pub fn new(
        states: impl IntoIterator<Item = EndpointState>,
    ) -> Result<Self, EndpointResolutionError> {
        let mut configured = BTreeMap::new();
        for state in states {
            validate_state(&state)?;
            let service = state.service.clone();
            if configured.insert(service.clone(), state).is_some() {
                return Err(EndpointResolutionError::invalid_state(format!(
                    "Static endpoint configuration contains Service Reference `{}` more than once.",
                    service.as_str()
                )));
            }
        }
        Ok(Self { states: configured })
    }
}

impl EndpointResolver for StaticEndpointResolver {
    fn resolve(
        &self,
        service: &ServiceReference,
    ) -> Result<EndpointState, EndpointResolutionError> {
        self.states
            .get(service)
            .cloned()
            .ok_or_else(|| EndpointResolutionError::no_usable_state(service))
    }
}

#[derive(Debug, Clone, Default)]
/// In-process registry for a local development supervisor that starts and
/// observes Autonomous Service Workloads on the same machine.
pub struct LocalProcessEndpointResolver {
    states: Arc<RwLock<BTreeMap<ServiceReference, EndpointState>>>,
}

impl LocalProcessEndpointResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish(&self, state: EndpointState) -> Result<(), EndpointResolutionError> {
        validate_state(&state)?;
        self.states
            .write()
            .expect("local process endpoint registry lock poisoned")
            .insert(state.service.clone(), state);
        Ok(())
    }

    pub fn remove(&self, service: &ServiceReference) -> Option<EndpointState> {
        self.states
            .write()
            .expect("local process endpoint registry lock poisoned")
            .remove(service)
    }
}

impl EndpointResolver for LocalProcessEndpointResolver {
    fn resolve(
        &self,
        service: &ServiceReference,
    ) -> Result<EndpointState, EndpointResolutionError> {
        self.states
            .read()
            .expect("local process endpoint registry lock poisoned")
            .get(service)
            .cloned()
            .ok_or_else(|| EndpointResolutionError::no_usable_state(service))
    }
}

#[derive(Debug, Clone)]
/// Client-side resolver that keeps the last valid state from any resolver
/// adapter, so source or System Plane availability is never request-path state.
pub struct LastValidEndpointResolver<R> {
    source: R,
    last_valid: Arc<RwLock<BTreeMap<ServiceReference, EndpointState>>>,
}

impl<R> LastValidEndpointResolver<R> {
    #[must_use]
    pub fn new(source: R) -> Self {
        Self {
            source,
            last_valid: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

impl<R: EndpointResolver> EndpointResolver for LastValidEndpointResolver<R> {
    fn resolve(
        &self,
        service: &ServiceReference,
    ) -> Result<EndpointState, EndpointResolutionError> {
        match self.source.resolve(service) {
            Ok(state) => {
                if validate_state_for(service, &state).is_ok() {
                    self.last_valid
                        .write()
                        .expect("last valid endpoint state lock poisoned")
                        .insert(service.clone(), state.clone());
                    Ok(state)
                } else {
                    self.cached_or_unavailable(service)
                }
            }
            Err(_) => self.cached_or_unavailable(service),
        }
    }
}

impl<R> LastValidEndpointResolver<R> {
    fn cached_or_unavailable(
        &self,
        service: &ServiceReference,
    ) -> Result<EndpointState, EndpointResolutionError> {
        self.last_valid
            .read()
            .expect("last valid endpoint state lock poisoned")
            .get(service)
            .cloned()
            .ok_or_else(|| EndpointResolutionError::no_usable_state(service))
    }
}

fn validate_state(state: &EndpointState) -> Result<(), EndpointResolutionError> {
    validate_state_for(&state.service, state)
}

fn validate_state_for(
    service: &ServiceReference,
    state: &EndpointState,
) -> Result<(), EndpointResolutionError> {
    if state.service != *service {
        return Err(EndpointResolutionError::invalid_state(
            "Endpoint state Service Reference does not match the requested Service Reference.",
        ));
    }
    if service.as_str().trim().is_empty() || state.endpoints.is_empty() {
        return Err(EndpointResolutionError::invalid_state(
            "Endpoint state requires a non-empty Service Reference and at least one endpoint.",
        ));
    }
    if state
        .endpoints
        .iter()
        .any(|endpoint| endpoint.address.trim().is_empty())
    {
        return Err(EndpointResolutionError::invalid_state(
            "Endpoint addresses must not be empty.",
        ));
    }
    Ok(())
}
