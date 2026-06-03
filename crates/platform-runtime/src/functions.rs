use crate::retries::RetryPolicy;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use platform_core::{
    ActorContext, AppError, AppResult, CorrelationId, DbPool, ErrorCode, ExecutionContext,
    ExecutionId, RuntimeSpanAttributes, TenantId, TraceContext, record_runtime_span_attributes,
    trace_context_from_headers, trace_headers,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::Instrument;
use uuid::Uuid;

#[async_trait]
pub trait FunctionHandler: Debug + Send + Sync {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value>;
}

pub use FunctionHandler as RuntimeFunction;

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
    pub name: &'static str,
    pub version: u16,
    pub queue: &'static str,
    pub retry_policy: RetryPolicy,
    pub handler: Arc<dyn FunctionHandler>,
}

#[derive(Debug, Default, Clone)]
pub struct FunctionRegistry {
    functions: BTreeMap<String, FunctionDefinition>,
}

impl FunctionRegistry {
    pub fn register(&mut self, function: FunctionDefinition) {
        self.functions.insert(function.name.to_owned(), function);
    }

    pub fn get(&self, name: &str) -> Option<&FunctionDefinition> {
        self.functions.get(name)
    }

    pub fn all(&self) -> impl Iterator<Item = &FunctionDefinition> {
        self.functions.values()
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
    pub trace: TraceContext,
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeClient {
    pool: DbPool,
}

impl RuntimeClient {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue_function(&self, request: EnqueueFunctionRequest) -> AppResult<String> {
        let id = format!("fnrun_{}", Uuid::now_v7());
        let max_attempts = request.max_attempts.unwrap_or(3);
        let mut input_json = request.input_json;
        attach_runtime_context_to_input(
            &mut input_json,
            &request.correlation_id,
            &request.trace,
            request.causation_id.as_deref(),
        );
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
            sqlx::query(
                r#"
            insert into runtime.function_runs (
                id,
                function_name,
                input_json,
                max_attempts,
                correlation_id,
                actor
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
            )
            .bind(&id)
            .bind(&request.function_name)
            .bind(&input_json)
            .bind(max_attempts)
            .bind(&request.correlation_id.0)
            .bind(serde_json::to_value(&request.actor).map_err(map_serde_error)?)
            .execute(&self.pool)
            .await
            .map_err(map_runtime_error)
        }
        .instrument(span)
        .await?;

        self.record_function_execution_log(
            &FunctionLogContext {
                id: id.clone(),
                function_name: request.function_name,
                correlation_id: request.correlation_id.0,
                trace: request.trace,
            },
            ExecutionLogSeverity::Info,
            "Function run enqueued",
            serde_json::json!({
                "attempt": 0,
                "max_attempts": max_attempts,
            }),
        )
        .await;

        Ok(id)
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
            function_log_record(run, severity, body, attributes),
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
pub struct RuntimeWorker {
    pool: DbPool,
    registry: Arc<FunctionRegistry>,
    worker_id: String,
}

impl RuntimeWorker {
    pub fn new(
        pool: DbPool,
        registry: Arc<FunctionRegistry>,
        worker_id: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            registry,
            worker_id: worker_id.into(),
        }
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
                where status in ('pending', 'failed')
                  and available_at <= now()
                order by available_at asc, created_at asc
                limit $1
                for update skip locked
            )
            update runtime.function_runs function_run
            set status = 'processing',
                locked_at = now(),
                locked_by = $2,
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
                function_run.actor
            "#,
            )
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
                self.mark_failed(&run, &error).await?;
                return Ok(());
            };

            let attempt = u32::try_from(run.attempts + 1).unwrap_or(u32::MAX);
            let ctx = ExecutionContext {
                execution_id: ExecutionId(run.id.clone()),
                function_name: run.function_name.clone(),
                attempt,
                queue: definition.queue.to_owned(),
                correlation_id: CorrelationId::new(run.correlation_id.clone()),
                causation_id: run.causation_id.clone(),
                actor: run.actor.clone(),
                tenant_id: None::<TenantId>,
                trace: run.trace.clone(),
                deadline: None::<DateTime<Utc>>,
            };

            match definition.handler.call(ctx, run.input_json.clone()).await {
                Ok(_output) => self.mark_completed(&run).await,
                Err(error) => self.mark_failed(&run, &error).await,
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

    pub async fn mark_failed(&self, run: &ClaimedFunctionRun, error: &AppError) -> AppResult<()> {
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
            sqlx::query(
                r#"
            update runtime.function_runs
            set status = $2,
                attempts = attempts + 1,
                available_at = case when $2 = 'failed' then now() else available_at end,
                locked_at = null,
                locked_by = null,
                last_error = $3,
                updated_at = now()
            where id = $1
            "#,
            )
            .bind(&run.id)
            .bind(status.as_str())
            .bind(error.public_message.as_str())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(map_runtime_error)?;

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
            function_log_record(run, severity, body, attributes),
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
) -> ExecutionLogProjectionRecord {
    ExecutionLogProjectionRecord::from_runtime_attrs(
        RuntimeSpanAttributes::function(run.correlation_id(), run.id(), run.function_name()),
        severity,
        body,
    )
    .with_attributes(attributes)
    .with_trace(run.trace())
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
}

async fn insert_execution_log_projection(
    pool: &DbPool,
    record: ExecutionLogProjectionRecord,
) -> AppResult<String> {
    let id = format!("elog_{}", Uuid::now_v7());

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
    .bind(Utc::now())
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

fn normalize_log_attributes(attributes: Value) -> Value {
    match attributes {
        Value::Object(_) => attributes,
        other => serde_json::json!({ "value": other }),
    }
}

type FunctionRunRow = (String, String, Value, i32, i32, String, Value);

impl TryFrom<FunctionRunRow> for ClaimedFunctionRun {
    type Error = AppError;

    fn try_from(row: FunctionRunRow) -> Result<Self, Self::Error> {
        let (id, function_name, input_json, attempts, max_attempts, correlation_id, actor) = row;
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

fn map_runtime_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Runtime operation failed").with_source(source)
}

fn map_serde_error(source: serde_json::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Runtime payload serialization failed").with_source(source)
}
