use chrono::{DateTime, Utc};
use platform_core::ExecutionId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct FlowDefinition {
    pub name: &'static str,
    pub version: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FlowRun {
    pub id: ExecutionId,
    pub flow_name: String,
    pub version: u16,
    pub state: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
