use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const STORY_SEGMENT_FEED_PROTOCOL: &str = "lenso.story-segment-feed.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentSource {
    pub service_id: String,
    pub workload_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentOperation {
    pub kind: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentContract {
    pub contract_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentWorkflow {
    pub instance_id: String,
    pub definition_owner: String,
    pub definition_name: String,
    pub definition_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compensation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intervention_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegment {
    pub story_id: String,
    pub segment_id: String,
    pub evidence_revision: u32,
    pub source: StorySegmentSource,
    pub operation: StorySegmentOperation,
    pub contract: StorySegmentContract,
    pub status: String,
    pub attempt: u32,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_segment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<StorySegmentWorkflow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentFeed {
    pub protocol: String,
    pub source_service_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub retention_window_seconds: u64,
    pub as_of: DateTime<Utc>,
    pub segments: Vec<StorySegment>,
    pub next_cursor: String,
}
