#[allow(clippy::wildcard_imports)]
use super::*;
use chrono::{DateTime, Utc};
use platform_core::ExecutionLogRow;
use serde_json::Value;
use sqlx::Row;

pub(crate) type OutboxAdminRow = (
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    DateTime<Utc>,
);

pub(crate) type FunctionRunAdminRow = (
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    DateTime<Utc>,
);

pub(crate) type OutboxDetailRow = (
    String,
    String,
    i32,
    String,
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
    Value,
    Value,
);

pub(crate) type FunctionRunDetailRow = (
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
    DateTime<Utc>,
    Value,
    Value,
);

pub(crate) type StoryEventDetailRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    i64,
    Option<String>,
    Value,
    Option<String>,
    Option<String>,
);

pub(crate) type TimelineRow = (
    String,
    String,
    String,
    String,
    i32,
    i32,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    String,
);

pub(crate) type SummaryCountRow = (i64, i64, i64, i64, i64, Option<i64>, Option<i64>);

pub(crate) type SummaryItemRow = (
    String,
    String,
    String,
    String,
    i32,
    i32,
    Option<String>,
    DateTime<Utc>,
    Option<String>,
);

pub(crate) type HeatmapRow = (
    DateTime<Utc>,
    DateTime<Utc>,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
);

pub(crate) type RuntimeNodeRefTuple = (String, String, String);

#[derive(Debug, Clone)]
pub(crate) struct RuntimeNodeRef {
    pub(crate) id: String,
    pub(crate) item_type: String,
    pub(crate) correlation_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StoryWorkRow {
    pub(crate) item_type: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) attempts: i32,
    pub(crate) max_attempts: i32,
    pub(crate) correlation_id: String,
    pub(crate) causation_id: Option<String>,
    pub(crate) service: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) started_at: Option<DateTime<Utc>>,
    pub(crate) completed_at: Option<DateTime<Utc>>,
    pub(crate) last_error: Option<String>,
    pub(crate) metadata: Value,
}

pub(crate) type StoryWorkTuple = (
    String,
    String,
    String,
    String,
    i32,
    i32,
    String,
    Option<String>,
    String,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    Value,
);

impl From<OutboxAdminRow> for AdminOutboxEvent {
    fn from(row: OutboxAdminRow) -> Self {
        let (
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at,
        ) = row;

        Self {
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at,
        }
    }
}

impl From<SummaryCountRow> for AdminRuntimeOutboxSummary {
    fn from(row: SummaryCountRow) -> Self {
        let (
            pending,
            processing,
            published,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        ) = row;

        Self {
            pending,
            processing,
            published,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        }
    }
}

impl From<SummaryCountRow> for AdminRuntimeFunctionSummary {
    fn from(row: SummaryCountRow) -> Self {
        let (
            pending,
            running,
            completed,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        ) = row;

        Self {
            pending,
            running,
            completed,
            failed,
            dead,
            oldest_pending_age_seconds,
            oldest_failed_age_seconds,
        }
    }
}

impl From<SummaryItemRow> for AdminRuntimeSummaryItem {
    fn from(row: SummaryItemRow) -> Self {
        let (
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            created_at,
            last_error,
        ) = row;

        Self {
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            created_at,
            last_error,
        }
    }
}

impl From<FunctionRunAdminRow> for AdminFunctionRun {
    fn from(row: FunctionRunAdminRow) -> Self {
        let (
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at,
        ) = row;

        Self {
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at,
            runtime_declaration: None,
        }
    }
}

pub(crate) fn remote_proxy_call_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AdminRemoteProxyCall, sqlx::Error> {
    Ok(AdminRemoteProxyCall {
        id: row.try_get("id")?,
        module_name: row.try_get("module_name")?,
        method: row.try_get("method")?,
        declared_path: row.try_get("declared_path")?,
        remote_path: row.try_get("remote_path")?,
        capability: row.try_get("capability")?,
        remote_status: row.try_get("remote_status")?,
        duration_ms: row.try_get("duration_ms")?,
        success: row.try_get("success")?,
        error_code: row.try_get("error_code")?,
        retryable: row.try_get("retryable")?,
        request_id: row.try_get("request_id")?,
        correlation_id: row.try_get("correlation_id")?,
        trace_id: row.try_get("trace_id")?,
        span_id: row.try_get("span_id")?,
        path_params: row.try_get("path_params")?,
        error_details: row.try_get("error_details")?,
        occurred_at: row.try_get("occurred_at")?,
    })
}

impl From<OutboxDetailRow> for AdminOutboxEventDetail {
    fn from(row: OutboxDetailRow) -> Self {
        let (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            causation_id,
            occurred_at,
            created_at,
            payload,
            headers,
        ) = row;

        let actor = headers
            .get("actor")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));
        let trace = headers
            .get("trace")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()));

        Self {
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            causation_id,
            occurred_at,
            created_at,
            payload,
            actor,
            trace,
            headers,
        }
    }
}

impl From<FunctionRunDetailRow> for AdminFunctionRunDetail {
    fn from(row: FunctionRunDetailRow) -> Self {
        let (
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at,
            input_json,
            actor,
        ) = row;

        Self {
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at,
            input_json,
            actor,
            runtime_declaration: None,
        }
    }
}

impl From<StoryEventDetailRow> for StoryEventDetail {
    fn from(row: StoryEventDetailRow) -> Self {
        let (
            id,
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
        ) = row;

        Self {
            id,
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
        }
    }
}

impl From<TimelineRow> for AdminRuntimeTimelineItem {
    fn from(row: TimelineRow) -> Self {
        let (
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            created_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
        ) = row;
        let related_node_id = Some(id.clone());

        Self {
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            created_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            related_node_id,
        }
    }
}

impl From<StoryWorkTuple> for StoryWorkRow {
    fn from(row: StoryWorkTuple) -> Self {
        let (
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            causation_id,
            service,
            created_at,
            started_at,
            completed_at,
            last_error,
            metadata,
        ) = row;

        Self {
            item_type,
            id,
            name,
            status,
            attempts,
            max_attempts,
            correlation_id,
            causation_id,
            service,
            created_at,
            started_at,
            completed_at,
            last_error,
            metadata,
        }
    }
}

impl From<HeatmapRow> for AdminRuntimeHeatmapCell {
    fn from(row: HeatmapRow) -> Self {
        let (
            bucket_start,
            bucket_end,
            service,
            node_type,
            total_count,
            error_count,
            retry_count,
            dead_count,
            avg_duration_ms,
            max_duration_ms,
        ) = row;

        Self {
            bucket_start,
            bucket_end,
            service,
            node_type,
            total_count,
            error_count,
            retry_count,
            dead_count,
            avg_duration_ms,
            max_duration_ms,
        }
    }
}

impl From<RuntimeNodeRefTuple> for RuntimeNodeRef {
    fn from(row: RuntimeNodeRefTuple) -> Self {
        let (id, item_type, correlation_id) = row;
        Self {
            id,
            item_type,
            correlation_id,
        }
    }
}

impl From<ExecutionLogRow> for AdminRuntimeExecutionLog {
    fn from(row: ExecutionLogRow) -> Self {
        Self {
            id: row.id,
            node_id: row.execution_id,
            node_type: row.execution_type,
            correlation_id: row.correlation_id,
            story_id: row.story_id,
            execution_name: row.execution_name,
            occurred_at: row.occurred_at,
            severity: row.severity,
            body: row.body,
            attributes: row.attributes,
            service_name: row.service_name,
            trace_id: row.trace_id,
            span_id: row.span_id,
            redacted_fields: row.redacted_fields,
        }
    }
}

impl From<&StoryWorkRow> for AdminRuntimeTimelineItem {
    fn from(row: &StoryWorkRow) -> Self {
        Self {
            item_type: timeline_item_type(&row.item_type, &row.status, row.attempts).to_owned(),
            id: row.id.clone(),
            name: row.name.clone(),
            status: row.status.clone(),
            attempts: row.attempts,
            max_attempts: row.max_attempts,
            created_at: row.created_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
            last_error: row.last_error.clone(),
            correlation_id: row.correlation_id.clone(),
            related_node_id: Some(row.id.clone()),
        }
    }
}

pub(crate) fn timeline_item_type(item_type: &str, status: &str, attempts: i32) -> &'static str {
    if status == "dead" {
        return "dead_letter";
    }
    if status == "failed" {
        return "failure";
    }
    if attempts > 1 {
        return "retry";
    }
    match item_type {
        "http" | "http_request" => "http_request",
        "event" | "outbox_event" => "outbox_event",
        "function" | "function_run" => "function_run",
        "remote_proxy_call" => "remote_proxy_call",
        _ => "runtime",
    }
}
