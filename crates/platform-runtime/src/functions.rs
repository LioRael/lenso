use crate::retries::RetryPolicy;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use platform_core::{
    ActorContext, AppError, AppResult, CorrelationId, DbPool, ErrorCode, ExecutionContext,
    ExecutionId, ExecutionLogCaptureReport, ExecutionLogCaptureStatus, ExecutionLogScope,
    ExecutionLogWriter, OutboxEvent, OutboxPublisher, PostgresExecutionLogWriter,
    RuntimeSpanAttributes, TenantId, TraceContext, capture_execution_logs, db::DbTransaction,
    record_runtime_span_attributes, trace_context_from_headers, trace_headers,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::Instrument;
use uuid::Uuid;

const STALE_PROCESSING_LOCK_SECONDS: i64 = 300;

#[async_trait]
pub trait FunctionHandler: Debug + Send + Sync {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value>;

    fn observability(&self) -> Option<FunctionHandlerObservability> {
        None
    }

    /// Requests one bounded Host-owned terminal observation if this handler's
    /// run becomes dead. The default keeps linked functions unchanged.
    fn terminal_observation(&self) -> Option<FunctionTerminalObservation> {
        None
    }
}

pub use FunctionHandler as RuntimeFunction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionHandlerObservability {
    pub source: String,
    pub attributes: Value,
}

impl FunctionHandlerObservability {
    pub fn new(source: impl Into<String>, attributes: Value) -> Self {
        Self {
            source: source.into(),
            attributes: normalize_log_attributes(attributes),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionTerminalObservation {
    pub owner_module: String,
}

impl FunctionTerminalObservation {
    #[must_use]
    pub fn new(owner_module: impl Into<String>) -> Self {
        Self {
            owner_module: owner_module.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionLogSeverity {
    Info,
    Error,
}

impl ExecutionLogSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub version: u16,
    pub queue: String,
    pub retry_policy: RetryPolicy,
    pub handler: Arc<dyn FunctionHandler>,
}

#[derive(Debug, Default, Clone)]
pub struct FunctionRegistry {
    functions: BTreeMap<String, FunctionDefinition>,
    duplicate_names: BTreeSet<String>,
}

impl FunctionRegistry {
    pub fn register(&mut self, function: FunctionDefinition) {
        let name = function.name.clone();
        if self.functions.insert(name.clone(), function).is_some() {
            self.duplicate_names.insert(name);
        }
    }

    pub fn get(&self, name: &str) -> Option<&FunctionDefinition> {
        self.functions.get(name)
    }

    pub fn all(&self) -> impl Iterator<Item = &FunctionDefinition> {
        self.functions.values()
    }

    pub fn duplicate_names(&self) -> impl Iterator<Item = &str> {
        self.duplicate_names.iter().map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionRunStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionTenancyMode {
    None,
    Optional,
    Required,
}

impl FunctionTenancyMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }

    const fn accepts(self, tenant_id: Option<&TenantId>) -> bool {
        match self {
            Self::None => tenant_id.is_none(),
            Self::Optional => true,
            Self::Required => tenant_id.is_some(),
        }
    }
}

impl FunctionRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnqueueFunctionRequest {
    pub function_name: String,
    pub input_json: Value,
    pub correlation_id: CorrelationId,
    pub actor: ActorContext,
    pub tenant_id: Option<TenantId>,
    pub tenancy_mode: FunctionTenancyMode,
    pub trace: TraceContext,
    pub causation_id: Option<String>,
    pub max_attempts: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimedFunctionRun {
    pub id: String,
    pub function_name: String,
    pub input_json: Value,
    pub attempts: i32,
    pub max_attempts: i32,
    pub correlation_id: String,
    pub actor: ActorContext,
    pub tenant_id: Option<TenantId>,
    pub tenancy_mode: FunctionTenancyMode,
    pub trace: TraceContext,
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeClient {
    pool: DbPool,
    service_name: String,
}

impl RuntimeClient {
    pub fn new(pool: DbPool) -> Self {
        Self {
            pool,
            service_name: "lenso".to_owned(),
        }
    }

    #[must_use]
    pub fn with_service_name(mut self, service_name: impl Into<String>) -> Self {
        self.service_name = service_name.into();
        self
    }

    pub async fn enqueue_function(&self, request: EnqueueFunctionRequest) -> AppResult<String> {
        let mut tx = self.pool.begin().await.map_err(map_runtime_error)?;
        let run = self.enqueue_function_in_tx(&mut tx, request).await?;
        tx.commit().await.map_err(map_runtime_error)?;
        self.record_function_enqueued(&run).await;
        Ok(run.id)
    }

    /// Enqueues one function run with a caller-owned durable identity inside an
    /// existing Host transaction. This is used by durable transport adapters
    /// that must atomically commit their own receipt and Runtime work.
    pub async fn enqueue_function_with_id_in_tx(
        &self,
        tx: &mut DbTransaction<'_>,
        run_id: impl Into<String>,
        request: EnqueueFunctionRequest,
    ) -> AppResult<()> {
        self.enqueue_function_in_tx_with_id(tx, run_id.into(), request)
            .await
            .map(|_| ())
    }

    pub(crate) async fn enqueue_function_in_tx(
        &self,
        tx: &mut DbTransaction<'_>,
        request: EnqueueFunctionRequest,
    ) -> AppResult<EnqueuedFunctionRun> {
        self.enqueue_function_in_tx_with_id(tx, format!("fnrun_{}", Uuid::now_v7()), request)
            .await
    }

    async fn enqueue_function_in_tx_with_id(
        &self,
        tx: &mut DbTransaction<'_>,
        id: String,
        request: EnqueueFunctionRequest,
    ) -> AppResult<EnqueuedFunctionRun> {
        if !request.tenancy_mode.accepts(request.tenant_id.as_ref()) {
            return Err(AppError::new(
                ErrorCode::Validation,
                "function tenant context is incompatible with its tenancy mode",
            ));
        }
        let max_attempts = request.max_attempts.unwrap_or(3);
        let mut input_json = request.input_json;
        attach_runtime_context_to_input(
            &mut input_json,
            &request.correlation_id,
            &request.trace,
            request.causation_id.as_deref(),
        );
        let actor = serde_json::to_value(&request.actor).map_err(map_serde_error)?;
        let tenant_id = request.tenant_id.as_ref().map(|tenant| tenant.0.clone());
        let span = tracing::info_span!(
            "function_enqueue",
            lenso.correlation_id = tracing::field::Empty,
            lenso.story_id = tracing::field::Empty,
            lenso.function_run_id = tracing::field::Empty,
            lenso.execution.kind = tracing::field::Empty,
            lenso.execution.name = tracing::field::Empty,
        );
        record_runtime_span_attributes(
            &span,
            &RuntimeSpanAttributes::function(
                request.correlation_id.0.clone(),
                id.clone(),
                request.function_name.clone(),
            ),
        );

        async {
            let inserted = sqlx::query_scalar::<_, bool>(
                r#"
                insert into runtime.function_runs (
                    id,
                    function_name,
                    input_json,
                    max_attempts,
                    correlation_id,
                    actor,
                    tenant_id,
                    tenancy_mode
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do nothing
                returning true
                "#,
            )
            .bind(&id)
            .bind(&request.function_name)
            .bind(&input_json)
            .bind(max_attempts)
            .bind(&request.correlation_id.0)
            .bind(&actor)
            .bind(&tenant_id)
            .bind(request.tenancy_mode.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(map_runtime_error)?
            .unwrap_or(false);

            if inserted {
                return Ok(());
            }

            let existing =
                sqlx::query_as::<_, (String, Value, i32, String, Value, Option<String>, String)>(
                    r#"
                select
                    function_name,
                    input_json,
                    max_attempts,
                    correlation_id,
                    actor,
                    tenant_id,
                    tenancy_mode
                from runtime.function_runs
                where id = $1
                "#,
                )
                .bind(&id)
                .fetch_one(&mut **tx)
                .await
                .map_err(map_runtime_error)?;

            let expected = (
                request.function_name.clone(),
                input_json.clone(),
                max_attempts,
                request.correlation_id.0.clone(),
                actor.clone(),
                tenant_id.clone(),
                request.tenancy_mode.as_str().to_owned(),
            );
            if existing != expected {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "function run identity is already bound to a different request",
                ));
            }

            Ok(())
        }
        .instrument(span)
        .await?;

        Ok(EnqueuedFunctionRun {
            id,
            function_name: request.function_name,
            correlation_id: request.correlation_id.0,
            trace: request.trace,
            max_attempts,
        })
    }

    pub(crate) async fn record_function_enqueued(&self, run: &EnqueuedFunctionRun) {
        self.record_function_execution_log(
            &FunctionLogContext {
                id: run.id.clone(),
                function_name: run.function_name.clone(),
                correlation_id: run.correlation_id.clone(),
                trace: run.trace.clone(),
            },
            ExecutionLogSeverity::Info,
            "Function run enqueued",
            serde_json::json!({
                "attempt": 0,
                "max_attempts": run.max_attempts,
            }),
        )
        .await;
    }

    async fn record_function_execution_log(
        &self,
        run: &FunctionLogContext,
        severity: ExecutionLogSeverity,
        body: &'static str,
        attributes: Value,
    ) {
        emit_function_lifecycle_event(run, severity, body, &attributes, None);
        if let Err(error) = insert_execution_log_projection(
            &self.pool,
            function_log_record(run, severity, body, attributes, &self.service_name),
        )
        .await
        {
            tracing::warn!(
                error = ?error,
                function_run_id = %run.id,
                "failed to write function execution log"
            );
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EnqueuedFunctionRun {
    pub id: String,
    function_name: String,
    correlation_id: String,
    trace: TraceContext,
    max_attempts: i32,
}

#[derive(Debug, Clone)]
pub struct RuntimeWorker {
    pool: DbPool,
    registry: Arc<FunctionRegistry>,
    worker_id: String,
    service_name: String,
    execution_log_writer: Option<Arc<dyn ExecutionLogWriter>>,
}

impl RuntimeWorker {
    pub fn new(
        pool: DbPool,
        registry: Arc<FunctionRegistry>,
        worker_id: impl Into<String>,
    ) -> Self {
        let execution_log_writer = Arc::new(PostgresExecutionLogWriter::new(pool.clone()));
        Self {
            pool,
            registry,
            worker_id: worker_id.into(),
            service_name: "lenso".to_owned(),
            execution_log_writer: Some(execution_log_writer),
        }
    }

    #[must_use]
    pub fn with_service_name(mut self, service_name: impl Into<String>) -> Self {
        self.service_name = service_name.into();
        self
    }

    #[must_use]
    #[doc(hidden)]
    pub fn with_execution_log_writer(
        mut self,
        execution_log_writer: Arc<dyn ExecutionLogWriter>,
    ) -> Self {
        self.execution_log_writer = Some(execution_log_writer);
        self
    }

    #[must_use]
    #[doc(hidden)]
    pub fn without_execution_log_capture(mut self) -> Self {
        self.execution_log_writer = None;
        self
    }

    pub async fn claim_batch(&self, batch_size: i64) -> AppResult<Vec<ClaimedFunctionRun>> {
        let span = tracing::info_span!(
            "function_claim_batch",
            worker_id = %self.worker_id,
            lenso.execution.kind = "function_claim",
            lenso.execution.name = "function.claim_batch",
        );

        async {
            let runs = sqlx::query_as::<_, FunctionRunRow>(
                r#"
            with claimed as (
                select id
                from runtime.function_runs
                where (
                    status in ('pending', 'failed')
                    and available_at <= now()
                )
                or (
                    status = 'processing'
                    and locked_at <= now() - ($1::double precision * interval '1 second')
                )
                order by available_at asc, created_at asc
                limit $2
                for update skip locked
            )
            update runtime.function_runs function_run
            set status = 'processing',
                locked_at = now(),
                locked_by = $3,
                started_at = coalesce(started_at, now()),
                last_error = null,
                updated_at = now()
            from claimed
            where function_run.id = claimed.id
            returning
                function_run.id,
                function_run.function_name,
                function_run.input_json,
                function_run.attempts,
                function_run.max_attempts,
                function_run.correlation_id,
                function_run.actor,
                function_run.tenant_id,
                function_run.tenancy_mode
            "#,
            )
            .bind(stale_processing_lock_seconds())
            .bind(batch_size)
            .bind(&self.worker_id)
            .fetch_all(&self.pool)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(TryInto::try_into)
                    .collect::<AppResult<Vec<_>>>()
            })
            .map_err(map_runtime_error)??;

            for run in &runs {
                self.record_function_execution_log(
                    run,
                    ExecutionLogSeverity::Info,
                    "Function run claimed",
                    serde_json::json!({
                        "attempt": run.attempts + 1,
                        "max_attempts": run.max_attempts,
                        "worker_id": self.worker_id,
                    }),
                )
                .await;
            }

            Ok(runs)
        }
        .instrument(span)
        .await
    }

    pub async fn claim_and_run_batch(&self, batch_size: i64) -> AppResult<usize> {
        let span = tracing::info_span!(
            "function_worker_loop",
            worker_id = %self.worker_id,
            lenso.execution.kind = "worker_loop",
            lenso.execution.name = "runtime_worker.claim_and_run_batch",
        );

        async {
            let runs = self.claim_batch(batch_size).await?;
            let count = runs.len();

            for run in runs {
                self.run_claimed(run).await?;
            }

            Ok(count)
        }
        .instrument(span)
        .await
    }

    async fn run_claimed(&self, run: ClaimedFunctionRun) -> AppResult<()> {
        let span = tracing::info_span!(
            "function_run",
            lenso.correlation_id = tracing::field::Empty,
            lenso.story_id = tracing::field::Empty,
            lenso.function_run_id = tracing::field::Empty,
            lenso.execution.kind = tracing::field::Empty,
            lenso.execution.name = tracing::field::Empty,
        );
        record_runtime_span_attributes(
            &span,
            &RuntimeSpanAttributes::function(
                run.correlation_id.clone(),
                run.id.clone(),
                run.function_name.clone(),
            ),
        );

        async {
            self.record_function_execution_log(
                &run,
                ExecutionLogSeverity::Info,
                "Function run started",
                serde_json::json!({
                    "attempt": run.attempts + 1,
                    "max_attempts": run.max_attempts,
                    "worker_id": self.worker_id,
                }),
            )
            .await;

            let Some(definition) = self.registry.get(&run.function_name) else {
                let error = AppError::new(
                    ErrorCode::Internal,
                    format!("Runtime function {} is not registered", run.function_name),
                )
                .retryable();
                self.mark_failed(&run, &error, RetryPolicy::default().initial_delay)
                    .await?;
                return Ok(());
            };

            let attempt = u32::try_from(run.attempts + 1).unwrap_or(u32::MAX);
            let ctx = ExecutionContext {
                execution_id: ExecutionId(run.id.clone()),
                function_name: run.function_name.clone(),
                attempt,
                queue: definition.queue.clone(),
                correlation_id: CorrelationId::new(run.correlation_id.clone()),
                causation_id: run.causation_id.clone(),
                actor: run.actor.clone(),
                tenant_id: run.tenant_id.clone(),
                trace: run.trace.clone(),
                deadline: None::<DateTime<Utc>>,
            };

            let execution_log_scope = ExecutionLogScope::function(
                &ctx,
                self.service_name.clone(),
                self.worker_id.clone(),
            );
            let observability = definition.handler.observability();
            let terminal_observation = definition.handler.terminal_observation();
            let ((result, started_at, duration_ms), execution_log_report) = capture_execution_logs(
                execution_log_scope,
                self.execution_log_writer.clone(),
                async {
                    let started_at = Utc::now();
                    let started = Instant::now();
                    let result = definition.handler.call(ctx, run.input_json.clone()).await;
                    let duration_ms = started.elapsed().as_millis().try_into().unwrap_or(i64::MAX);
                    (result, started_at, duration_ms)
                },
            )
            .await;
            if execution_log_capture_is_incomplete(execution_log_report) {
                tracing::warn!(
                    function_run_id = %run.id,
                    status = ?execution_log_report.status,
                    observed = execution_log_report.observed,
                    persisted = execution_log_report.persisted,
                    dropped = execution_log_report.dropped,
                    write_failures = execution_log_report.write_failures,
                    "structured execution log capture was incomplete"
                );
            }
            if let Some(observability) = observability {
                self.record_function_handler_operation_log(
                    &run,
                    observability,
                    started_at,
                    duration_ms,
                    result.as_ref().err(),
                )
                .await;
            }

            match result {
                Ok(_output) => self.mark_completed(&run).await,
                Err(error) => {
                    self.mark_failed_with_terminal_observation(
                        &run,
                        &error,
                        retry_delay_for_error(definition.retry_policy.initial_delay, &error),
                        terminal_observation.as_ref(),
                    )
                    .await
                }
            }
        }
        .instrument(span)
        .await
    }

    pub async fn mark_completed(&self, run: &ClaimedFunctionRun) -> AppResult<()> {
        sqlx::query(
            r#"
            update runtime.function_runs
            set status = 'completed',
                completed_at = now(),
                locked_at = null,
                locked_by = null,
                last_error = null,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(&run.id)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_runtime_error)?;

        self.record_function_execution_log(
            run,
            ExecutionLogSeverity::Info,
            "Function run completed",
            serde_json::json!({
                "attempt": run.attempts + 1,
                "max_attempts": run.max_attempts,
                "worker_id": self.worker_id,
            }),
        )
        .await;

        Ok(())
    }

    pub async fn mark_failed(
        &self,
        run: &ClaimedFunctionRun,
        error: &AppError,
        retry_delay: Duration,
    ) -> AppResult<()> {
        self.mark_failed_with_terminal_observation(run, error, retry_delay, None)
            .await
    }

    async fn mark_failed_with_terminal_observation(
        &self,
        run: &ClaimedFunctionRun,
        error: &AppError,
        retry_delay: Duration,
        terminal_observation: Option<&FunctionTerminalObservation>,
    ) -> AppResult<()> {
        let next_attempt = run.attempts + 1;
        let status = if next_attempt >= run.max_attempts {
            FunctionRunStatus::Dead
        } else if error.retryable {
            FunctionRunStatus::Failed
        } else {
            FunctionRunStatus::Dead
        };

        let span = tracing::info_span!(
            "function_run_fail",
            lenso.correlation_id = tracing::field::Empty,
            lenso.story_id = tracing::field::Empty,
            lenso.function_run_id = tracing::field::Empty,
            lenso.execution.kind = tracing::field::Empty,
            lenso.execution.name = tracing::field::Empty,
        );
        record_runtime_span_attributes(
            &span,
            &RuntimeSpanAttributes::function(
                run.correlation_id.clone(),
                run.id.clone(),
                run.function_name.clone(),
            ),
        );

        async {
            let mut tx = self.pool.begin().await.map_err(map_runtime_error)?;
            let updated = sqlx::query(
                r#"
            update runtime.function_runs
            set status = $2,
                attempts = attempts + 1,
                available_at = case
                    when $2 = 'failed' then now() + ($4::double precision * interval '1 second')
                    else available_at
                end,
                locked_at = null,
                locked_by = null,
                last_error = $3,
                updated_at = now()
            where id = $1 and status = 'processing'
            "#,
            )
            .bind(&run.id)
            .bind(status.as_str())
            .bind(error.public_message.as_str())
            .bind(retry_delay_seconds(retry_delay))
            .execute(&mut *tx)
            .await
            .map_err(map_runtime_error)?;

            if updated.rows_affected() == 0 {
                tx.commit().await.map_err(map_runtime_error)?;
                return Ok(());
            }

            if status == FunctionRunStatus::Dead
                && let Some(observation) = terminal_observation
            {
                OutboxPublisher
                    .publish_in_tx(&mut tx, &terminal_function_event(run, error, observation))
                    .await?;
            }

            tx.commit().await.map_err(map_runtime_error)?;

            self.record_function_execution_log(
                run,
                ExecutionLogSeverity::Error,
                if status == FunctionRunStatus::Dead {
                    "Function run marked dead"
                } else {
                    "Function run failed"
                },
                serde_json::json!({
                    "attempt": next_attempt,
                    "max_attempts": run.max_attempts,
                    "status": status.as_str(),
                    "retryable": error.retryable,
                    "retry_after_ms": error.retry_after_ms,
                    "provider_trace_reference": error.provider_trace_reference,
                    "error": error.public_message,
                    "worker_id": self.worker_id,
                }),
            )
            .await;

            Ok(())
        }
        .instrument(span)
        .await
    }

    async fn record_function_execution_log(
        &self,
        run: &ClaimedFunctionRun,
        severity: ExecutionLogSeverity,
        body: &'static str,
        attributes: Value,
    ) {
        emit_function_lifecycle_event(run, severity, body, &attributes, Some(&self.worker_id));
        if let Err(error) = insert_execution_log_projection(
            &self.pool,
            function_log_record(run, severity, body, attributes, &self.service_name),
        )
        .await
        {
            tracing::warn!(
                error = ?error,
                function_run_id = %run.id,
                "failed to write function execution log"
            );
        }
    }

    async fn record_function_handler_operation_log(
        &self,
        run: &ClaimedFunctionRun,
        observability: FunctionHandlerObservability,
        started_at: DateTime<Utc>,
        duration_ms: i64,
        error: Option<&AppError>,
    ) {
        let body = if error.is_some() {
            "Function handler operation failed"
        } else {
            "Function handler operation completed"
        };
        let severity = if error.is_some() {
            ExecutionLogSeverity::Error
        } else {
            ExecutionLogSeverity::Info
        };
        let attributes = function_handler_operation_attributes(
            run,
            &self.worker_id,
            observability,
            duration_ms,
            error,
        );
        emit_function_lifecycle_event(run, severity, body, &attributes, Some(&self.worker_id));
        if let Err(error) = insert_execution_log_projection(
            &self.pool,
            function_log_record(run, severity, body, attributes, &self.service_name)
                .with_occurred_at(started_at),
        )
        .await
        {
            tracing::warn!(
                error = ?error,
                function_run_id = %run.id,
                "failed to write function handler operation log"
            );
        }
    }
}

fn execution_log_capture_is_incomplete(report: ExecutionLogCaptureReport) -> bool {
    report.status != ExecutionLogCaptureStatus::Complete
}

fn terminal_function_event(
    run: &ClaimedFunctionRun,
    error: &AppError,
    observation: &FunctionTerminalObservation,
) -> OutboxEvent {
    const EVENT_NAME: &str = "lenso.runtime-function-terminal.v1";
    OutboxEvent {
        id: format!("runtime:function-run:{}:terminal", run.id),
        event_name: EVENT_NAME.to_owned(),
        event_version: 1,
        source_module: "lenso/runtime".to_owned(),
        aggregate_type: "runtime_function_run".to_owned(),
        aggregate_id: run.id.clone(),
        correlation_id: run.correlation_id.clone(),
        causation_id: Some(run.id.clone()),
        occurred_at: Utc::now(),
        payload: serde_json::json!({
            "failureClassification": if error.retryable {
                "retry_exhausted"
            } else {
                "permanent_failure"
            },
            "failureCode": error.code.as_str(),
            "functionName": run.function_name,
            "functionRunId": run.id,
            "ownerModule": observation.owner_module,
            "terminalState": "dead",
        }),
        headers: serde_json::json!({
            "schema_ref": "contracts/events/lenso/lenso.runtime-function-terminal.v1.schema.json",
        }),
    }
}

#[derive(Debug)]
struct FunctionLogContext {
    id: String,
    function_name: String,
    correlation_id: String,
    trace: TraceContext,
}

trait FunctionLogSource {
    fn id(&self) -> String;
    fn function_name(&self) -> String;
    fn correlation_id(&self) -> String;
    fn trace(&self) -> TraceContext;
}

impl FunctionLogSource for FunctionLogContext {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn function_name(&self) -> String {
        self.function_name.clone()
    }

    fn correlation_id(&self) -> String {
        self.correlation_id.clone()
    }

    fn trace(&self) -> TraceContext {
        self.trace.clone()
    }
}

impl FunctionLogSource for ClaimedFunctionRun {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn function_name(&self) -> String {
        self.function_name.clone()
    }

    fn correlation_id(&self) -> String {
        self.correlation_id.clone()
    }

    fn trace(&self) -> TraceContext {
        self.trace.clone()
    }
}

fn emit_function_lifecycle_event(
    run: &impl FunctionLogSource,
    severity: ExecutionLogSeverity,
    body: &'static str,
    attributes: &Value,
    worker_id: Option<&str>,
) {
    match severity {
        ExecutionLogSeverity::Error => {
            tracing::error!(
                function_run_id = %run.id(),
                function_name = %run.function_name(),
                correlation_id = %run.correlation_id(),
                worker_id = worker_id.unwrap_or(""),
                attributes = %attributes,
                "{body}"
            );
        }
        _ => {
            tracing::info!(
                function_run_id = %run.id(),
                function_name = %run.function_name(),
                correlation_id = %run.correlation_id(),
                worker_id = worker_id.unwrap_or(""),
                attributes = %attributes,
                "{body}"
            );
        }
    }
}

fn function_log_record(
    run: &impl FunctionLogSource,
    severity: ExecutionLogSeverity,
    body: impl Into<String>,
    attributes: Value,
    service_name: &str,
) -> ExecutionLogProjectionRecord {
    ExecutionLogProjectionRecord::from_runtime_attrs(
        RuntimeSpanAttributes::function(run.correlation_id(), run.id(), run.function_name()),
        severity,
        body,
    )
    .with_attributes(attributes)
    .with_trace(run.trace())
    .with_service_name(service_name)
}

#[derive(Debug, Clone)]
struct ExecutionLogProjectionRecord {
    correlation_id: String,
    execution_id: String,
    execution_type: String,
    execution_name: String,
    severity: ExecutionLogSeverity,
    body: String,
    attributes: Value,
    trace: TraceContext,
    service_name: String,
    occurred_at: Option<DateTime<Utc>>,
}

impl ExecutionLogProjectionRecord {
    fn from_runtime_attrs(
        attrs: RuntimeSpanAttributes,
        severity: ExecutionLogSeverity,
        body: impl Into<String>,
    ) -> Self {
        let execution_id = attrs
            .function_run_id
            .clone()
            .or(attrs.outbox_event_id)
            .unwrap_or_else(|| attrs.story_id.clone());

        Self {
            correlation_id: attrs.correlation_id,
            execution_id,
            execution_type: attrs.execution_kind,
            execution_name: attrs.execution_name,
            severity,
            body: body.into(),
            attributes: Value::Object(Default::default()),
            trace: TraceContext::default(),
            service_name: "lenso".to_owned(),
            occurred_at: None,
        }
    }

    fn with_attributes(mut self, attributes: Value) -> Self {
        self.attributes = attributes;
        self
    }

    fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = trace;
        self
    }

    fn with_service_name(mut self, service_name: &str) -> Self {
        self.service_name = service_name.to_owned();
        self
    }

    fn with_occurred_at(mut self, occurred_at: DateTime<Utc>) -> Self {
        self.occurred_at = Some(occurred_at);
        self
    }
}

async fn insert_execution_log_projection(
    pool: &DbPool,
    record: ExecutionLogProjectionRecord,
) -> AppResult<String> {
    let id = format!("elog_{}", Uuid::now_v7());
    let occurred_at = record.occurred_at.unwrap_or_else(Utc::now);

    sqlx::query(
        r#"
        insert into platform.execution_logs (
            id,
            correlation_id,
            story_id,
            execution_id,
            execution_type,
            execution_name,
            occurred_at,
            severity,
            body,
            attributes,
            trace_id,
            span_id,
            service_name,
            redacted_fields
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        "#,
    )
    .bind(&id)
    .bind(&record.correlation_id)
    .bind(&record.correlation_id)
    .bind(&record.execution_id)
    .bind(&record.execution_type)
    .bind(&record.execution_name)
    .bind(occurred_at)
    .bind(record.severity.as_str())
    .bind(&record.body)
    .bind(normalize_log_attributes(record.attributes))
    .bind(&record.trace.trace_id)
    .bind(&record.trace.span_id)
    .bind(&record.service_name)
    .bind(Vec::<String>::new())
    .execute(pool)
    .await
    .map_err(map_runtime_error)?;

    Ok(id)
}

fn function_handler_operation_attributes(
    run: &ClaimedFunctionRun,
    worker_id: &str,
    observability: FunctionHandlerObservability,
    duration_ms: i64,
    error: Option<&AppError>,
) -> Value {
    let mut attributes = match observability.attributes {
        Value::Object(attributes) => attributes,
        other => serde_json::Map::from_iter([("value".to_owned(), other)]),
    };
    attributes.insert("source".to_owned(), Value::String(observability.source));
    attributes.insert("attempt".to_owned(), serde_json::json!(run.attempts + 1));
    attributes.insert(
        "max_attempts".to_owned(),
        serde_json::json!(run.max_attempts),
    );
    attributes.insert("duration_ms".to_owned(), serde_json::json!(duration_ms));
    attributes.insert("success".to_owned(), serde_json::json!(error.is_none()));
    attributes.insert("worker_id".to_owned(), serde_json::json!(worker_id));
    attributes.insert(
        "function_name".to_owned(),
        serde_json::json!(run.function_name),
    );
    attributes.insert("request_id".to_owned(), serde_json::json!(run.id));
    attributes.insert("trace_id".to_owned(), serde_json::json!(run.trace.trace_id));
    attributes.insert("span_id".to_owned(), serde_json::json!(run.trace.span_id));

    if let Some(error) = error {
        attributes.insert(
            "error_code".to_owned(),
            serde_json::json!(error.code.as_str()),
        );
        attributes.insert("error".to_owned(), serde_json::json!(error.public_message));
        attributes.insert("retryable".to_owned(), serde_json::json!(error.retryable));
        attributes.insert(
            "retry_after_ms".to_owned(),
            serde_json::json!(error.retry_after_ms),
        );
        attributes.insert(
            "provider_trace_reference".to_owned(),
            serde_json::json!(error.provider_trace_reference),
        );
        attributes.insert("error_details".to_owned(), serde_json::json!(error.details));
    }

    Value::Object(attributes)
}

fn normalize_log_attributes(attributes: Value) -> Value {
    match attributes {
        Value::Object(_) => attributes,
        other => serde_json::json!({ "value": other }),
    }
}

fn stale_processing_lock_seconds() -> f64 {
    STALE_PROCESSING_LOCK_SECONDS as f64
}

fn retry_delay_seconds(delay: Duration) -> f64 {
    delay.as_secs_f64()
}

fn retry_delay_for_error(default: Duration, error: &AppError) -> Duration {
    error
        .retry_after_ms
        .map(Duration::from_millis)
        .unwrap_or(default)
}

type FunctionRunRow = (
    String,
    String,
    Value,
    i32,
    i32,
    String,
    Value,
    Option<String>,
    String,
);

impl TryFrom<FunctionRunRow> for ClaimedFunctionRun {
    type Error = AppError;

    fn try_from(row: FunctionRunRow) -> Result<Self, Self::Error> {
        let (
            id,
            function_name,
            input_json,
            attempts,
            max_attempts,
            correlation_id,
            actor,
            tenant_id,
            tenancy_mode,
        ) = row;
        let runtime_context = input_json.get("_lenso_runtime");
        let trace = runtime_context
            .map(trace_context_from_headers)
            .unwrap_or_default();
        let causation_id = runtime_context
            .and_then(|context| context.get("causation_id"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        Ok(Self {
            id,
            function_name,
            input_json,
            attempts,
            max_attempts,
            correlation_id,
            actor: serde_json::from_value(actor).map_err(map_serde_error)?,
            tenant_id: tenant_id.map(TenantId),
            tenancy_mode: serde_json::from_value(Value::String(tenancy_mode))
                .map_err(map_serde_error)?,
            trace,
            causation_id,
        })
    }
}

fn attach_runtime_context_to_input(
    input_json: &mut Value,
    correlation_id: &CorrelationId,
    trace: &TraceContext,
    causation_id: Option<&str>,
) {
    let mut runtime_context = trace_headers(trace, correlation_id);
    if let Some(causation_id) = causation_id {
        runtime_context["causation_id"] = Value::String(causation_id.to_owned());
    }

    match input_json {
        Value::Object(object) => {
            object.insert("_lenso_runtime".to_owned(), runtime_context);
        }
        other => {
            *other = serde_json::json!({
                "payload": other.clone(),
                "_lenso_runtime": runtime_context,
            });
        }
    }
}

pub(crate) fn map_runtime_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Runtime operation failed").with_source(source)
}

fn map_serde_error(source: serde_json::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Runtime payload serialization failed").with_source(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_execution_log_capture_is_reported_as_incomplete() {
        assert!(execution_log_capture_is_incomplete(
            ExecutionLogCaptureReport {
                status: ExecutionLogCaptureStatus::Disabled,
                observed: 0,
                persisted: 0,
                dropped: 0,
                write_failures: 0,
            }
        ));
    }
}
