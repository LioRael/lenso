//! Runtime Console contribution contracts.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConsoleSurface {
    pub name: String,
    pub label: String,
    pub area: ConsoleArea,
    pub route: String,
    pub package: ConsolePackage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleArea {
    Runtime,
    Operations,
    Data,
    Configuration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ConsolePackage {
    pub name: String,
    pub export: String,
}
