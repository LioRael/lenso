//! Lenso Console Module declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

pub const CONSOLE_BRIDGE_PROTOCOL: &str = "lenso.console-bridge.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleSurface {
    pub name: String,
    pub label: String,
    pub route: String,
    pub presentation: ConsoleSurfacePresentation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<ConsoleNavigation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleSurfacePresentation {
    Declarative {
        schema: Value,
    },
    Isolated {
        entry: String,
        bridge_protocol: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsolePermissionRequest {
    pub permission_id: String,
    #[serde(default)]
    pub operations: Vec<String>,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default)]
    pub outbound_destinations: Vec<String>,
    #[serde(default)]
    pub secret_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsolePermissionGrant {
    pub module_id: String,
    pub module_release_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_artifact_digest: Option<String>,
    #[serde(default)]
    pub granted_permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleContribution {
    pub target: String,
    pub target_version: u32,
    pub label: String,
    pub action: ConsoleContributionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleSlot {
    pub id: String,
    pub version: u32,
    pub label: String,
    #[serde(default)]
    pub accepts: Vec<ConsoleContributionKind>,
    #[serde(default)]
    pub context: Vec<ConsoleSlotContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleContributionKind {
    AdminAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleSlotContext {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<ConsoleSlotContextField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleSlotContextField {
    pub name: String,
    pub field_type: ConsoleSlotContextFieldType,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleSlotContextFieldType {
    String,
    Boolean,
    Number,
    Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleContributionAction {
    AdminAction {
        module: String,
        name: String,
        #[serde(default)]
        input_bindings: Vec<ConsoleActionInputBinding>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleActionInputBinding {
    pub input: String,
    pub value: ConsoleActionInputValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleActionInputValue {
    SlotContext { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleNavigation {
    pub workspace: ConsoleWorkspaceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<ConsoleNavigationGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleWorkspaceRef {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ConsoleNavigationGroup {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}
