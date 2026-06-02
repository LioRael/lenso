use crate::context::{ActorContext, RequestContext};
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct HttpRequestStoryEventRecord {
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: i64,
}

#[doc(hidden)]
pub async fn insert_http_request_story_projection(
    pool: &DbPool,
    request_ctx: &RequestContext,
    record: HttpRequestStoryEventRecord,
) -> AppResult<String> {
    let id = format!("storyevt_{}", Uuid::now_v7());
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
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        on conflict (source_type, source_id) do update
        set
            status = excluded.status,
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
    .bind("http_request")
    .bind(&request_ctx.request_id.0)
    .bind("http_request")
    .bind(format!("{} {}", record.method, record.path))
    .bind("failed")
    .bind("api")
    .bind(&request_ctx.correlation_id.0)
    .bind(&request_ctx.causation_id)
    .bind(record.started_at)
    .bind(record.completed_at)
    .bind(record.duration_ms)
    .bind(http_request_error(&record))
    .bind(http_request_metadata(request_ctx, &record))
    .bind(&request_ctx.trace.trace_id)
    .bind(&request_ctx.trace.span_id)
    .bind(record.completed_at)
    .execute(pool)
    .await
    .map_err(map_story_event_error)?;

    Ok(id)
}

fn http_request_error(record: &HttpRequestStoryEventRecord) -> String {
    match record.error_code.as_deref() {
        Some(error_code) => format!("HTTP {} {error_code}", record.status_code),
        None => format!("HTTP request failed with status {}", record.status_code),
    }
}

fn http_request_metadata(
    request_ctx: &RequestContext,
    record: &HttpRequestStoryEventRecord,
) -> Value {
    json!({
        "request_id": request_ctx.request_id.0,
        "method": record.method,
        "path": record.path,
        "status_code": record.status_code,
        "error_code": record.error_code,
        "duration_ms": record.duration_ms,
        "actor_kind": actor_kind(&request_ctx.actor),
    })
}

fn actor_kind(actor: &ActorContext) -> &'static str {
    match actor {
        ActorContext::Anonymous => "anonymous",
        ActorContext::User { .. } => "user",
        ActorContext::Service { .. } => "service",
        ActorContext::System => "system",
    }
}

fn map_story_event_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Story event operation failed").with_source(source)
}
