use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoryDisplaySource {
    ExecutionName { name: String },
    HttpRequest { method: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StoryDisplayDescriptor {
    pub source: StoryDisplaySource,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_title: Option<String>,
}
