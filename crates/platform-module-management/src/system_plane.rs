//! Target-owned Service Installation capability for the independent System Plane.

use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
};
use lenso_module_management::{
    ManagementActor, ServiceInstallationChange, ServiceInstallationError, ServiceInstallationPlan,
    ServiceInstallationReceipt, ServiceInstallationSet, WorkspaceServiceInstallationManager,
};
use lenso_service::system_plane::CapabilityAdvertisement;
use platform_system_plane::{
    AuthorizedSystemPlaneCaller, SystemPlaneErrorBody, SystemPlaneRejection,
};
use schemars::schema_for;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};
use utoipa_axum::{router::OpenApiRouter, routes};

pub const SERVICE_INSTALLATIONS_PROTOCOL: &str = "lenso.system-plane.service-installations.v1";
pub const SERVICE_INSTALLATIONS_PATH: &str = "/system-plane/v1/service-installations";
pub const SERVICE_INSTALLATIONS_FEATURE_SNAPSHOT: &str = "installation.snapshot";
pub const SERVICE_INSTALLATIONS_FEATURE_PLAN: &str = "installation.plan";
pub const SERVICE_INSTALLATIONS_FEATURE_APPLY: &str = "installation.apply";

#[derive(Debug, Clone)]
pub struct ServiceInstallationsProvider {
    root: PathBuf,
}

impl ServiceInstallationsProvider {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    #[must_use]
    pub fn advertisement() -> CapabilityAdvertisement {
        CapabilityAdvertisement {
            contract_id: SERVICE_INSTALLATIONS_PROTOCOL.to_owned(),
            major_version: 1,
            feature_ids: BTreeSet::from([
                SERVICE_INSTALLATIONS_FEATURE_APPLY.to_owned(),
                SERVICE_INSTALLATIONS_FEATURE_PLAN.to_owned(),
                SERVICE_INSTALLATIONS_FEATURE_SNAPSHOT.to_owned(),
            ]),
            schema_digest: service_installations_schema_digest(),
            endpoint: SERVICE_INSTALLATIONS_PATH.to_owned(),
        }
    }

    pub fn snapshot(
        &self,
        system_id: impl Into<String>,
        environment_id: impl Into<String>,
    ) -> Result<ServiceInstallationSet, ServiceInstallationError> {
        WorkspaceServiceInstallationManager::new(&self.root, system_id, environment_id).snapshot()
    }

    pub fn preview(
        &self,
        system_id: impl Into<String>,
        environment_id: impl Into<String>,
        change: ServiceInstallationChange,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<ServiceInstallationPlan, ServiceInstallationError> {
        WorkspaceServiceInstallationManager::new(&self.root, system_id, environment_id)
            .preview(change, now)
    }

    pub fn apply(
        &self,
        operation_id: &str,
        plan: &ServiceInstallationPlan,
        actor: &ManagementActor,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<ServiceInstallationReceipt, ServiceInstallationError> {
        WorkspaceServiceInstallationManager::new(&self.root, &plan.system_id, &plan.environment_id)
            .apply(
                operation_id,
                plan,
                &actor.actor_id,
                &actor.verified_authorities,
                now,
            )
    }
}

#[must_use]
pub fn service_installations_schema_digest() -> String {
    lenso_contracts::digest_json(&serde_json::json!({
        "change": schema_for!(ServiceInstallationChange),
        "plan": schema_for!(ServiceInstallationPlan),
        "snapshot": schema_for!(lenso_module_management::ServiceInstallationSet),
        "receipt": schema_for!(lenso_module_management::ServiceInstallationReceipt),
    }))
    .expect("Service Installation schemas are serializable")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallationScope {
    system_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewInstallationBody {
    system_id: String,
    environment_id: String,
    change: ServiceInstallationChange,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyInstallationBody {
    operation_id: String,
    plan: ServiceInstallationPlan,
}

#[must_use]
pub fn system_plane_router<S>(
    provider: Option<Arc<ServiceInstallationsProvider>>,
) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(installation_snapshot))
        .routes(routes!(preview_installation))
        .routes(routes!(apply_installation))
        .layer(Extension(provider))
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/service-installations/{environment_id}",
    params(("environment_id" = String, Path), ("system_id" = String, Query)),
    responses(
        (status = 200, description = "Target-owned desired Service Installation Set", body = Value),
        (status = 401, description = "Workload Identity was rejected", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, description = "Enrollment does not grant snapshot access", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, description = "Service Installation state is unavailable", body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-service-installations"
)]
async fn installation_snapshot(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ServiceInstallationsProvider>>>,
    Path(environment_id): Path<String>,
    Query(scope): Query<InstallationScope>,
) -> Result<Json<Value>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_feature(&caller, SERVICE_INSTALLATIONS_FEATURE_SNAPSHOT)?;
    encode(
        provider
            .snapshot(scope.system_id, environment_id)
            .map_err(map_error)?,
    )
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/service-installations/plans/preview",
    request_body(content = Value, content_type = "application/json"),
    responses(
        (status = 200, description = "Immutable Service Installation Plan", body = Value),
        (status = 400, description = "Installation change is invalid", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, description = "Workload Identity was rejected", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, description = "Enrollment does not grant planning access", body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-service-installations"
)]
async fn preview_installation(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ServiceInstallationsProvider>>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_feature(&caller, SERVICE_INSTALLATIONS_FEATURE_PLAN)?;
    let body: PreviewInstallationBody = serde_json::from_value(body).map_err(|error| {
        invalid_request(format!("Service Installation change is invalid: {error}"))
    })?;
    encode(
        provider
            .preview(
                body.system_id,
                body.environment_id,
                body.change,
                chrono::Utc::now(),
            )
            .map_err(map_error)?,
    )
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/service-installations/plans/{plan_id}/apply",
    params(("plan_id" = String, Path)),
    request_body(content = Value, content_type = "application/json"),
    responses(
        (status = 200, description = "Durable Service Installation Receipt", body = Value),
        (status = 400, description = "Installation plan is invalid", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, description = "Workload Identity was rejected", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, description = "Enrollment does not grant apply access", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 409, description = "Installation state changed after preview", body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-service-installations"
)]
async fn apply_installation(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ServiceInstallationsProvider>>>,
    Path(plan_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_feature(&caller, SERVICE_INSTALLATIONS_FEATURE_APPLY)?;
    let body: ApplyInstallationBody = serde_json::from_value(body).map_err(|error| {
        invalid_request(format!("Service Installation plan is invalid: {error}"))
    })?;
    if body.plan.plan_id != plan_id {
        return Err(invalid_request(
            "Service Installation plan identity differs from request path",
        ));
    }
    let actor = ManagementActor {
        actor_id: format!("service:{}", caller.service_principal),
        verified_authorities: BTreeSet::from(["service.manage".to_owned()]),
    };
    encode(
        provider
            .apply(&body.operation_id, &body.plan, &actor, chrono::Utc::now())
            .map_err(map_error)?,
    )
}

fn require_provider(
    provider: Option<Arc<ServiceInstallationsProvider>>,
) -> Result<Arc<ServiceInstallationsProvider>, SystemPlaneRejection> {
    provider.ok_or_else(|| {
        SystemPlaneRejection::unavailable(
            "service_installations_unavailable",
            "Service Installations capability is not configured for this Service",
            "configure_service_installations",
        )
    })
}

fn require_feature(
    caller: &AuthorizedSystemPlaneCaller,
    feature: &str,
) -> Result<(), SystemPlaneRejection> {
    caller.require_capability(
        SERVICE_INSTALLATIONS_PROTOCOL,
        &service_installations_schema_digest(),
        [feature],
    )
}

fn encode(value: impl serde::Serialize) -> Result<Json<Value>, SystemPlaneRejection> {
    serde_json::to_value(value).map(Json).map_err(|_| {
        SystemPlaneRejection::unavailable(
            "service_installations_serialization_failed",
            "Service Installation result could not be encoded",
            "inspect_service_installation_state",
        )
    })
}

fn invalid_request(message: impl Into<String>) -> SystemPlaneRejection {
    SystemPlaneRejection::new(
        StatusCode::BAD_REQUEST,
        "service_installations_invalid_request",
        message,
        "correct_service_installation_request",
    )
}

fn map_error(error: ServiceInstallationError) -> SystemPlaneRejection {
    let (status, code, next_action) = match error {
        ServiceInstallationError::InvalidContract(_)
        | ServiceInstallationError::UnsafeOperationIdentity
        | ServiceInstallationError::Json(_) => (
            StatusCode::BAD_REQUEST,
            "service_installations_invalid_request",
            "correct_service_installation_request",
        ),
        ServiceInstallationError::MissingAuthority(_) => (
            StatusCode::FORBIDDEN,
            "service_installations_authority_required",
            "review_service_enrollment_grant",
        ),
        ServiceInstallationError::StaleState => (
            StatusCode::CONFLICT,
            "service_installations_stale_state",
            "preview_service_installation_again",
        ),
        ServiceInstallationError::Io(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "service_installations_store_unavailable",
            "restore_service_installation_store",
        ),
    };
    SystemPlaneRejection::new(status, code, error.to_string(), next_action)
}
