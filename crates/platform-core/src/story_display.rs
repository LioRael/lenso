use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoryDisplaySource {
    ExecutionName { name: String },
    HttpRequest { method: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryDisplayDescriptor {
    pub source: StoryDisplaySource,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story_title: Option<String>,
}
