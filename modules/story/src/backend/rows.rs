#[allow(clippy::wildcard_imports)]
use super::*;

#[derive(Debug, Clone)]
pub(super) struct StoryWorkRow {
    pub(super) item_type: String,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) status: String,
    pub(super) attempts: i32,
    pub(super) max_attempts: i32,
    pub(super) correlation_id: String,
    pub(super) causation_id: Option<String>,
    pub(super) service: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) started_at: Option<DateTime<Utc>>,
    pub(super) completed_at: Option<DateTime<Utc>>,
    pub(super) last_error: Option<String>,
    pub(super) metadata: Value,
}

pub(super) type StoryWorkTuple = (
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

pub(super) type HeatmapRow = (
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

pub(super) type ExecutionLogTuple = (
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

pub(super) struct AdminRemoteProxyCall {
    pub(super) id: String,
    pub(super) module_name: String,
    pub(super) method: String,
    pub(super) declared_path: String,
    pub(super) remote_path: String,
    pub(super) capability: Option<String>,
    pub(super) remote_status: Option<i32>,
    pub(super) duration_ms: i64,
    pub(super) success: bool,
    pub(super) error_code: Option<String>,
    pub(super) retryable: bool,
    pub(super) request_id: String,
    pub(super) correlation_id: String,
    pub(super) trace_id: Option<String>,
    pub(super) span_id: Option<String>,
    pub(super) occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct RuntimeNodeIndex {
    pub(super) ids: std::collections::BTreeSet<String>,
}

impl RuntimeNodeIndex {
    pub(super) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
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
