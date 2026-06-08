#[allow(clippy::wildcard_imports)]
use super::*;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StoryQuery {
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HeatmapQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub bucket_seconds: Option<i64>,
    pub status: Option<String>,
    pub event_name: Option<String>,
    pub function_name: Option<String>,
    pub limit: Option<i64>,
    pub created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageInfo {
    pub limit: i64,
    pub next_created_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeStoryListResponse)]
pub struct AdminRuntimeStoryListResponse {
    pub data: Vec<AdminRuntimeStoryListItem>,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeStoryDetailResponse)]
pub struct AdminRuntimeStoryDetailResponse {
    pub data: AdminRuntimeStoryDetail,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeHeatmapResponse)]
pub struct AdminRuntimeHeatmapResponse {
    pub data: Vec<AdminRuntimeHeatmapCell>,
    pub bucket_seconds: i64,
    pub page: PageInfo,
    pub order: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRuntimeTechnicalOperationListResponse)]
pub struct AdminRuntimeTechnicalOperationListResponse {
    pub data: Vec<AdminRuntimeTechnicalOperation>,
    pub order: &'static str,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminRuntimeStoryListItem {
    pub title: String,
    pub correlation_id: String,
    pub status: String,
    pub duration: i64,
    pub node_count: usize,
    pub error_count: usize,
    pub services: Vec<String>,
    pub pattern: Vec<String>,
    pub root_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeStoryDetail {
    pub summary: AdminRuntimeStoryListItem,
    pub nodes: Vec<AdminRuntimeStoryNode>,
    pub edges: Vec<AdminRuntimeStoryEdge>,
    pub timeline_items: Vec<AdminRuntimeTimelineItem>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminRuntimeStoryNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub service: String,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: i64,
    pub error: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeStoryEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeHeatmapCell {
    pub bucket_start: DateTime<Utc>,
    pub bucket_end: DateTime<Utc>,
    pub service: String,
    pub node_type: String,
    pub total_count: i64,
    pub error_count: i64,
    pub retry_count: i64,
    pub dead_count: i64,
    pub avg_duration_ms: Option<i64>,
    pub max_duration_ms: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeTechnicalOperation {
    pub id: String,
    pub story_id: String,
    pub correlation_id: String,
    pub related_node_id: Option<String>,
    pub category: String,
    pub name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub attributes: Value,
    pub source: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRuntimeTimelineItem {
    #[serde(rename = "type")]
    pub item_type: String,
    pub id: String,
    pub name: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub correlation_id: String,
    pub related_node_id: Option<String>,
}
