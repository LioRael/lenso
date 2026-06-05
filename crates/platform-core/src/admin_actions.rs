use crate::context::{ActorContext, RequestContext};
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct AdminActionStoryRecord {
    pub module_name: String,
    pub action_name: String,
    pub label: String,
    pub capability: String,
    pub input: Value,
    pub result: Option<Value>,
    pub success: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub duration_ms: i64,
}

pub async fn insert_admin_action_story_event(
    pool: &DbPool,
    request_ctx: &RequestContext,
    record: AdminActionStoryRecord,
) -> AppResult<String> {
    let id = admin_action_story_event_id(request_ctx);
    let completed_at = record.started_at + Duration::milliseconds(record.duration_ms.max(0));
    let status = if record.success {
        "completed"
    } else {
        "failed"
    };

    sqlx::query(
        r#"
        insert into platform.story_events (
            id,
            source_type,
            source_id,
            node_type,
            name,
            status,
            service,
            correlation_id,
            causation_id,
            started_at,
            completed_at,
            duration_ms,
            error,
            metadata,
            trace_id,
            span_id,
            updated_at
        )
        values ($1, 'admin_action', $2, 'admin_action', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $9)
        on conflict (source_type, source_id) do update
        set
            name = excluded.name,
            status = excluded.status,
            service = excluded.service,
            correlation_id = excluded.correlation_id,
            causation_id = excluded.causation_id,
            started_at = excluded.started_at,
            completed_at = excluded.completed_at,
            duration_ms = excluded.duration_ms,
            error = excluded.error,
            metadata = excluded.metadata,
            trace_id = excluded.trace_id,
            span_id = excluded.span_id,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&id)
    .bind(&request_ctx.request_id.0)
    .bind(&record.label)
    .bind(status)
    .bind(&record.module_name)
    .bind(&request_ctx.correlation_id.0)
    .bind(&request_ctx.causation_id)
    .bind(record.started_at)
    .bind(completed_at)
    .bind(record.duration_ms)
    .bind(record.error_message.clone())
    .bind(admin_action_metadata(request_ctx, &record))
    .bind(&request_ctx.trace.trace_id)
    .bind(&request_ctx.trace.span_id)
    .execute(pool)
    .await
    .map_err(map_admin_action_error)?;

    Ok(id)
}

pub fn admin_action_story_event_id(request_ctx: &RequestContext) -> String {
    format!("adminaction_{}", request_ctx.request_id.0)
}

fn admin_action_metadata(request_ctx: &RequestContext, record: &AdminActionStoryRecord) -> Value {
    json!({
        "module_name": &record.module_name,
        "action_name": &record.action_name,
        "label": &record.label,
        "capability": &record.capability,
        "duration_ms": record.duration_ms,
        "request_id": request_ctx.request_id.0,
        "trace_id": request_ctx.trace.trace_id,
        "span_id": request_ctx.trace.span_id,
        "actor_kind": actor_kind(&request_ctx.actor),
        "success": record.success,
        "error_code": &record.error_code,
        "error_message": &record.error_message,
        "input_summary": value_summary(&record.input),
        "result_summary": record.result.as_ref().map(value_summary),
    })
}

fn value_summary(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => truncate_summary(value),
        Value::Array(items) => format!("{} items", items.len()),
        Value::Object(entries) if entries.is_empty() => "{}".to_owned(),
        Value::Object(entries) => truncate_summary(
            &entries
                .iter()
                .take(4)
                .map(|(key, value)| format!("{key}: {}", scalar_summary(value)))
                .collect::<Vec<_>>()
                .join(" / "),
        ),
    }
}

fn scalar_summary(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(items) => format!("{} items", items.len()),
        Value::Object(_) => "{...}".to_owned(),
    }
}

fn truncate_summary(value: &str) -> String {
    const LIMIT: usize = 160;
    if value.chars().count() <= LIMIT {
        return value.to_owned();
    }
    format!(
        "{}...",
        value
            .chars()
            .take(LIMIT.saturating_sub(3))
            .collect::<String>()
    )
}

fn actor_kind(actor: &ActorContext) -> &'static str {
    match actor {
        ActorContext::Anonymous => "anonymous",
        ActorContext::User { .. } => "user",
        ActorContext::Service { .. } => "service",
        ActorContext::System => "system",
    }
}

fn map_admin_action_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Admin action story operation failed").with_source(source)
}
