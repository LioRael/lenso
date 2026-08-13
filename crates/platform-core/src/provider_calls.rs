use crate::context::RequestContext;
use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHttpBodyCaptureStatus {
    Captured,
    NotApplicable,
    NotCaptured,
}

impl ProviderHttpBodyCaptureStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::NotApplicable => "not_applicable",
            Self::NotCaptured => "not_captured",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderHttpBodyEvidence {
    body: Option<Value>,
    capture_status: ProviderHttpBodyCaptureStatus,
    capture_reason: Option<String>,
    observed_bytes: Option<i64>,
}

impl ProviderHttpBodyEvidence {
    #[must_use]
    pub fn captured(body: Value, observed_bytes: usize) -> Self {
        Self {
            body: Some(body),
            capture_status: ProviderHttpBodyCaptureStatus::Captured,
            capture_reason: None,
            observed_bytes: Some(i64::try_from(observed_bytes).unwrap_or(i64::MAX)),
        }
    }

    #[must_use]
    pub fn not_applicable(reason: impl Into<String>) -> Self {
        Self {
            body: None,
            capture_status: ProviderHttpBodyCaptureStatus::NotApplicable,
            capture_reason: Some(reason.into()),
            observed_bytes: None,
        }
    }

    #[must_use]
    pub fn not_captured(reason: impl Into<String>, observed_bytes: Option<usize>) -> Self {
        Self {
            body: None,
            capture_status: ProviderHttpBodyCaptureStatus::NotCaptured,
            capture_reason: Some(reason.into()),
            observed_bytes: observed_bytes.map(|bytes| i64::try_from(bytes).unwrap_or(i64::MAX)),
        }
    }

    #[must_use]
    pub const fn body(&self) -> Option<&Value> {
        self.body.as_ref()
    }

    #[must_use]
    pub const fn capture_status(&self) -> ProviderHttpBodyCaptureStatus {
        self.capture_status
    }

    #[must_use]
    pub fn capture_reason(&self) -> Option<&str> {
        self.capture_reason.as_deref()
    }

    #[must_use]
    pub const fn observed_bytes(&self) -> Option<i64> {
        self.observed_bytes
    }
}

#[derive(Debug, Clone)]
pub struct ProviderHttpCallRecord {
    pub module_name: String,
    pub method: String,
    pub declared_path: String,
    pub provider_path: String,
    pub capability: Option<String>,
    pub display_name: Option<String>,
    pub story_title: Option<String>,
    pub provider_status: Option<u16>,
    pub duration_ms: i64,
    pub success: bool,
    pub error_code: Option<String>,
    pub retryable: bool,
    pub path_params: Value,
    pub error_details: Value,
    pub request_body: ProviderHttpBodyEvidence,
    pub response_body: ProviderHttpBodyEvidence,
}

pub async fn insert_provider_http_call(
    pool: &DbPool,
    ids: &dyn crate::IdGenerator,
    request_ctx: &RequestContext,
    record: ProviderHttpCallRecord,
) -> AppResult<String> {
    let id = ids.new_id("rproxy");
    let path_params = normalize_object(record.path_params.clone());
    let error_details = normalize_array(record.error_details.clone());
    let occurred_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"
        insert into platform.provider_http_calls (
            id,
            module_name,
            method,
            declared_path,
            provider_path,
            capability,
            provider_status,
            duration_ms,
            success,
            error_code,
            retryable,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details,
            request_body,
            request_body_capture_status,
            request_body_capture_reason,
            request_body_observed_bytes,
            response_body,
            response_body_capture_status,
            response_body_capture_reason,
            response_body_observed_bytes
        )
        values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20, $21, $22, $23, $24, $25
        )
        returning occurred_at
        "#,
    )
    .bind(&id)
    .bind(&record.module_name)
    .bind(&record.method)
    .bind(&record.declared_path)
    .bind(&record.provider_path)
    .bind(&record.capability)
    .bind(record.provider_status.map(i32::from))
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
    .bind(&record.request_body.body)
    .bind(record.request_body.capture_status.as_str())
    .bind(&record.request_body.capture_reason)
    .bind(record.request_body.observed_bytes)
    .bind(&record.response_body.body)
    .bind(record.response_body.capture_status.as_str())
    .bind(&record.response_body.capture_reason)
    .bind(record.response_body.observed_bytes)
    .fetch_one(pool)
    .await
    .map_err(map_provider_call_error)?;

    insert_provider_call_story_event(pool, &id, request_ctx, &record, &path_params, occurred_at)
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

fn map_provider_call_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Remote proxy call operation failed").with_source(source)
}

async fn insert_provider_call_story_event(
    pool: &DbPool,
    id: &str,
    request_ctx: &RequestContext,
    record: &ProviderHttpCallRecord,
    path_params: &Value,
    occurred_at: DateTime<Utc>,
) -> AppResult<()> {
    let story_event_id = provider_call_story_event_id(id);
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
        values ($1, 'provider_call', $2, 'provider_call', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $9)
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
    .bind(provider_call_story_event_name(record))
    .bind(status)
    .bind(&record.module_name)
    .bind(&request_ctx.correlation_id.0)
    .bind(&request_ctx.causation_id)
    .bind(occurred_at)
    .bind(completed_at)
    .bind(record.duration_ms)
    .bind(provider_call_story_event_error(record))
    .bind(provider_call_story_event_metadata(
        id,
        request_ctx,
        record,
        path_params,
    ))
    .bind(&request_ctx.trace.trace_id)
    .bind(&request_ctx.trace.span_id)
    .execute(pool)
    .await
    .map_err(map_provider_call_error)?;

    Ok(())
}

pub fn provider_call_story_event_id(id: &str) -> String {
    format!("remoteproxy_{id}")
}

fn provider_call_story_event_name(record: &ProviderHttpCallRecord) -> String {
    if let Some(display_name) = record.display_name.as_deref() {
        return display_name.to_owned();
    }

    format!(
        "{} {} {}",
        record.module_name, record.method, record.declared_path
    )
}

fn provider_call_story_event_error(record: &ProviderHttpCallRecord) -> Option<String> {
    if record.success {
        return None;
    }

    Some(match record.error_code.as_deref() {
        Some(error_code) => format!("remote proxy call failed with {error_code}"),
        None => "remote proxy call failed".to_owned(),
    })
}

fn provider_call_story_event_metadata(
    id: &str,
    request_ctx: &RequestContext,
    record: &ProviderHttpCallRecord,
    path_params: &Value,
) -> Value {
    serde_json::json!({
        "provider_call_id": id,
        "module_name": &record.module_name,
        "method": &record.method,
        "declared_path": &record.declared_path,
        "provider_path": &record.provider_path,
        "capability": &record.capability,
        "display_name": &record.display_name,
        "story_title": &record.story_title,
        "provider_status": record.provider_status,
        "duration_ms": record.duration_ms,
        "request_id": request_ctx.request_id.0,
        "trace_id": request_ctx.trace.trace_id,
        "span_id": request_ctx.trace.span_id,
        "success": record.success,
        "error_code": &record.error_code,
        "retryable": record.retryable,
        "path_params": path_params,
        "error_details": &record.error_details,
        "request_body_capture_status": record.request_body.capture_status.as_str(),
        "request_body_capture_reason": &record.request_body.capture_reason,
        "request_body_observed_bytes": record.request_body.observed_bytes,
        "response_body_capture_status": record.response_body.capture_status.as_str(),
        "response_body_capture_reason": &record.response_body.capture_reason,
        "response_body_observed_bytes": record.response_body.observed_bytes,
    })
}
