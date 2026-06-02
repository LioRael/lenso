use crate::TraceContext;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::telemetry_attrs::RuntimeSpanAttributes;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::Debug;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecutionLogSeverity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl ExecutionLogSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionLogRecord {
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

impl ExecutionLogRecord {
    pub(crate) fn from_runtime_attrs(
        attrs: RuntimeSpanAttributes,
        severity: ExecutionLogSeverity,
        body: impl Into<String>,
    ) -> Self {
        let execution_id = attrs
            .function_run_id
            .clone()
            .or_else(|| attrs.outbox_event_id.clone())
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

    pub(crate) fn with_attributes(mut self, attributes: Value) -> Self {
        self.attributes = attributes;
        self
    }

    pub(crate) fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = trace;
        self
    }
}

pub(crate) async fn insert_execution_log_projection(
    pool: &DbPool,
    record: ExecutionLogRecord,
) -> AppResult<String> {
    let id = next_execution_log_id();
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
    .bind(normalize_attributes(record.attributes))
    .bind(&record.trace.trace_id)
    .bind(&record.trace.span_id)
    .bind(&record.service_name)
    .bind(Vec::<String>::new())
    .execute(pool)
    .await
    .map_err(map_execution_log_error)?;

    Ok(id)
}

fn normalize_attributes(attributes: Value) -> Value {
    match attributes {
        Value::Object(_) => attributes,
        other => json!({ "value": other }),
    }
}

fn next_execution_log_id() -> String {
    format!("elog_{}", Uuid::now_v7())
}

fn map_execution_log_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Execution log operation failed").with_source(source)
}

#[derive(Debug, Clone)]
pub struct ExecutionLogRow {
    pub id: String,
    pub correlation_id: String,
    pub story_id: String,
    pub execution_id: String,
    pub execution_type: String,
    pub execution_name: String,
    pub occurred_at: DateTime<Utc>,
    pub severity: String,
    pub body: String,
    pub attributes: Value,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub service_name: String,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionLogQuery {
    pub execution_id: String,
    pub occurred_before: Option<DateTime<Utc>>,
    pub limit: i64,
}

#[async_trait]
pub trait ExecutionLogProvider: Debug + Send + Sync {
    async fn query_execution_logs(
        &self,
        query: ExecutionLogQuery,
    ) -> AppResult<Vec<ExecutionLogRow>>;
}

#[derive(Debug, Clone)]
pub struct PostgresExecutionLogProvider {
    pool: DbPool,
}

impl PostgresExecutionLogProvider {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionLogProvider for PostgresExecutionLogProvider {
    async fn query_execution_logs(
        &self,
        query: ExecutionLogQuery,
    ) -> AppResult<Vec<ExecutionLogRow>> {
        let mut rows = sqlx::query_as::<_, ExecutionLogTuple>(
            r#"
            select *
            from (
                select
                    concat('elog_outbox_enqueued_', id) as id,
                    correlation_id,
                    correlation_id as story_id,
                    id as execution_id,
                    'outbox_event'::text as execution_type,
                    event_name as execution_name,
                    created_at as occurred_at,
                    'info'::text as severity,
                    'Outbox event enqueued'::text as body,
                    jsonb_build_object(
                        'event_name', event_name,
                        'event_version', event_version,
                        'aggregate_type', aggregate_type,
                        'aggregate_id', aggregate_id,
                        'source_module', source_module
                    ) as attributes,
                    headers #>> '{trace,trace_id}' as trace_id,
                    headers #>> '{trace,span_id}' as span_id,
                    source_module as service_name,
                    array[]::text[] as redacted_fields
                from platform.outbox
                where id = $1

                union all

                select
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
                from platform.execution_logs
                where execution_id = $1
            ) execution_log_rows
            where ($2::timestamptz is null or occurred_at < $2)
            order by occurred_at desc, id desc
            limit $3
            "#,
        )
        .bind(query.execution_id)
        .bind(query.occurred_before)
        .bind(query.limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_execution_log_error)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();

        rows.reverse();
        Ok(rows)
    }
}

type ExecutionLogTuple = (
    String,
    String,
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    String,
    String,
    Value,
    Option<String>,
    Option<String>,
    String,
    Vec<String>,
);

impl From<ExecutionLogTuple> for ExecutionLogRow {
    fn from(row: ExecutionLogTuple) -> Self {
        let (
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
            redacted_fields,
        ) = row;

        Self {
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
            redacted_fields,
        }
    }
}
