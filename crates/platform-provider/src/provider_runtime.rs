use crate::{ProviderConfig, ProviderHttpProxyRegistry, ProviderSource, ProviderTransport};
use async_trait::async_trait;
use lenso_module_management::{
    EndpointResolverSource, PROVIDER_RUNTIME_PLAN_PROTOCOL, ProviderRuntimeModule,
    ProviderRuntimePlan, ProviderRuntimeService, ServiceIdentityPolicy, ServiceReference,
    ServiceTransportBinding, StaticEndpointDeclaration,
};
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::Module;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const BEARER_ENV_TRUST_PROFILE: &str = "bearer_env";

#[derive(Debug, Clone)]
pub struct ProviderEndpointResolutionRequest {
    pub service_ref: ServiceReference,
    pub source_id: String,
    pub public_config: BTreeMap<String, String>,
    pub secret_references: Vec<String>,
}

#[async_trait]
pub trait ProviderEndpointResolver: std::fmt::Debug + Send + Sync {
    async fn resolve(
        &self,
        request: &ProviderEndpointResolutionRequest,
    ) -> AppResult<Vec<StaticEndpointDeclaration>>;
}

#[async_trait]
pub trait ProviderCredentialResolver: std::fmt::Debug + Send + Sync {
    async fn resolve_bearer(&self, policy: &ServiceIdentityPolicy) -> AppResult<String>;
}

#[derive(Debug, Clone, Default)]
pub struct ProviderRuntimeAdapters {
    endpoint_resolvers: BTreeMap<String, Arc<dyn ProviderEndpointResolver>>,
    credential_resolvers: BTreeMap<String, Arc<dyn ProviderCredentialResolver>>,
}

impl ProviderRuntimeAdapters {
    #[must_use]
    pub fn production_defaults() -> Self {
        Self::default().with_credential_resolver(
            BEARER_ENV_TRUST_PROFILE,
            Arc::new(EnvironmentBearerCredentialResolver),
        )
    }

    #[must_use]
    pub fn with_endpoint_resolver(
        mut self,
        source_id: impl Into<String>,
        resolver: Arc<dyn ProviderEndpointResolver>,
    ) -> Self {
        self.endpoint_resolvers.insert(source_id.into(), resolver);
        self
    }

    #[must_use]
    pub fn with_credential_resolver(
        mut self,
        trust_profile: impl Into<String>,
        resolver: Arc<dyn ProviderCredentialResolver>,
    ) -> Self {
        self.credential_resolvers
            .insert(trust_profile.into(), resolver);
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EnvironmentBearerCredentialResolver;

#[async_trait]
impl ProviderCredentialResolver for EnvironmentBearerCredentialResolver {
    async fn resolve_bearer(&self, policy: &ServiceIdentityPolicy) -> AppResult<String> {
        if policy.credential_references.len() != 1 {
            return Err(AppError::new(
                ErrorCode::Validation,
                "bearer_env requires exactly one credential reference",
            ));
        }
        let reference = &policy.credential_references[0];
        let Some(name) = reference.strip_prefix("env://") else {
            return Err(AppError::new(
                ErrorCode::Validation,
                "bearer_env accepts only opaque env:// credential references",
            ));
        };
        if name.is_empty()
            || !name.chars().all(|character| {
                character == '_' || character.is_ascii_uppercase() || character.is_ascii_digit()
            })
        {
            return Err(AppError::new(
                ErrorCode::Validation,
                "bearer_env credential reference contains an unsafe environment name",
            ));
        }
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Validation,
                    format!("Provider credential reference '{reference}' is unresolved"),
                )
            })
    }
}

/// Internal transport adapter for an already compiled Provider Runtime Plan.
///
/// Selection, release resolution, and Manifest authority stay in
/// `lenso-module-management`; this adapter only resolves an allowed endpoint,
/// verifies its live descriptor, and attaches transport-backed behavior.
#[derive(Debug, Clone)]
pub struct ProviderRuntimeAdapter {
    plan: ProviderRuntimePlan,
    adapters: ProviderRuntimeAdapters,
}

#[derive(Debug)]
pub struct LoadedProviderRuntime {
    modules: Vec<Module>,
    configs: Vec<ProviderConfig>,
}

impl ProviderRuntimeAdapter {
    pub fn new(plan: ProviderRuntimePlan) -> AppResult<Self> {
        if plan.protocol != PROVIDER_RUNTIME_PLAN_PROTOCOL {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Runtime Plan protocol is unsupported",
            ));
        }
        Ok(Self {
            plan,
            adapters: ProviderRuntimeAdapters::production_defaults(),
        })
    }

    pub fn with_adapters(
        plan: ProviderRuntimePlan,
        adapters: ProviderRuntimeAdapters,
    ) -> AppResult<Self> {
        let mut runtime = Self::new(plan)?;
        runtime.adapters = adapters;
        Ok(runtime)
    }

    pub async fn load_verified(self) -> AppResult<LoadedProviderRuntime> {
        let mut modules = Vec::new();
        let mut configs = Vec::new();
        for provider in &self.plan.providers {
            let bearer = resolve_identity(&self.adapters, provider).await?;
            let endpoints = resolve_endpoints(&self.adapters, provider).await?;
            let endpoint = select_endpoint(provider, &endpoints)?;
            for locked in &provider.modules {
                let config = module_config(
                    locked,
                    &provider.service_release.digest,
                    endpoint,
                    bearer.as_deref(),
                );
                let loaded = ProviderSource::new(config)?
                    .load_locked(
                        &provider.service_ref.service_id,
                        &provider.service_release.version,
                        &provider.service_release.digest,
                        &locked.export_key,
                        &locked.module_release_digest,
                        &locked.manifest_digest,
                        &locked.contract_digests,
                        &locked.manifest,
                    )
                    .await?;
                configs.push(loaded.config);
                modules.push(loaded.module);
            }
        }
        Ok(LoadedProviderRuntime { modules, configs })
    }
}

async fn resolve_identity(
    adapters: &ProviderRuntimeAdapters,
    provider: &ProviderRuntimeService,
) -> AppResult<Option<String>> {
    if provider
        .endpoint_binding
        .identity_policy
        .credential_references
        .is_empty()
    {
        return Ok(None);
    }
    let profile = &provider.endpoint_binding.identity_policy.trust_profile;
    let resolver = adapters.credential_resolvers.get(profile).ok_or_else(|| {
        AppError::new(
            ErrorCode::Validation,
            format!(
                "Provider Service '{}' requires credential adapter '{}'",
                provider.service_ref.service_id, profile
            ),
        )
    })?;
    resolver
        .resolve_bearer(&provider.endpoint_binding.identity_policy)
        .await
        .map(Some)
}

impl LoadedProviderRuntime {
    #[must_use]
    pub fn proxy_registry(&self) -> ProviderHttpProxyRegistry {
        ProviderHttpProxyRegistry::from_modules(&self.modules, &self.configs)
    }

    #[must_use]
    pub fn into_modules(self) -> Vec<Module> {
        self.modules
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<Module>, Vec<ProviderConfig>) {
        (self.modules, self.configs)
    }
}

async fn resolve_endpoints(
    adapters: &ProviderRuntimeAdapters,
    provider: &ProviderRuntimeService,
) -> AppResult<Vec<StaticEndpointDeclaration>> {
    let (source_id, public_config, secret_references) =
        match &provider.endpoint_binding.resolver_source {
            EndpointResolverSource::Static { endpoints } => return Ok(endpoints.clone()),
            EndpointResolverSource::LocalProcess { source_id } => {
                (source_id.clone(), BTreeMap::new(), Vec::new())
            }
            EndpointResolverSource::Adapter {
                adapter_id,
                public_config,
                secret_references,
            } => (
                adapter_id.clone(),
                public_config.clone(),
                secret_references.clone(),
            ),
        };
    let resolver = adapters.endpoint_resolvers.get(&source_id).ok_or_else(|| {
        AppError::new(
            ErrorCode::Validation,
            format!(
                "Provider Service '{}' requires endpoint resolver adapter '{}'",
                provider.service_ref.service_id, source_id
            ),
        )
    })?;
    resolver
        .resolve(&ProviderEndpointResolutionRequest {
            service_ref: provider.service_ref.clone(),
            source_id,
            public_config,
            secret_references,
        })
        .await
}

fn select_endpoint<'a>(
    provider: &ProviderRuntimeService,
    endpoints: &'a [StaticEndpointDeclaration],
) -> AppResult<&'a StaticEndpointDeclaration> {
    let allowed = &provider.endpoint_binding.allowed_bindings;
    let preferred_regions = &provider.endpoint_binding.selection_policy.preferred_regions;
    endpoints
        .iter()
        .filter(|endpoint| {
            allowed.contains(&endpoint.binding)
                && matches!(
                    endpoint.binding,
                    ServiceTransportBinding::ProviderHttpJson
                        | ServiceTransportBinding::ProviderGrpc
                )
                && !endpoint.address.trim().is_empty()
        })
        .min_by_key(|endpoint| {
            let region_rank = endpoint
                .region
                .as_ref()
                .and_then(|region| preferred_regions.iter().position(|item| item == region))
                .unwrap_or(usize::MAX);
            (region_rank, endpoint.priority, endpoint.address.as_str())
        })
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::Validation,
                format!(
                    "Provider Service '{}' has no eligible resolved endpoint",
                    provider.service_ref.service_id
                ),
            )
        })
}

#[derive(Debug, Clone, Default)]
pub struct FixedProviderEndpointResolver {
    endpoints: BTreeMap<ServiceReference, Vec<StaticEndpointDeclaration>>,
}

impl FixedProviderEndpointResolver {
    #[must_use]
    pub fn new(
        endpoints: impl IntoIterator<Item = (ServiceReference, Vec<StaticEndpointDeclaration>)>,
    ) -> Self {
        Self {
            endpoints: endpoints.into_iter().collect(),
        }
    }
}

#[async_trait]
impl ProviderEndpointResolver for FixedProviderEndpointResolver {
    async fn resolve(
        &self,
        request: &ProviderEndpointResolutionRequest,
    ) -> AppResult<Vec<StaticEndpointDeclaration>> {
        self.endpoints
            .get(&request.service_ref)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Validation,
                    format!(
                        "endpoint resolver '{}' has no state for Provider Service '{}'",
                        request.source_id, request.service_ref.service_id
                    ),
                )
            })
    }
}

#[derive(Debug, Clone)]
pub struct FixedBearerCredentialResolver {
    bearer: String,
}

impl FixedBearerCredentialResolver {
    #[must_use]
    pub fn new(bearer: impl Into<String>) -> Self {
        Self {
            bearer: bearer.into(),
        }
    }
}

#[async_trait]
impl ProviderCredentialResolver for FixedBearerCredentialResolver {
    async fn resolve_bearer(&self, _policy: &ServiceIdentityPolicy) -> AppResult<String> {
        if self.bearer.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::Validation,
                "fixed Provider bearer credential is empty",
            ));
        }
        Ok(self.bearer.clone())
    }
}

fn module_config(
    locked: &ProviderRuntimeModule,
    service_release_digest: &str,
    endpoint: &StaticEndpointDeclaration,
    bearer: Option<&str>,
) -> ProviderConfig {
    let transport = match endpoint.binding {
        ServiceTransportBinding::ProviderHttpJson => ProviderTransport::HttpJson,
        ServiceTransportBinding::ProviderGrpc => ProviderTransport::Grpc,
        _ => unreachable!("endpoint selection accepts only Provider transports"),
    };
    let config = ProviderConfig::new(&locked.module_id, &endpoint.address)
        .with_transport(transport, &endpoint.address)
        .with_export_key(&locked.export_key)
        .with_locked_contract(
            service_release_digest,
            &locked.module_release_digest,
            &locked.manifest_digest,
            locked.contract_digests.clone(),
        );
    match bearer {
        Some(bearer) => config.with_auth_token(bearer),
        None => config,
    }
}
