//! Host-independent runtime composition for one `lenso.service.v2` Service.

mod operations;
mod transport;
mod transport_nats_jetstream;
mod workflow;
mod workflow_child;
mod workflow_compensation;

pub use operations::*;
pub use transport::*;
pub use transport_nats_jetstream::*;
pub use workflow::*;
pub use workflow_child::*;
pub use workflow_compensation::*;

use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse as _, Response},
};
use lenso_contracts::{ModuleManifest, WORKFLOW_DEFINITION_PROTOCOL, WorkflowDefinition};
use lenso_service::{
    AutonomousServiceContract, EventContractArtifact, ServiceTenancyMode, WorkloadRole,
    validate_autonomous_service_contract,
};
use platform_core::{
    Clock, EventHandlerRegistry, Migration, OutboxRelay, SystemClock, apply_migrations,
};
use platform_runtime::{FunctionRegistry, RuntimeWorker};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
    time::Duration,
};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRuntimeConfig {
    pub service_id: String,
    pub store_id: String,
    pub store_owner_service_id: String,
    pub operator_environment: DeadLetterOperatorEnvironment,
    pub values: serde_json::Value,
    pub workflow_definitions: Vec<WorkflowDefinition>,
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
            operator_environment: DeadLetterOperatorEnvironment::LocalSandbox,
            values: serde_json::json!({}),
            workflow_definitions: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_values(mut self, values: serde_json::Value) -> Self {
        self.values = values;
        self
    }

    #[must_use]
    pub fn with_workflow_definitions(
        mut self,
        workflow_definitions: Vec<WorkflowDefinition>,
    ) -> Self {
        self.workflow_definitions = workflow_definitions;
        self
    }

    /// Collects Durable Workflow declarations from the composing Modules.
    #[must_use]
    pub fn with_module_manifests(mut self, manifests: &[ModuleManifest]) -> Self {
        self.workflow_definitions = manifests
            .iter()
            .filter_map(|manifest| manifest.runtime.as_ref())
            .flat_map(|runtime| runtime.workflows.iter().cloned())
            .collect();
        self
    }

    #[must_use]
    pub const fn with_operator_environment(
        mut self,
        operator_environment: DeadLetterOperatorEnvironment,
    ) -> Self {
        self.operator_environment = operator_environment;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedServiceRuntime {
    pub service_id: String,
    pub api_workload_id: String,
    pub migration_workload_id: String,
    pub worker_workload_id: String,
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
    MissingWorkerWorkload,
    AmbiguousWorkerWorkload,
    MissingStore,
    StoreOwnerMismatch,
    StoreAlreadyOwned,
    StoreOwnershipCheckFailed,
    MigrationFailed,
    ApiServeFailed,
    WorkerRunFailed,
    MissingConfigValue,
    InvalidConfigValue,
    InvalidWorkflowDefinition,
    DuplicateWorkflowDefinition,
    WorkflowOwnerNotDeclared,
}

pub const SERVICE_RUNTIME_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "autonomous-service/0001_create_story_segments",
        sql: include_str!("../migrations/0001_create_story_segments.sql"),
    },
    Migration {
        name: "autonomous-service/0002_create_worker_runtime",
        sql: include_str!("../migrations/0002_create_worker_runtime.sql"),
    },
    Migration {
        name: "autonomous-service/0003_create_event_delivery",
        sql: include_str!("../migrations/0003_create_event_delivery.sql"),
    },
    Migration {
        name: "autonomous-service/0005_make_event_inbox_idempotent",
        sql: include_str!("../migrations/0005_make_event_inbox_idempotent.sql"),
    },
    Migration {
        name: "autonomous-service/0006_classify_event_delivery_failures",
        sql: include_str!("../migrations/0006_classify_event_delivery_failures.sql"),
    },
    Migration {
        name: "autonomous-service/0008_manage_dead_letter_replays",
        sql: include_str!("../migrations/0008_manage_dead_letter_replays.sql"),
    },
    Migration {
        name: "autonomous-service/0010_create_durable_workflows",
        sql: include_str!("../migrations/0010_create_durable_workflows.sql"),
    },
    Migration {
        name: "autonomous-service/0011_advance_durable_workflow_steps",
        sql: include_str!("../migrations/0011_advance_durable_workflow_steps.sql"),
    },
    Migration {
        name: "autonomous-service/0012_run_child_workflows",
        sql: include_str!("../migrations/0012_run_child_workflows.sql"),
    },
    Migration {
        name: "autonomous-service/0013_recover_workflow_retries_and_timers",
        sql: include_str!("../migrations/0013_recover_workflow_retries_and_timers.sql"),
    },
    Migration {
        name: "autonomous-service/0014_compensate_workflow_effects",
        sql: include_str!("../migrations/0014_compensate_workflow_effects.sql"),
    },
    Migration {
        name: "autonomous-service/0015_create_workflow_compensations",
        sql: include_str!("../migrations/0015_create_workflow_compensations.sql"),
    },
    Migration {
        name: "autonomous-service/0016_create_workflow_compensation_history",
        sql: include_str!("../migrations/0016_create_workflow_compensation_history.sql"),
    },
];

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
    worker_phase: Arc<RwLock<RuntimePhase>>,
    pool: Option<PgPool>,
    workflow_definitions: Arc<Vec<WorkflowDefinition>>,
    event_contracts: Arc<Vec<EventContractArtifact>>,
    workflow_clock: Arc<dyn Clock>,
}

#[derive(Debug, Clone)]
struct ServiceRuntimeIdentity {
    service_id: String,
    api_workload_id: String,
    store_id: String,
    migration_workload_id: String,
    worker_workload_id: String,
    operator_environment: DeadLetterOperatorEnvironment,
    tenancy_mode: ServiceTenancyMode,
}

impl ServiceRuntimeState {
    #[must_use]
    pub fn starting(
        service_id: impl Into<String>,
        api_workload_id: impl Into<String>,
        store_id: impl Into<String>,
        migration_workload_id: impl Into<String>,
        worker_workload_id: impl Into<String>,
    ) -> Self {
        Self {
            identity: Arc::new(ServiceRuntimeIdentity {
                service_id: service_id.into(),
                api_workload_id: api_workload_id.into(),
                store_id: store_id.into(),
                migration_workload_id: migration_workload_id.into(),
                worker_workload_id: worker_workload_id.into(),
                operator_environment: DeadLetterOperatorEnvironment::LocalSandbox,
                tenancy_mode: ServiceTenancyMode::None,
            }),
            phase: Arc::new(RwLock::new(RuntimePhase::Starting)),
            worker_phase: Arc::new(RwLock::new(RuntimePhase::Starting)),
            pool: None,
            workflow_definitions: Arc::new(Vec::new()),
            event_contracts: Arc::new(Vec::new()),
            workflow_clock: Arc::new(SystemClock),
        }
    }

    #[must_use]
    pub fn ready(
        service_id: impl Into<String>,
        api_workload_id: impl Into<String>,
        store_id: impl Into<String>,
        migration_workload_id: impl Into<String>,
        worker_workload_id: impl Into<String>,
    ) -> Self {
        let state = Self::starting(
            service_id,
            api_workload_id,
            store_id,
            migration_workload_id,
            worker_workload_id,
        );
        state.set_phase(RuntimePhase::Ready);
        state.set_worker_phase(RuntimePhase::Ready);
        state
    }

    #[must_use]
    pub fn with_store(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    fn with_operator_environment(
        mut self,
        operator_environment: DeadLetterOperatorEnvironment,
    ) -> Self {
        Arc::make_mut(&mut self.identity).operator_environment = operator_environment;
        self
    }

    fn with_tenancy_mode(mut self, tenancy_mode: ServiceTenancyMode) -> Self {
        Arc::make_mut(&mut self.identity).tenancy_mode = tenancy_mode;
        self
    }

    fn with_workflow_definitions(mut self, workflow_definitions: Vec<WorkflowDefinition>) -> Self {
        self.workflow_definitions = Arc::new(workflow_definitions);
        self
    }

    fn with_event_contracts(mut self, event_contracts: Vec<EventContractArtifact>) -> Self {
        self.event_contracts = Arc::new(event_contracts);
        self
    }

    /// Overrides wall-clock time for deterministic System Sandbox workflow
    /// timers. Production composition should keep the default System clock.
    #[must_use]
    pub fn with_workflow_clock(mut self, workflow_clock: Arc<dyn Clock>) -> Self {
        self.workflow_clock = workflow_clock;
        self
    }

    pub fn set_phase(&self, phase: RuntimePhase) {
        *self.phase.write().expect("runtime phase lock poisoned") = phase;
    }

    #[must_use]
    pub fn phase(&self) -> RuntimePhase {
        *self.phase.read().expect("runtime phase lock poisoned")
    }

    #[must_use]
    pub fn worker_phase(&self) -> RuntimePhase {
        *self
            .worker_phase
            .read()
            .expect("worker phase lock poisoned")
    }

    fn set_worker_phase(&self, phase: RuntimePhase) {
        *self
            .worker_phase
            .write()
            .expect("worker phase lock poisoned") = phase;
    }

    fn store(&self) -> Result<&PgPool, platform_core::AppError> {
        self.pool.as_ref().ok_or_else(|| {
            platform_core::AppError::new(
                platform_core::ErrorCode::ExternalDependency,
                "Service Store is not available",
            )
        })
    }

    fn worker_id(&self) -> String {
        format!(
            "{}/{}",
            self.identity.service_id, self.identity.worker_workload_id
        )
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub service_id: String,
    pub workload_id: String,
    pub store_id: String,
    pub migration_workload_id: String,
    pub worker_workload_id: String,
    pub phase: RuntimePhase,
    pub worker_phase: RuntimePhase,
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
        &validated.worker_workload_id,
    )
    .with_store(pool.clone())
    .with_operator_environment(config.operator_environment)
    .with_tenancy_mode(contract.tenancy_mode.clone())
    .with_workflow_definitions(config.workflow_definitions.clone())
    .with_event_contracts(contract.event_contracts.clone());
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
    if let Err(error) = apply_migrations(&pool, platform_core::PLATFORM_MIGRATIONS).await {
        state.set_phase(RuntimePhase::Failed);
        return Err(runtime_error(
            RuntimeErrorCode::MigrationFailed,
            format!(
                "Service-owned platform migration failed: {}",
                error.public_message
            ),
            format!(
                "Verify Store `{}` platform migration compatibility, then restart Service `{}`.",
                validated.store_id, validated.service_id
            ),
        ));
    }
    if let Err(error) = apply_migrations(&pool, platform_runtime::RUNTIME_MIGRATIONS).await {
        state.set_phase(RuntimePhase::Failed);
        return Err(runtime_error(
            RuntimeErrorCode::MigrationFailed,
            format!(
                "Service-owned runtime migration failed: {}",
                error.public_message
            ),
            format!(
                "Verify Store `{}` runtime migration compatibility, then restart Service `{}`.",
                validated.store_id, validated.service_id
            ),
        ));
    }
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

/// Releases claims owned by this Service's Worker Workload without touching other workers.
pub async fn release_worker_claims(
    state: &ServiceRuntimeState,
) -> Result<(), platform_core::AppError> {
    let pool = state.store()?;
    let worker_id = state.worker_id();
    let mut transaction = pool.begin().await.map_err(worker_store_error)?;
    sqlx::query(
        r#"
        update platform.outbox
        set status = 'pending', locked_at = null, locked_by = null
        where status = 'processing' and locked_by = $1
        "#,
    )
    .bind(&worker_id)
    .execute(&mut *transaction)
    .await
    .map_err(worker_store_error)?;
    sqlx::query(
        r#"
        update runtime.function_runs
        set status = 'pending', locked_at = null, locked_by = null, updated_at = now()
        where status = 'processing' and locked_by = $1
        "#,
    )
    .bind(&worker_id)
    .execute(&mut *transaction)
    .await
    .map_err(worker_store_error)?;
    transaction.commit().await.map_err(worker_store_error)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceWorkerConfig {
    pub poll_interval: Duration,
    pub batch_size: i64,
}

impl Default for ServiceWorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(500),
            batch_size: 25,
        }
    }
}

/// Runs the Service-owned Worker Workload until the shared Service shutdown is signalled.
pub async fn run_worker(
    state: ServiceRuntimeState,
    function_registry: Arc<FunctionRegistry>,
    event_handlers: EventHandlerRegistry,
    config: ServiceWorkerConfig,
    shutdown: platform_core::Shutdown,
) -> Result<(), platform_core::AppError> {
    let pool = state.store()?.clone();
    let worker_id = state.worker_id();
    persist_worker_phase(&state, RuntimePhase::Ready).await?;
    let relay = OutboxRelay::new(pool.clone(), &worker_id);
    let worker = RuntimeWorker::new(pool, function_registry, &worker_id);
    let mut receiver = shutdown.subscribe();

    let run_result = loop {
        tokio::select! {
            changed = receiver.changed() => {
                if changed.is_err() || *receiver.borrow() {
                    break Ok(());
                }
            }
            () = tokio::time::sleep(config.poll_interval) => {
                if let Err(error) = relay.relay_once(&event_handlers, config.batch_size).await {
                    break Err(error);
                }
                if let Err(error) = worker.claim_and_run_batch(config.batch_size).await {
                    break Err(error);
                }
                if let Err(error) = project_background_story_segments(&state).await {
                    break Err(error);
                }
            }
        }
    };

    let mut result = run_result;
    record_cleanup_result(
        &mut result,
        persist_worker_phase(&state, RuntimePhase::Stopping).await,
    );
    record_cleanup_result(&mut result, release_worker_claims(&state).await);
    record_cleanup_result(&mut result, project_background_story_segments(&state).await);
    let final_phase = if result.is_ok() {
        RuntimePhase::Stopped
    } else {
        RuntimePhase::Failed
    };
    record_cleanup_result(&mut result, persist_worker_phase(&state, final_phase).await);
    result
}

fn record_cleanup_result(
    result: &mut Result<(), platform_core::AppError>,
    cleanup: Result<(), platform_core::AppError>,
) {
    if result.is_ok() {
        *result = cleanup;
    }
}

async fn persist_worker_phase(
    state: &ServiceRuntimeState,
    phase: RuntimePhase,
) -> Result<(), platform_core::AppError> {
    state.set_worker_phase(phase);
    let pool = state.store()?;
    sqlx::query(
        r#"
        insert into platform.service_worker_health (service_id, workload_id, phase)
        values ($1, $2, $3)
        on conflict (service_id, workload_id) do update
        set phase = excluded.phase, updated_at = now()
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(&state.identity.worker_workload_id)
    .bind(format!("{phase:?}").to_lowercase())
    .execute(pool)
    .await
    .map_err(worker_store_error)?;
    Ok(())
}

async fn project_background_story_segments(
    state: &ServiceRuntimeState,
) -> Result<(), platform_core::AppError> {
    let pool = state.store()?;
    sqlx::query(
        r#"
        insert into platform.service_story_segments (
            segment_id, service_id, workload_id, operation, status, started_at, completed_at
        )
        select
            concat('function:', id, ':', status, ':', attempts), $1, $2,
            concat('function ', function_name), status,
            coalesce(started_at, created_at), coalesce(completed_at, updated_at)
        from runtime.function_runs
        where status in ('completed', 'failed', 'dead')
        on conflict (segment_id) do nothing
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(&state.identity.worker_workload_id)
    .execute(pool)
    .await
    .map_err(worker_store_error)?;
    sqlx::query(
        r#"
        insert into platform.service_story_segments (
            segment_id, service_id, workload_id, operation, status, started_at, completed_at
        )
        select
            concat('event:', id, ':', status, ':', attempts), $1, $2,
            concat('event ', event_name), status,
            created_at, coalesce(published_at, available_at)
        from platform.outbox
        where status in ('published', 'failed', 'dead')
        on conflict (segment_id) do nothing
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(&state.identity.worker_workload_id)
    .execute(pool)
    .await
    .map_err(worker_store_error)?;
    Ok(())
}

fn worker_store_error(error: sqlx::Error) -> platform_core::AppError {
    platform_core::AppError::new(
        platform_core::ErrorCode::Internal,
        "Service Worker Store operation failed",
    )
    .with_source(error)
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
    boot_with_worker(
        contract,
        config,
        pool,
        module_migrations,
        business,
        Arc::new(FunctionRegistry::default()),
        EventHandlerRegistry::default(),
        ServiceWorkerConfig::default(),
        listener,
        shutdown,
    )
    .await
}

/// Boots API, Migration, and Worker Workloads under one Service identity.
pub async fn boot_with_worker(
    contract: &AutonomousServiceContract,
    config: &ServiceRuntimeConfig,
    pool: PgPool,
    module_migrations: &[Migration],
    business: OpenApiRouter<ServiceRuntimeState>,
    function_registry: Arc<FunctionRegistry>,
    event_handlers: EventHandlerRegistry,
    worker_config: ServiceWorkerConfig,
    listener: tokio::net::TcpListener,
    shutdown: platform_core::Shutdown,
) -> Result<(), ServiceBootFailure> {
    let state = prepare_runtime(contract, config, pool, module_migrations)
        .await
        .map_err(|error| ServiceBootFailure {
            health: failed_runtime_health(contract, config),
            error,
        })?;
    let api_state = state.clone();
    let worker_state = state.clone();
    let api_shutdown = shutdown.clone();
    let worker_shutdown = shutdown.clone();
    let api = serve(listener, business, api_state, api_shutdown);
    let worker = run_worker(
        worker_state,
        function_registry,
        event_handlers,
        worker_config,
        worker_shutdown,
    );
    tokio::pin!(api);
    tokio::pin!(worker);

    tokio::select! {
        result = &mut api => {
            shutdown.signal();
            let worker_result = worker.await;
            result.map_err(|error| api_boot_failure(&state, error))?;
            worker_result.map_err(|error| worker_boot_failure(&state, error))?;
        }
        result = &mut worker => {
            shutdown.signal();
            let api_result = api.await;
            result.map_err(|error| worker_boot_failure(&state, error))?;
            api_result.map_err(|error| api_boot_failure(&state, error))?;
        }
    }
    Ok(())
}

fn api_boot_failure(state: &ServiceRuntimeState, error: std::io::Error) -> ServiceBootFailure {
    state.set_phase(RuntimePhase::Failed);
    ServiceBootFailure {
        health: runtime_health(state),
        error: runtime_error(
            RuntimeErrorCode::ApiServeFailed,
            format!("API Workload failed: {error}"),
            format!(
                "Verify the API listener for Service `{}` and restart it.",
                state.identity.service_id
            ),
        ),
    }
}

fn worker_boot_failure(
    state: &ServiceRuntimeState,
    error: platform_core::AppError,
) -> ServiceBootFailure {
    state.set_worker_phase(RuntimePhase::Failed);
    ServiceBootFailure {
        health: runtime_health(state),
        error: runtime_error(
            RuntimeErrorCode::WorkerRunFailed,
            format!("Worker Workload failed: {}", error.public_message),
            format!(
                "Verify the Service Store and Worker registrations for Service `{}`, then restart it.",
                state.identity.service_id
            ),
        ),
    }
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
        worker_workload_id: workload(WorkloadRole::WORKER),
        phase: RuntimePhase::Failed,
        worker_phase: RuntimePhase::Failed,
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
        ServiceEventEvidence,
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
        .routes(routes!(event_delivery_evidence))
        .merge(workflow::workflow_router())
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
    health_response(
        &state,
        state.phase() == RuntimePhase::Ready && state.worker_phase() == RuntimePhase::Ready,
    )
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
        worker_workload_id: state.identity.worker_workload_id.clone(),
        phase: state.phase(),
        worker_phase: state.worker_phase(),
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
    let pool = state
        .store()
        .map_err(platform_http::ApiErrorResponse::from)?;
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
        && path != "/runtime/event-deliveries"
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
    validate_workflow_definitions(contract, &config.workflow_definitions)?;

    let api_workload_id = one_workload(contract, WorkloadRole::API)?;
    let migration_workload_id = one_workload(contract, WorkloadRole::MIGRATION)?;
    let worker_workload_id = one_workload(contract, WorkloadRole::WORKER)?;
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
        worker_workload_id,
        store_id: store.store_id.clone(),
    })
}

fn validate_workflow_definitions(
    contract: &AutonomousServiceContract,
    definitions: &[WorkflowDefinition],
) -> Result<(), RuntimeError> {
    let declared_modules = contract.modules.iter().collect::<HashSet<_>>();
    let mut identities = HashSet::new();
    for definition in definitions {
        if !declared_modules.contains(&definition.owner) {
            return Err(runtime_error(
                RuntimeErrorCode::WorkflowOwnerNotDeclared,
                format!(
                    "Workflow Definition `{}/{}` is owned by a Module this Service does not declare",
                    definition.owner, definition.name
                ),
                format!(
                    "Declare Module `{}` in Service `{}` before registering its workflow.",
                    definition.owner, contract.service_id
                ),
            ));
        }
        let valid = definition.protocol == WORKFLOW_DEFINITION_PROTOCOL
            && !definition.owner.trim().is_empty()
            && !definition.name.trim().is_empty()
            && !definition.version.trim().is_empty()
            && !definition.input_contract.contract_id.trim().is_empty()
            && !definition.input_contract.version.trim().is_empty()
            && !definition.result_contract.contract_id.trim().is_empty()
            && !definition.result_contract.version.trim().is_empty()
            && !definition.steps.is_empty()
            && definition.steps.iter().all(|step| {
                !step.name.trim().is_empty()
                    && step
                        .timeout_ms
                        .is_none_or(|timeout| timeout > 0 && i64::try_from(timeout).is_ok())
                    && step.retry_policy.as_ref().is_none_or(|policy| {
                        policy.max_attempts > 0
                            && i32::try_from(policy.max_attempts).is_ok()
                            && policy.delays_ms.len()
                                == usize::try_from(policy.max_attempts.saturating_sub(1))
                                    .unwrap_or(usize::MAX)
                            && policy
                                .delays_ms
                                .iter()
                                .all(|delay| i64::try_from(*delay).is_ok())
                    })
                    && step.compensation.as_ref().is_none_or(|compensation| {
                        !compensation.name.trim().is_empty()
                            && compensation.order > 0
                            && i32::try_from(compensation.order).is_ok()
                            && !compensation.contract.contract_id.trim().is_empty()
                            && !compensation.contract.version.trim().is_empty()
                            && !compensation
                                .completion_contract
                                .contract_id
                                .trim()
                                .is_empty()
                            && !compensation.completion_contract.version.trim().is_empty()
                    })
            })
            && definition
                .steps
                .iter()
                .map(|step| &step.name)
                .collect::<HashSet<_>>()
                .len()
                == definition.steps.len()
            && definition
                .steps
                .iter()
                .filter_map(|step| step.compensation.as_ref().map(|value| &value.name))
                .collect::<HashSet<_>>()
                .len()
                == definition
                    .steps
                    .iter()
                    .filter(|step| step.compensation.is_some())
                    .count()
            && definition
                .steps
                .iter()
                .filter_map(|step| step.compensation.as_ref().map(|value| value.order))
                .collect::<HashSet<_>>()
                .len()
                == definition
                    .steps
                    .iter()
                    .filter(|step| step.compensation.is_some())
                    .count();
        if !valid {
            return Err(runtime_error(
                RuntimeErrorCode::InvalidWorkflowDefinition,
                format!(
                    "Workflow Definition `{}/{}` version `{}` is incomplete or invalid",
                    definition.owner, definition.name, definition.version
                ),
                "Fix the Module workflow declaration and restart the Service.",
            ));
        }
        if !identities.insert((&definition.owner, &definition.name, &definition.version)) {
            return Err(runtime_error(
                RuntimeErrorCode::DuplicateWorkflowDefinition,
                format!(
                    "Workflow Definition `{}/{}` version `{}` is registered more than once",
                    definition.owner, definition.name, definition.version
                ),
                "Keep one registered definition per owner, name, and version.",
            ));
        }
    }
    Ok(())
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
        (WorkloadRole::Worker, []) => Err(runtime_error(
            RuntimeErrorCode::MissingWorkerWorkload,
            "Service has no Worker Workload",
            "Declare exactly one Worker Workload in the Service v2 definition.",
        )),
        (WorkloadRole::Worker, _) => Err(runtime_error(
            RuntimeErrorCode::AmbiguousWorkerWorkload,
            "Service has more than one Worker Workload",
            "Declare exactly one Worker Workload for this runtime profile.",
        )),
        _ => unreachable!("runtime validates only API, Migration, and Worker roles"),
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
    use http::{Request, StatusCode, header};
    use http_body_util::BodyExt as _;
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
                AutonomousServiceWorkload::new("support-worker", "support", WorkloadRole::WORKER),
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
        assert_eq!(runtime.worker_workload_id, "support-worker");
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
        let state = ServiceRuntimeState::ready(
            "support",
            "support-api",
            "primary",
            "support-migrate",
            "support-worker",
        );
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

    #[tokio::test]
    async fn public_readiness_fails_when_worker_is_failed() {
        let state = ServiceRuntimeState::ready(
            "support",
            "support-api",
            "primary",
            "support-migrate",
            "support-worker",
        );
        state.set_worker_phase(super::RuntimePhase::Failed);
        let app = service_router(OpenApiRouter::new(), state);

        let ready = app
            .oneshot(
                Request::get("/health/ready")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn public_runtime_surfaces_are_in_openapi() {
        let document = super::openapi_document();

        assert!(document.paths.paths.contains_key("/health/live"));
        assert!(document.paths.paths.contains_key("/health/ready"));
        assert!(document.paths.paths.contains_key("/health/startup"));
        assert!(document.paths.paths.contains_key("/runtime/story-segments"));
        assert!(
            document
                .paths
                .paths
                .contains_key("/runtime/event-deliveries")
        );
        assert!(
            document
                .paths
                .paths
                .contains_key("/runtime/workflows/{owner}/{name}/instances")
        );
        assert!(
            document
                .paths
                .paths
                .contains_key("/runtime/workflows/instances/{instance_id}")
        );
    }

    #[tokio::test]
    async fn workflow_request_errors_have_stable_codes_and_next_actions() {
        let state = ServiceRuntimeState::ready(
            "support",
            "support-api",
            "primary",
            "support-migrate",
            "support-worker",
        );
        let app = service_router(OpenApiRouter::new(), state);

        let response = app
            .oneshot(
                Request::post("/runtime/workflows/support/ticket_sla/instances")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(error["code"], "workflow_invalid_request");
        assert_eq!(error["next_actions"][0], "correct_workflow_request");
    }

    #[tokio::test]
    async fn shutdown_transitions_service_to_stopped() {
        let state = ServiceRuntimeState::ready(
            "support",
            "support-api",
            "primary",
            "support-migrate",
            "support-worker",
        );
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
