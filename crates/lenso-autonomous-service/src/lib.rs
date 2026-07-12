//! Host-independent runtime composition for one `lenso.service.v2` Service.

use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse as _, Response},
};
use lenso_service::{
    AutonomousServiceContract, WorkloadRole, validate_autonomous_service_contract,
};
use platform_core::{Migration, apply_migrations};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::{Arc, RwLock};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRuntimeConfig {
    pub service_id: String,
    pub store_id: String,
    pub store_owner_service_id: String,
    pub values: serde_json::Value,
}

impl ServiceRuntimeConfig {
    #[must_use]
    pub fn new(
        service_id: impl Into<String>,
        store_id: impl Into<String>,
        store_owner_service_id: impl Into<String>,
    ) -> Self {
        Self {
            service_id: service_id.into(),
            store_id: store_id.into(),
            store_owner_service_id: store_owner_service_id.into(),
            values: serde_json::json!({}),
        }
    }

    #[must_use]
    pub fn with_values(mut self, values: serde_json::Value) -> Self {
        self.values = values;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedServiceRuntime {
    pub service_id: String,
    pub api_workload_id: String,
    pub migration_workload_id: String,
    pub store_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeErrorCode {
    InvalidServiceDefinition,
    ServiceIdentityMismatch,
    MissingApiWorkload,
    MissingMigrationWorkload,
    AmbiguousApiWorkload,
    AmbiguousMigrationWorkload,
    MissingStore,
    StoreOwnerMismatch,
    StoreAlreadyOwned,
    StoreOwnershipCheckFailed,
    MigrationFailed,
    ApiServeFailed,
    MissingConfigValue,
    InvalidConfigValue,
}

pub const SERVICE_RUNTIME_MIGRATIONS: &[Migration] = &[Migration {
    name: "autonomous-service/0001_create_story_segments",
    sql: include_str!("../migrations/0001_create_story_segments.sql"),
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    Starting,
    Ready,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ServiceRuntimeState {
    identity: Arc<ServiceRuntimeIdentity>,
    phase: Arc<RwLock<RuntimePhase>>,
    pool: Option<PgPool>,
}

#[derive(Debug)]
struct ServiceRuntimeIdentity {
    service_id: String,
    api_workload_id: String,
    store_id: String,
    migration_workload_id: String,
}

impl ServiceRuntimeState {
    #[must_use]
    pub fn starting(
        service_id: impl Into<String>,
        api_workload_id: impl Into<String>,
        store_id: impl Into<String>,
        migration_workload_id: impl Into<String>,
    ) -> Self {
        Self {
            identity: Arc::new(ServiceRuntimeIdentity {
                service_id: service_id.into(),
                api_workload_id: api_workload_id.into(),
                store_id: store_id.into(),
                migration_workload_id: migration_workload_id.into(),
            }),
            phase: Arc::new(RwLock::new(RuntimePhase::Starting)),
            pool: None,
        }
    }

    #[must_use]
    pub fn ready(
        service_id: impl Into<String>,
        api_workload_id: impl Into<String>,
        store_id: impl Into<String>,
        migration_workload_id: impl Into<String>,
    ) -> Self {
        let state = Self::starting(service_id, api_workload_id, store_id, migration_workload_id);
        state.set_phase(RuntimePhase::Ready);
        state
    }

    #[must_use]
    pub fn with_store(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn set_phase(&self, phase: RuntimePhase) {
        *self.phase.write().expect("runtime phase lock poisoned") = phase;
    }

    #[must_use]
    pub fn phase(&self) -> RuntimePhase {
        *self.phase.read().expect("runtime phase lock poisoned")
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub service_id: String,
    pub workload_id: String,
    pub store_id: String,
    pub migration_workload_id: String,
    pub phase: RuntimePhase,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceBootFailure {
    pub health: RuntimeHealth,
    pub error: RuntimeError,
}

/// Applies Service-owned migrations before marking the API Workload ready.
pub async fn prepare_runtime(
    contract: &AutonomousServiceContract,
    config: &ServiceRuntimeConfig,
    pool: PgPool,
    module_migrations: &[Migration],
) -> Result<ServiceRuntimeState, RuntimeError> {
    let validated = validate_runtime(contract, config)?;
    let state = ServiceRuntimeState::starting(
        &validated.service_id,
        &validated.api_workload_id,
        &validated.store_id,
        &validated.migration_workload_id,
    )
    .with_store(pool.clone());
    if let Err(error) = apply_migrations(&pool, SERVICE_RUNTIME_MIGRATIONS).await {
        state.set_phase(RuntimePhase::Failed);
        return Err(runtime_error(
            RuntimeErrorCode::MigrationFailed,
            format!("Service-owned migration failed: {}", error.public_message),
            format!(
                "Verify Store `{}` connectivity and migration compatibility, then restart Service `{}`.",
                validated.store_id, validated.service_id
            ),
        ));
    }
    claim_store_ownership(&pool, &validated).await?;
    if let Err(error) = apply_migrations(&pool, module_migrations).await {
        state.set_phase(RuntimePhase::Failed);
        return Err(runtime_error(
            RuntimeErrorCode::MigrationFailed,
            format!(
                "Service-owned module migration failed: {}",
                error.public_message
            ),
            format!(
                "Verify Store `{}` migration compatibility, then restart Service `{}`.",
                validated.store_id, validated.service_id
            ),
        ));
    }
    state.set_phase(RuntimePhase::Ready);
    Ok(state)
}

/// Boots Migration and API Workloads directly from one Service v2 definition.
pub async fn boot(
    contract: &AutonomousServiceContract,
    config: &ServiceRuntimeConfig,
    pool: PgPool,
    module_migrations: &[Migration],
    business: OpenApiRouter<ServiceRuntimeState>,
    listener: tokio::net::TcpListener,
    shutdown: platform_core::Shutdown,
) -> Result<(), ServiceBootFailure> {
    let state = prepare_runtime(contract, config, pool, module_migrations)
        .await
        .map_err(|error| ServiceBootFailure {
            health: failed_runtime_health(contract, config),
            error,
        })?;
    let failure_state = state.clone();
    serve(listener, business, state, shutdown)
        .await
        .map_err(|error| {
            failure_state.set_phase(RuntimePhase::Failed);
            ServiceBootFailure {
                health: runtime_health(&failure_state),
                error: runtime_error(
                    RuntimeErrorCode::ApiServeFailed,
                    format!("API Workload failed: {error}"),
                    format!(
                        "Verify the API listener for Service `{}` and restart it.",
                        contract.service_id
                    ),
                ),
            }
        })
}

fn failed_runtime_health(
    contract: &AutonomousServiceContract,
    config: &ServiceRuntimeConfig,
) -> RuntimeHealth {
    let workload = |role| {
        contract
            .workloads
            .iter()
            .find(|workload| workload.role == role)
            .map_or_else(
                || "unresolved".to_owned(),
                |workload| workload.workload_id.clone(),
            )
    };
    RuntimeHealth {
        service_id: contract.service_id.clone(),
        workload_id: workload(WorkloadRole::API),
        store_id: config.store_id.clone(),
        migration_workload_id: workload(WorkloadRole::MIGRATION),
        phase: RuntimePhase::Failed,
    }
}

async fn claim_store_ownership(
    pool: &PgPool,
    runtime: &ValidatedServiceRuntime,
) -> Result<(), RuntimeError> {
    sqlx::query(
        r#"
        insert into platform.service_store_ownership (store_id, service_id)
        values ($1, $2)
        on conflict (store_id) do nothing
        "#,
    )
    .bind(&runtime.store_id)
    .bind(&runtime.service_id)
    .execute(pool)
    .await
    .map_err(|error| {
        runtime_error(
            RuntimeErrorCode::StoreOwnershipCheckFailed,
            format!("could not claim Service Store ownership: {error}"),
            "Verify the Service Store migration state and retry.",
        )
    })?;
    let owner: String = sqlx::query_scalar(
        "select service_id from platform.service_store_ownership where store_id = $1",
    )
    .bind(&runtime.store_id)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        runtime_error(
            RuntimeErrorCode::StoreOwnershipCheckFailed,
            format!("could not verify Service Store ownership: {error}"),
            "Verify the Service Store migration state and retry.",
        )
    })?;
    if owner != runtime.service_id {
        return Err(runtime_error(
            RuntimeErrorCode::StoreAlreadyOwned,
            format!(
                "Store `{}` is already owned by Service `{owner}`",
                runtime.store_id
            ),
            "Use a Store that is isolated for this Service.",
        ));
    }
    Ok(())
}

/// Mounts public Service health surfaces and Story Segment persistence around business routes.
#[must_use]
pub fn service_router(
    business: OpenApiRouter<ServiceRuntimeState>,
    state: ServiceRuntimeState,
) -> Router {
    OpenApiRouter::with_openapi(ServiceRuntimeApi::openapi())
        .merge(runtime_router())
        .merge(business)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            persist_story_segment,
        ))
        .with_state(state)
        .split_for_parts()
        .0
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Lenso Autonomous Service Runtime API",
        version = "1.0.0",
        description = "Service-owned health and local Story Segment surfaces"
    ),
    components(schemas(
        RuntimeHealth,
        RuntimePhase,
        StorySegment,
        platform_http::ErrorResponse,
        platform_http::ProblemErrorDetail
    ))
)]
struct ServiceRuntimeApi;

fn runtime_router() -> OpenApiRouter<ServiceRuntimeState> {
    OpenApiRouter::new()
        .routes(routes!(liveness))
        .routes(routes!(readiness))
        .routes(routes!(startup))
        .routes(routes!(story_segments))
}

#[must_use]
pub fn openapi_document() -> utoipa::openapi::OpenApi {
    OpenApiRouter::<ServiceRuntimeState>::with_openapi(ServiceRuntimeApi::openapi())
        .merge(runtime_router())
        .to_openapi()
}

/// Serves one API Workload and performs a deterministic phase transition on shutdown.
pub async fn serve(
    listener: tokio::net::TcpListener,
    business: OpenApiRouter<ServiceRuntimeState>,
    state: ServiceRuntimeState,
    shutdown: platform_core::Shutdown,
) -> std::io::Result<()> {
    let app = service_router(business, state.clone());
    let stopping_state = state.clone();
    let mut receiver = shutdown.subscribe();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if !*receiver.borrow() {
                let _ = receiver.changed().await;
            }
            stopping_state.set_phase(RuntimePhase::Stopping);
        })
        .await?;
    state.set_phase(RuntimePhase::Stopped);
    Ok(())
}

#[utoipa::path(get, path = "/health/live", responses((status = 200, body = RuntimeHealth)), tag = "service-runtime")]
async fn liveness(State(state): State<ServiceRuntimeState>) -> (StatusCode, Json<RuntimeHealth>) {
    health_response(&state, state.phase() != RuntimePhase::Stopped)
}

#[utoipa::path(get, path = "/health/ready", responses((status = 200, body = RuntimeHealth), (status = 503, body = RuntimeHealth)), tag = "service-runtime")]
async fn readiness(State(state): State<ServiceRuntimeState>) -> (StatusCode, Json<RuntimeHealth>) {
    health_response(&state, state.phase() == RuntimePhase::Ready)
}

#[utoipa::path(get, path = "/health/startup", responses((status = 200, body = RuntimeHealth), (status = 503, body = RuntimeHealth)), tag = "service-runtime")]
async fn startup(State(state): State<ServiceRuntimeState>) -> (StatusCode, Json<RuntimeHealth>) {
    health_response(
        &state,
        matches!(state.phase(), RuntimePhase::Ready | RuntimePhase::Stopping),
    )
}

fn health_response(
    state: &ServiceRuntimeState,
    healthy: bool,
) -> (StatusCode, Json<RuntimeHealth>) {
    (
        if healthy {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(runtime_health(state)),
    )
}

fn runtime_health(state: &ServiceRuntimeState) -> RuntimeHealth {
    RuntimeHealth {
        service_id: state.identity.service_id.clone(),
        workload_id: state.identity.api_workload_id.clone(),
        store_id: state.identity.store_id.clone(),
        migration_workload_id: state.identity.migration_workload_id.clone(),
        phase: state.phase(),
    }
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StorySegment {
    pub segment_id: String,
    pub service_id: String,
    pub workload_id: String,
    pub operation: String,
    pub status: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/runtime/story-segments",
    responses(
        (status = 200, body = [StorySegment]),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 500, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn story_segments(
    State(state): State<ServiceRuntimeState>,
) -> Result<Json<Vec<StorySegment>>, platform_http::ApiErrorResponse> {
    let Some(pool) = &state.pool else {
        return Err(platform_core::AppError::new(
            platform_core::ErrorCode::ExternalDependency,
            "Service Store is not available",
        )
        .into());
    };
    sqlx::query_as::<_, StorySegment>(
        r#"
        select segment_id, service_id, workload_id, operation, status, started_at, completed_at
        from platform.service_story_segments
        where service_id = $1
        order by completed_at desc, segment_id
        limit 100
        "#,
    )
    .bind(&state.identity.service_id)
    .fetch_all(pool)
    .await
    .map(Json)
    .map_err(|error| {
        platform_http::ApiErrorResponse::from(
            platform_core::AppError::new(
                platform_core::ErrorCode::Internal,
                "Could not read local Story Segments",
            )
            .with_source(error),
        )
    })
}

async fn persist_story_segment(
    State(state): State<ServiceRuntimeState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    if response.status().is_success()
        && !path.starts_with("/health/")
        && path != "/runtime/story-segments"
    {
        let Some(pool) = &state.pool else {
            return platform_http::ApiErrorResponse::from(platform_core::AppError::new(
                platform_core::ErrorCode::Internal,
                "Successful operation has no Service Store for local Story Segment persistence",
            ))
            .into_response();
        };
        let now = chrono::Utc::now();
        if let Err(error) = sqlx::query(
            r#"
            insert into platform.service_story_segments (
                segment_id, service_id, workload_id, operation, status, started_at, completed_at
            ) values ($1, $2, $3, $4, 'succeeded', $5, $5)
            "#,
        )
        .bind(Uuid::now_v7().to_string())
        .bind(&state.identity.service_id)
        .bind(&state.identity.api_workload_id)
        .bind(format!("{method} {path}"))
        .bind(now)
        .execute(pool)
        .await
        {
            return platform_http::ApiErrorResponse::from(
                platform_core::AppError::new(
                    platform_core::ErrorCode::Internal,
                    "Successful operation could not persist its local Story Segment",
                )
                .with_source(error),
            )
            .into_response();
        }
    }
    response
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[error("{message}")]
pub struct RuntimeError {
    pub code: RuntimeErrorCode,
    pub message: String,
    pub next_action: String,
}

pub fn validate_runtime(
    contract: &AutonomousServiceContract,
    config: &ServiceRuntimeConfig,
) -> Result<ValidatedServiceRuntime, RuntimeError> {
    let issues = validate_autonomous_service_contract(contract);
    if let Some(issue) = issues.first() {
        return Err(runtime_error(
            RuntimeErrorCode::InvalidServiceDefinition,
            format!("invalid Service v2 definition: {}", issue.message),
            issue.next_action.clone(),
        ));
    }
    if config.service_id != contract.service_id {
        return Err(runtime_error(
            RuntimeErrorCode::ServiceIdentityMismatch,
            "runtime Service identity does not match the Service v2 definition",
            format!(
                "Set the runtime Service identity to `{}`.",
                contract.service_id
            ),
        ));
    }
    validate_config_values(contract, &config.values)?;

    let api_workload_id = one_workload(contract, WorkloadRole::API)?;
    let migration_workload_id = one_workload(contract, WorkloadRole::MIGRATION)?;
    let store = contract
        .stores
        .iter()
        .find(|store| store.store_id == config.store_id)
        .ok_or_else(|| {
            runtime_error(
                RuntimeErrorCode::MissingStore,
                "runtime Store is not declared by the Service",
                format!(
                    "Declare Store `{}` in Service `{}` or select a declared Store.",
                    config.store_id, contract.service_id
                ),
            )
        })?;
    if store.service_id != contract.service_id
        || config.store_owner_service_id != contract.service_id
    {
        return Err(runtime_error(
            RuntimeErrorCode::StoreOwnerMismatch,
            "runtime Store ownership does not match the Service identity",
            format!(
                "Set the runtime Store owner to Service `{}`.",
                contract.service_id
            ),
        ));
    }

    Ok(ValidatedServiceRuntime {
        service_id: contract.service_id.clone(),
        api_workload_id,
        migration_workload_id,
        store_id: store.store_id.clone(),
    })
}

fn validate_config_values(
    contract: &AutonomousServiceContract,
    values: &serde_json::Value,
) -> Result<(), RuntimeError> {
    let Some(config_contract) = &contract.config_contract else {
        return Ok(());
    };
    for field in &config_contract.fields {
        let value = field
            .path
            .split('.')
            .try_fold(values, |current, segment| current.get(segment))
            .ok_or_else(|| {
                runtime_error(
                    RuntimeErrorCode::MissingConfigValue,
                    format!("required Service configuration `{}` is missing", field.path),
                    format!("Set `{}` before starting the Service.", field.path),
                )
            })?;
        let valid = match field.shape.as_str() {
            "positive_integer" => value.as_u64().is_some_and(|value| value > 0),
            "secret_reference" | "string" => {
                value.as_str().is_some_and(|value| !value.trim().is_empty())
            }
            "boolean" => value.is_boolean(),
            _ => false,
        };
        if !valid {
            return Err(runtime_error(
                RuntimeErrorCode::InvalidConfigValue,
                format!(
                    "Service configuration `{}` does not match `{}`",
                    field.path, field.shape
                ),
                format!(
                    "Set `{}` to a value matching `{}`.",
                    field.path, field.shape
                ),
            ));
        }
    }
    Ok(())
}

fn one_workload(
    contract: &AutonomousServiceContract,
    role: WorkloadRole,
) -> Result<String, RuntimeError> {
    let ids = contract
        .workloads
        .iter()
        .filter(|workload| workload.role == role)
        .map(|workload| workload.workload_id.clone())
        .collect::<Vec<_>>();
    match (role, ids.as_slice()) {
        (_, [id]) => Ok(id.clone()),
        (WorkloadRole::Api, []) => Err(runtime_error(
            RuntimeErrorCode::MissingApiWorkload,
            "Service has no API Workload",
            "Declare exactly one API Workload in the Service v2 definition.",
        )),
        (WorkloadRole::Migration, []) => Err(runtime_error(
            RuntimeErrorCode::MissingMigrationWorkload,
            "Service has no Migration Workload",
            "Declare exactly one Migration Workload in the Service v2 definition.",
        )),
        (WorkloadRole::Api, _) => Err(runtime_error(
            RuntimeErrorCode::AmbiguousApiWorkload,
            "Service has more than one API Workload",
            "Declare exactly one API Workload for this runtime profile.",
        )),
        (WorkloadRole::Migration, _) => Err(runtime_error(
            RuntimeErrorCode::AmbiguousMigrationWorkload,
            "Service has more than one Migration Workload",
            "Declare exactly one Migration Workload for this runtime profile.",
        )),
        _ => unreachable!("runtime validates only API and Migration roles"),
    }
}

fn runtime_error(
    code: RuntimeErrorCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> RuntimeError {
    RuntimeError {
        code,
        message: message.into(),
        next_action: next_action.into(),
    }
}

#[cfg(test)]
mod tests {
    use axum::routing::get;
    use http::{Request, StatusCode};
    use lenso_service::{
        AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
        ConfigActivation, ConfigContract, ConfigFieldContract, ConfigMutability, ConfigScope,
        SchemaArtifactReference, ServiceTenancyMode, WorkloadRole,
    };

    use super::{RuntimeErrorCode, ServiceRuntimeConfig, validate_runtime};
    use super::{ServiceRuntimeState, service_router};
    use tower::ServiceExt as _;
    use utoipa_axum::router::OpenApiRouter;

    fn service() -> AutonomousServiceContract {
        let mut service = AutonomousServiceContract::new(
            "support",
            vec![
                AutonomousServiceWorkload::new("support-api", "support", WorkloadRole::API),
                AutonomousServiceWorkload::new(
                    "support-migrate",
                    "support",
                    WorkloadRole::MIGRATION,
                ),
            ],
            ServiceTenancyMode::None,
            vec!["local".to_owned()],
        );
        service.stores = vec![AutonomousServiceStore::new("primary", "support")];
        service
    }

    #[test]
    fn valid_service_composes_api_and_migration_under_one_identity() {
        let runtime = validate_runtime(
            &service(),
            &ServiceRuntimeConfig::new("support", "primary", "support"),
        )
        .expect("valid runtime");

        assert_eq!(runtime.service_id, "support");
        assert_eq!(runtime.api_workload_id, "support-api");
        assert_eq!(runtime.migration_workload_id, "support-migrate");
        assert_eq!(runtime.store_id, "primary");
    }

    #[test]
    fn incoherent_store_ownership_has_a_stable_code_and_next_action() {
        let error = validate_runtime(
            &service(),
            &ServiceRuntimeConfig::new("support", "primary", "billing"),
        )
        .expect_err("store owner must match");

        assert_eq!(error.code, RuntimeErrorCode::StoreOwnerMismatch);
        assert_eq!(
            error.next_action,
            "Set the runtime Store owner to Service `support`."
        );
    }

    #[test]
    fn missing_declared_configuration_fails_before_startup() {
        let mut definition = service();
        definition.config_contract = Some(ConfigContract::new(
            "support-config",
            "v1",
            SchemaArtifactReference::new("contracts/config/support.v1.schema.json"),
            vec![ConfigFieldContract {
                path: "sla.defaultHours".to_owned(),
                shape: "positive_integer".to_owned(),
                sensitive: false,
                scope: ConfigScope::Service,
                mutability: ConfigMutability::Mutable,
                activation: ConfigActivation::Restart,
            }],
        ));

        let error = validate_runtime(
            &definition,
            &ServiceRuntimeConfig::new("support", "primary", "support"),
        )
        .expect_err("missing config must fail");

        assert_eq!(error.code, RuntimeErrorCode::MissingConfigValue);
        assert_eq!(
            error.next_action,
            "Set `sla.defaultHours` before starting the Service."
        );
    }

    #[tokio::test]
    async fn public_health_surfaces_report_service_readiness() {
        let state =
            ServiceRuntimeState::ready("support", "support-api", "primary", "support-migrate");
        let app = service_router(
            OpenApiRouter::new().route("/tickets", get(|| async { "ok" })),
            state,
        );

        let live = app
            .clone()
            .oneshot(
                Request::get("/health/live")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let ready = app
            .oneshot(
                Request::get("/health/ready")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(live.status(), StatusCode::OK);
        assert_eq!(ready.status(), StatusCode::OK);
    }

    #[test]
    fn public_runtime_surfaces_are_in_openapi() {
        let document = super::openapi_document();

        assert!(document.paths.paths.contains_key("/health/live"));
        assert!(document.paths.paths.contains_key("/health/ready"));
        assert!(document.paths.paths.contains_key("/health/startup"));
        assert!(document.paths.paths.contains_key("/runtime/story-segments"));
    }

    #[tokio::test]
    async fn shutdown_transitions_service_to_stopped() {
        let state =
            ServiceRuntimeState::ready("support", "support-api", "primary", "support-migrate");
        let shutdown = platform_core::Shutdown::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let task_state = state.clone();
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move {
            super::serve(listener, OpenApiRouter::new(), task_state, task_shutdown)
                .await
                .unwrap();
        });

        shutdown.signal();
        task.await.unwrap();

        assert_eq!(state.phase(), super::RuntimePhase::Stopped);
    }

    #[tokio::test]
    async fn boot_failure_exposes_failed_startup_status_and_action() {
        let mut definition = service();
        definition
            .workloads
            .retain(|workload| workload.role != WorkloadRole::MIGRATION);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1/unused")
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let failure = super::boot(
            &definition,
            &ServiceRuntimeConfig::new("support", "primary", "support"),
            pool,
            &[],
            OpenApiRouter::new(),
            listener,
            platform_core::Shutdown::new(),
        )
        .await
        .expect_err("missing Migration Workload must fail startup");

        assert_eq!(failure.health.phase, super::RuntimePhase::Failed);
        assert_eq!(
            failure.error.code,
            RuntimeErrorCode::MissingMigrationWorkload
        );
        assert!(!failure.error.next_action.is_empty());
    }
}
