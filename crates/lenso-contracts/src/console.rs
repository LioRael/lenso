//! Lenso Console Module declarations and the framework-owned ESM contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

/// The versioned manifest consumed by the Console ESM loader.
pub const CONSOLE_MODULE_PROTOCOL: &str = "lenso.console-module.v1";
/// The major protocol understood by the current Console ESM loader.
pub const CONSOLE_MODULE_PROTOCOL_MAJOR: u32 = 1;
/// The only executable Console artifact format supported by the framework.
pub const CONSOLE_UI_ESM_FORMAT: &str = "console_ui_esm";
/// Retired bridge identifier retained only so older published authoring crates
/// can be rebuilt while their releases are rejected by contract validation.
pub const CONSOLE_BRIDGE_PROTOCOL: &str = "lenso.console-bridge.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConsoleSurfaceArea {
    Runtime,
    Operations,
    Data,
    Configuration,
}

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
    /// A same-realm ESM surface loaded from the owning Module Release.
    Esm {
        entry: String,
    },
    /// Retired compatibility authoring shape. Module Releases reject it.
    #[schemars(skip)]
    Isolated {
        entry: String,
        bridge_protocol: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleModuleSurface {
    pub id: String,
    pub path: String,
    pub label: String,
    pub area: ConsoleSurfaceArea,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<ConsoleNavigation>,
}

/// The immutable presentation manifest embedded in a `console_ui_esm`
/// artifact. Its field names intentionally match the public Console SDK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleModuleManifest {
    pub protocol: String,
    pub module_id: String,
    pub host_api: String,
    pub console_ui: String,
    pub surfaces: Vec<ConsoleModuleSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleUiArtifactStyleAsset {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct ConsoleNavigation {
    pub workspace: ConsoleWorkspaceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<ConsoleNavigationGroup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleWorkspaceRef {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleNavigationGroup {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}
