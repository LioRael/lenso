use crate::context::{ActorContext, RequestContext};
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct HttpRequestStoryEventRecord {
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub creation: HttpRequestStoryCreation,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum HttpRequestStoryCreation {
    Never,
    Always,
    WhenRuntimeWorkExists,
}

#[doc(hidden)]
pub async fn insert_http_request_story_projection(
    pool: &DbPool,
    request_ctx: &RequestContext,
    record: HttpRequestStoryEventRecord,
) -> AppResult<String> {
    let id = http_request_story_event_id(request_ctx);
    if !should_insert_http_request_story(pool, request_ctx, &record, &id).await? {
        return Ok(id);
    }

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
    .bind(http_request_status(&record))
    .bind("api")
    .bind(&request_ctx.correlation_id.0)
    .bind(None::<String>)
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

async fn should_insert_http_request_story(
    pool: &DbPool,
    request_ctx: &RequestContext,
    record: &HttpRequestStoryEventRecord,
    story_event_id: &str,
) -> AppResult<bool> {
    match record.creation {
        HttpRequestStoryCreation::Never => Ok(false),
        HttpRequestStoryCreation::Always => Ok(true),
        HttpRequestStoryCreation::WhenRuntimeWorkExists => {
            request_has_runtime_work(pool, request_ctx, story_event_id).await
        }
    }
}

async fn request_has_runtime_work(
    pool: &DbPool,
    request_ctx: &RequestContext,
    story_event_id: &str,
) -> AppResult<bool> {
    let has_outbox_work = sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
            select 1
            from platform.outbox
            where correlation_id = $1
                and causation_id = $2
        )
        "#,
    )
    .bind(&request_ctx.correlation_id.0)
    .bind(story_event_id)
    .fetch_one(pool)
    .await
    .map_err(map_story_event_error)?;

    if has_outbox_work {
        return Ok(true);
    }

    let function_work = sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
            select 1
            from runtime.function_runs
            where correlation_id = $1
                and input_json #>> '{_lenso_runtime,causation_id}' = $2
        )
        "#,
    )
    .bind(&request_ctx.correlation_id.0)
    .bind(story_event_id)
    .fetch_one(pool)
    .await;

    match function_work {
        Ok(has_function_work) => Ok(has_function_work),
        Err(error) if is_missing_runtime_relation(&error) => Ok(false),
        Err(error) => Err(map_story_event_error(error)),
    }
}

fn http_request_status(record: &HttpRequestStoryEventRecord) -> &'static str {
    if record.status_code >= 400 {
        "failed"
    } else {
        "completed"
    }
}

fn http_request_error(record: &HttpRequestStoryEventRecord) -> Option<String> {
    if record.status_code < 400 {
        return None;
    }

    Some(match record.error_code.as_deref() {
        Some(error_code) => format!("HTTP {} {error_code}", record.status_code),
        None => format!("HTTP request failed with status {}", record.status_code),
    })
}

pub fn http_request_story_event_id(request_ctx: &RequestContext) -> String {
    format!("httpreq_{}", request_ctx.request_id.0)
}

pub fn http_request_story_creation(path: &str, status_code: u16) -> HttpRequestStoryCreation {
    if is_console_or_internal_path(path) {
        return HttpRequestStoryCreation::Never;
    }
    if status_code >= 500 {
        return HttpRequestStoryCreation::Always;
    }
    if status_code < 400 {
        return if path.starts_with("/v1/") {
            HttpRequestStoryCreation::WhenRuntimeWorkExists
        } else {
            HttpRequestStoryCreation::Never
        };
    }

    if path.starts_with("/v1/") && matches!(status_code, 400 | 403 | 409 | 422) {
        HttpRequestStoryCreation::Always
    } else {
        HttpRequestStoryCreation::Never
    }
}

fn is_console_or_internal_path(path: &str) -> bool {
    path.starts_with("/admin/runtime")
        || path == "/docs"
        || path == "/openapi.json"
        || path.ends_with("/health")
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

fn is_missing_runtime_relation(error: &sqlx::Error) -> bool {
    let Some(database_error) = error.as_database_error() else {
        return false;
    };

    matches!(database_error.code().as_deref(), Some("3F000" | "42P01"))
}
