use crate::context::RequestContext;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RemoteHttpProxyCallRecord {
    pub module_name: String,
    pub method: String,
    pub declared_path: String,
    pub remote_path: String,
    pub capability: Option<String>,
    pub remote_status: Option<u16>,
    pub duration_ms: i64,
    pub success: bool,
    pub error_code: Option<String>,
    pub retryable: bool,
    pub path_params: Value,
    pub error_details: Value,
}

pub async fn insert_remote_http_proxy_call(
    pool: &DbPool,
    ids: &dyn crate::IdGenerator,
    request_ctx: &RequestContext,
    record: RemoteHttpProxyCallRecord,
) -> AppResult<String> {
    let id = ids.new_id("rproxy");
    let path_params = normalize_object(record.path_params.clone());
    let error_details = normalize_array(record.error_details.clone());
    let occurred_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"
        insert into platform.remote_http_proxy_calls (
            id,
            module_name,
            method,
            declared_path,
            remote_path,
            capability,
            remote_status,
            duration_ms,
            success,
            error_code,
            retryable,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        returning occurred_at
        "#,
    )
    .bind(&id)
    .bind(&record.module_name)
    .bind(&record.method)
    .bind(&record.declared_path)
    .bind(&record.remote_path)
    .bind(&record.capability)
    .bind(record.remote_status.map(i32::from))
    .bind(record.duration_ms)
    .bind(record.success)
    .bind(&record.error_code)
    .bind(record.retryable)
    .bind(&request_ctx.request_id.0)
    .bind(&request_ctx.correlation_id.0)
    .bind(&request_ctx.trace.trace_id)
    .bind(&request_ctx.trace.span_id)
    .bind(&path_params)
    .bind(&error_details)
    .fetch_one(pool)
    .await
    .map_err(map_remote_proxy_call_error)?;

    insert_remote_proxy_call_story_event(
        pool,
        &id,
        request_ctx,
        &record,
        &path_params,
        occurred_at,
    )
    .await?;

    Ok(id)
}

fn normalize_object(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        _ => Value::Object(Default::default()),
    }
}

fn normalize_array(value: Value) -> Value {
    match value {
        Value::Array(_) => value,
        _ => Value::Array(Vec::new()),
    }
}

fn map_remote_proxy_call_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Remote proxy call operation failed").with_source(source)
}

async fn insert_remote_proxy_call_story_event(
    pool: &DbPool,
    id: &str,
    request_ctx: &RequestContext,
    record: &RemoteHttpProxyCallRecord,
    path_params: &Value,
    occurred_at: DateTime<Utc>,
) -> AppResult<()> {
    let story_event_id = remote_proxy_call_story_event_id(id);
    let completed_at = occurred_at + Duration::milliseconds(record.duration_ms.max(0));
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
        values ($1, 'remote_proxy_call', $2, 'remote_proxy_call', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $9)
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
    .bind(story_event_id)
    .bind(id)
    .bind(remote_proxy_call_story_event_name(record))
    .bind(status)
    .bind(&record.module_name)
    .bind(&request_ctx.correlation_id.0)
    .bind(&request_ctx.causation_id)
    .bind(occurred_at)
    .bind(completed_at)
    .bind(record.duration_ms)
    .bind(remote_proxy_call_story_event_error(record))
    .bind(remote_proxy_call_story_event_metadata(
        id,
        request_ctx,
        record,
        path_params,
    ))
    .bind(&request_ctx.trace.trace_id)
    .bind(&request_ctx.trace.span_id)
    .execute(pool)
    .await
    .map_err(map_remote_proxy_call_error)?;

    Ok(())
}

pub fn remote_proxy_call_story_event_id(id: &str) -> String {
    format!("remoteproxy_{id}")
}

fn remote_proxy_call_story_event_name(record: &RemoteHttpProxyCallRecord) -> String {
    format!(
        "{} {} {}",
        record.module_name, record.method, record.declared_path
    )
}

fn remote_proxy_call_story_event_error(record: &RemoteHttpProxyCallRecord) -> Option<String> {
    if record.success {
        return None;
    }

    Some(match record.error_code.as_deref() {
        Some(error_code) => format!("remote proxy call failed with {error_code}"),
        None => "remote proxy call failed".to_owned(),
    })
}

fn remote_proxy_call_story_event_metadata(
    id: &str,
    request_ctx: &RequestContext,
    record: &RemoteHttpProxyCallRecord,
    path_params: &Value,
) -> Value {
    serde_json::json!({
        "remote_proxy_call_id": id,
        "module_name": &record.module_name,
        "method": &record.method,
        "declared_path": &record.declared_path,
        "remote_path": &record.remote_path,
        "capability": &record.capability,
        "remote_status": record.remote_status,
        "duration_ms": record.duration_ms,
        "request_id": request_ctx.request_id.0,
        "trace_id": request_ctx.trace.trace_id,
        "span_id": request_ctx.trace.span_id,
        "success": record.success,
        "error_code": &record.error_code,
        "retryable": record.retryable,
        "path_params": path_params,
    })
}
