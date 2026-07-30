//! Capability-neutral System Plane Core routing, registration, and negotiation.

mod enrollment;

pub use enrollment::*;

use axum::{
    Extension, Json,
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use lenso_service::{
    AuthenticatedTransportBinding, WorkloadIdentityProvider, WorkloadIdentityVerification,
    system_plane::{
        CORE_PROTOCOL, CapabilityAdvertisement, CoreDocument, CoreIssue, validate_core_document,
    },
};
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashSet},
    fmt,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Debug, Clone)]
pub struct SystemPlaneRegistry {
    document: Arc<CoreDocument>,
}

impl SystemPlaneRegistry {
    pub fn new(document: CoreDocument) -> Result<Self, Vec<CoreIssue>> {
        let issues = validate_core_document(&document);
        if issues.is_empty() {
            Ok(Self {
                document: Arc::new(document),
            })
        } else {
            Err(issues)
        }
    }

    #[must_use]
    pub fn document(&self) -> &CoreDocument {
        &self.document
    }

    #[must_use]
    pub fn negotiate(&self, requirements: &[CapabilityRequirement]) -> CapabilityNegotiation {
        negotiate_capabilities(self.document(), requirements)
    }
}

#[derive(Debug, Clone)]
pub struct SystemPlaneRegistryBuilder {
    document: CoreDocument,
}

impl SystemPlaneRegistryBuilder {
    #[must_use]
    pub fn new(
        service_id: impl Into<String>,
        service_principal: impl Into<String>,
        service_revision: impl Into<String>,
    ) -> Self {
        Self {
            document: CoreDocument {
                protocol: CORE_PROTOCOL.to_owned(),
                service_id: service_id.into(),
                service_principal: service_principal.into(),
                service_revision: service_revision.into(),
                capabilities: Vec::new(),
            },
        }
    }

    #[must_use]
    pub fn register(mut self, capability: CapabilityAdvertisement) -> Self {
        self.document.capabilities.push(capability);
        self
    }

    pub fn build(mut self) -> Result<SystemPlaneRegistry, Vec<CoreIssue>> {
        self.document
            .capabilities
            .sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
        SystemPlaneRegistry::new(self.document)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRequirement {
    pub capability_id: String,
    pub supported_major_versions: BTreeSet<u32>,
    pub required_feature_ids: BTreeSet<String>,
    pub accepted_schema_digests: BTreeSet<String>,
}

impl CapabilityRequirement {
    #[must_use]
    pub fn new(
        capability_id: impl Into<String>,
        supported_major_versions: impl IntoIterator<Item = u32>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            supported_major_versions: supported_major_versions.into_iter().collect(),
            required_feature_ids: BTreeSet::new(),
            accepted_schema_digests: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn requiring_features(
        mut self,
        feature_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.required_feature_ids = feature_ids.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn accepting_schema_digests(
        mut self,
        digests: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.accepted_schema_digests = digests.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityNegotiationIssueCode {
    InvalidRequirement,
    DuplicateRequirement,
    MissingCapability,
    UnsupportedMajorVersion,
    MissingRequiredFeature,
    SchemaDigestMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityNegotiationIssue {
    pub code: CapabilityNegotiationIssueCode,
    pub capability_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiatedCapability {
    pub capability_id: String,
    pub advertisement: CapabilityAdvertisement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityNegotiation {
    pub accepted: Vec<NegotiatedCapability>,
    pub issues: Vec<CapabilityNegotiationIssue>,
}

impl CapabilityNegotiation {
    #[must_use]
    pub const fn is_compatible(&self) -> bool {
        self.issues.is_empty()
    }
}

#[must_use]
pub fn negotiate_capabilities(
    document: &CoreDocument,
    requirements: &[CapabilityRequirement],
) -> CapabilityNegotiation {
    let mut accepted = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();

    for requirement in requirements {
        if !valid_capability_id(&requirement.capability_id)
            || requirement.supported_major_versions.is_empty()
            || requirement.supported_major_versions.contains(&0)
        {
            negotiation_issue(
                &mut issues,
                CapabilityNegotiationIssueCode::InvalidRequirement,
                requirement,
                "Capability requirements need a canonical identifier and at least one positive major version",
            );
            continue;
        }
        if !seen.insert(requirement.capability_id.as_str()) {
            negotiation_issue(
                &mut issues,
                CapabilityNegotiationIssueCode::DuplicateRequirement,
                requirement,
                "Each capability may be negotiated once",
            );
            continue;
        }

        let prefix = format!("lenso.system-plane.{}.v", requirement.capability_id);
        let candidates = document
            .capabilities
            .iter()
            .filter(|capability| {
                capability.contract_id.starts_with(&prefix)
                    && capability.contract_id == format!("{}{}", prefix, capability.major_version)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            negotiation_issue(
                &mut issues,
                CapabilityNegotiationIssueCode::MissingCapability,
                requirement,
                "The managed Service does not advertise this capability",
            );
            continue;
        }
        let candidate = candidates
            .into_iter()
            .filter(|capability| {
                requirement
                    .supported_major_versions
                    .contains(&capability.major_version)
            })
            .max_by_key(|capability| capability.major_version);
        let Some(candidate) = candidate else {
            negotiation_issue(
                &mut issues,
                CapabilityNegotiationIssueCode::UnsupportedMajorVersion,
                requirement,
                "The managed Service and consumer share no supported major version",
            );
            continue;
        };
        if !requirement
            .required_feature_ids
            .is_subset(&candidate.feature_ids)
        {
            negotiation_issue(
                &mut issues,
                CapabilityNegotiationIssueCode::MissingRequiredFeature,
                requirement,
                "The advertised contract is missing a required feature identifier",
            );
            continue;
        }
        if !requirement.accepted_schema_digests.is_empty()
            && !requirement
                .accepted_schema_digests
                .contains(&candidate.schema_digest)
        {
            negotiation_issue(
                &mut issues,
                CapabilityNegotiationIssueCode::SchemaDigestMismatch,
                requirement,
                "The advertised schema digest is not accepted by the consumer",
            );
            continue;
        }
        accepted.push(NegotiatedCapability {
            capability_id: requirement.capability_id.clone(),
            advertisement: candidate.clone(),
        });
    }

    accepted.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
    CapabilityNegotiation { accepted, issues }
}

fn negotiation_issue(
    issues: &mut Vec<CapabilityNegotiationIssue>,
    code: CapabilityNegotiationIssueCode,
    requirement: &CapabilityRequirement,
    message: &str,
) {
    issues.push(CapabilityNegotiationIssue {
        code,
        capability_id: requirement.capability_id.clone(),
        message: message.to_owned(),
    });
}

fn valid_capability_id(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('-')
                && !segment.ends_with('-')
                && !segment.contains("--")
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

#[derive(Clone)]
pub struct SystemPlaneAccess {
    provider: Arc<dyn WorkloadIdentityProvider>,
    audience: String,
    enrollment_authorizer: Arc<dyn EnrollmentAuthorizer>,
}

impl fmt::Debug for SystemPlaneAccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SystemPlaneAccess")
            .field("provider", &self.provider)
            .field("audience", &self.audience)
            .field("enrollment_authorizer", &self.enrollment_authorizer)
            .finish()
    }
}

impl SystemPlaneAccess {
    #[must_use]
    pub fn new(
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
        enrollment_authorizer: Arc<dyn EnrollmentAuthorizer>,
    ) -> Self {
        Self {
            provider,
            audience: audience.into(),
            enrollment_authorizer,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemPlaneRuntime {
    pub registry: Arc<SystemPlaneRegistry>,
    pub access: Arc<SystemPlaneAccess>,
}

impl SystemPlaneRuntime {
    #[must_use]
    pub fn new(registry: SystemPlaneRegistry, access: SystemPlaneAccess) -> Self {
        Self {
            registry: Arc::new(registry),
            access: Arc::new(access),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemPlaneErrorBody {
    pub code: &'static str,
    pub message: String,
    pub next_actions: Vec<&'static str>,
}

#[derive(Debug)]
pub struct SystemPlaneRejection {
    status: StatusCode,
    code: &'static str,
    message: String,
    next_action: &'static str,
}

impl SystemPlaneRejection {
    #[must_use]
    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        next_action: &'static str,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            next_action,
        }
    }

    #[must_use]
    pub fn unavailable(
        code: &'static str,
        message: impl Into<String>,
        next_action: &'static str,
    ) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code,
            message: message.into(),
            next_action,
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

impl IntoResponse for SystemPlaneRejection {
    fn into_response(self) -> Response {
        let body = SystemPlaneErrorBody {
            code: self.code,
            message: self.message,
            next_actions: vec![self.next_action],
        };
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
            .headers_mut()
            .insert("x-lenso-error-code", HeaderValue::from_static(self.code));
        response
    }
}

#[derive(Debug, Clone)]
pub struct AuthorizedSystemPlaneCaller {
    pub runtime: Arc<SystemPlaneRuntime>,
    pub service_principal: String,
    pub enrollment: EnrollmentAuthorization,
}

impl AuthorizedSystemPlaneCaller {
    pub fn require_capability(
        &self,
        contract_id: &str,
        schema_digest: &str,
        required_feature_ids: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), SystemPlaneRejection> {
        if self.enrollment.system_id == "system-sandbox" {
            return Ok(());
        }
        let required_feature_ids = required_feature_ids
            .into_iter()
            .map(|feature| feature.as_ref().to_owned())
            .collect::<BTreeSet<_>>();
        let granted = self.enrollment.capabilities.iter().any(|capability| {
            capability.contract_id == contract_id
                && capability.schema_digest == schema_digest
                && required_feature_ids.is_subset(&capability.feature_ids)
        });
        if granted {
            Ok(())
        } else {
            Err(SystemPlaneRejection {
                status: StatusCode::FORBIDDEN,
                code: "system_plane_capability_not_granted",
                message: "Active Enrollment Grant does not authorize the requested capability"
                    .to_owned(),
                next_action: "review_service_enrollment_grant",
            })
        }
    }
}

impl<S> FromRequestParts<S> for AuthorizedSystemPlaneCaller
where
    S: Send + Sync,
{
    type Rejection = SystemPlaneRejection;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let runtime = parts
            .extensions
            .get::<Option<Arc<SystemPlaneRuntime>>>()
            .and_then(Clone::clone)
            .ok_or_else(|| SystemPlaneRejection {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "system_plane_unavailable",
                message: "System Plane access is not configured for this Service".to_owned(),
                next_action: "configure_system_plane",
            })?;
        let token = bearer_token(&parts.headers)?;
        let binding = parts
            .extensions
            .get::<AuthenticatedTransportBinding>()
            .ok_or_else(|| SystemPlaneRejection {
                status: StatusCode::UNAUTHORIZED,
                code: "system_plane_transport_binding_required",
                message: "System Plane access requires an authenticated transport binding"
                    .to_owned(),
                next_action: "use_authenticated_transport",
            })?;
        let principal = runtime
            .access
            .provider
            .verify(
                token,
                &WorkloadIdentityVerification::new(
                    &runtime.access.audience,
                    &binding.0,
                    now_unix_ms(),
                ),
            )
            .map_err(|error| SystemPlaneRejection {
                status: StatusCode::UNAUTHORIZED,
                code: "system_plane_workload_identity_rejected",
                message: error.message,
                next_action: "refresh_workload_identity",
            })?;
        let enrollment = runtime
            .access
            .enrollment_authorizer
            .authorize(
                &runtime.registry.document().service_id,
                &principal.service_principal,
                now_unix_ms(),
            )
            .await
            .map_err(enrollment_rejection)?;
        Ok(Self {
            runtime,
            service_principal: principal.service_principal,
            enrollment,
        })
    }
}

/// Builds the mandatory Core discovery route. An absent runtime fails closed.
#[must_use]
pub fn router<S>(runtime: Option<Arc<SystemPlaneRuntime>>) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(discover_core))
        .layer(Extension(runtime))
}

#[utoipa::path(
    get,
    path = "/system-plane/v1",
    responses(
        (status = 200, description = "Authenticated System Plane Core document", body = CoreDocument),
        (status = 401, description = "Workload Identity or transport binding was not accepted", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not the enrolled Console Service Principal", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, description = "System Plane access is not configured", body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane"
)]
async fn discover_core(caller: AuthorizedSystemPlaneCaller) -> Json<CoreDocument> {
    Json(caller.runtime.registry.document().clone())
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, SystemPlaneRejection> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SystemPlaneRejection {
            status: StatusCode::UNAUTHORIZED,
            code: "system_plane_workload_identity_required",
            message: "System Plane access requires a Workload Identity Bearer credential"
                .to_owned(),
            next_action: "provide_workload_identity",
        })
}

fn enrollment_rejection(error: EnrollmentError) -> SystemPlaneRejection {
    let (status, code, next_action) = match error.code {
        EnrollmentErrorCode::StoreUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "system_plane_enrollment_unavailable",
            "restore_enrollment_store",
        ),
        EnrollmentErrorCode::Expired => (
            StatusCode::FORBIDDEN,
            "system_plane_enrollment_expired",
            "renew_service_enrollment",
        ),
        EnrollmentErrorCode::Revoked => (
            StatusCode::FORBIDDEN,
            "system_plane_enrollment_revoked",
            "complete_service_enrollment",
        ),
        EnrollmentErrorCode::PrincipalMismatch => (
            StatusCode::FORBIDDEN,
            "system_plane_console_not_enrolled",
            "complete_service_enrollment",
        ),
        EnrollmentErrorCode::NotEnrolled
        | EnrollmentErrorCode::InvalidGrant
        | EnrollmentErrorCode::InvalidDecision
        | EnrollmentErrorCode::SignatureRejected
        | EnrollmentErrorCode::NonceReused
        | EnrollmentErrorCode::AlreadyEnrolled
        | EnrollmentErrorCode::StaleAuthorizationEpoch => (
            StatusCode::FORBIDDEN,
            "system_plane_enrollment_required",
            "complete_service_enrollment",
        ),
    };
    SystemPlaneRejection {
        status,
        code,
        message: error.message,
        next_action,
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
