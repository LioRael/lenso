use crate::context::RequestContext;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
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
    sqlx::query(
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
    .bind(normalize_object(record.path_params))
    .bind(normalize_array(record.error_details))
    .execute(pool)
    .await
    .map_err(map_remote_proxy_call_error)?;

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
