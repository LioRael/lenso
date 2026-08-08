use lenso_contracts::{
    ConsoleContributionAction, ModuleConfigActivation, ModuleConfigFieldType,
    ModuleConfigMutability, ModuleConfigScope, digest_json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::ToSchema;

pub const MODULE_OPERATIONS_PROTOCOL: &str = "lenso.system-plane.module-operations.v1";
pub const MODULE_OPERATIONS_PATH: &str = "/system-plane/v1/modules";
pub const MODULE_OPERATIONS_FEATURE_INVENTORY_READ: &str = "module.inventory.read";
pub const MODULE_OPERATIONS_FEATURE_CONTRIBUTIONS_RESOLVE: &str =
    "module.action-contributions.resolve";
pub const MODULE_OPERATIONS_FEATURE_CONFIG_READ: &str = "module.config.read";
pub const MODULE_OPERATIONS_FEATURE_CONFIG_WRITE: &str = "module.config.write";

/// The target and caller identity carried by every Module System Plane request.
/// The target Service verifies this against its authenticated transport,
/// enrollment grant, and installed Module declarations before executing an
/// operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedServiceContext {
    pub system_id: String,
    pub service_id: String,
    pub environment_id: String,
    pub target_service_principal: String,
    pub caller_module_id: String,
    pub delegated_actor_subject: String,
    pub delegated_authority_digest: String,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
}

impl ManagedServiceContext {
    #[must_use]
    pub fn new(
        system_id: impl Into<String>,
        service_id: impl Into<String>,
        environment_id: impl Into<String>,
        target_service_principal: impl Into<String>,
        caller_module_id: impl Into<String>,
        delegated_actor_subject: impl Into<String>,
        delegated_authority_digest: impl Into<String>,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            system_id: system_id.into(),
            service_id: service_id.into(),
            environment_id: environment_id.into(),
            target_service_principal: target_service_principal.into(),
            caller_module_id: caller_module_id.into(),
            delegated_actor_subject: delegated_actor_subject.into(),
            delegated_authority_digest: delegated_authority_digest.into(),
            capabilities: capabilities.into_iter().map(Into::into).collect(),
        }
    }

    /// Stable digest used in operation evidence. It never includes a secret
    /// value because the context contains only authority metadata.
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        digest_json(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInventoryRequest {
    pub context: ManagedServiceContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInventorySnapshot {
    pub protocol: String,
    pub context: ManagedServiceContext,
    pub service_revision: String,
    pub snapshot_revision: String,
    pub schema_digest: String,
    pub modules: Vec<ModuleInventoryModule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInventoryModule {
    pub module_id: String,
    pub version: String,
    pub release_digest: String,
    pub manifest_digest: String,
    pub delivery: ModuleInventoryDelivery,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_module_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<ModuleInventoryRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_functions: Vec<String>,
    pub runtime_status: ModuleRuntimeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_ui: Option<ModuleInventoryConsoleUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleInventoryDelivery {
    Linked,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRuntimeStatus {
    Active,
    Disabled,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInventoryRoute {
    pub method: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInventoryConsoleUi {
    pub format: String,
    pub protocol_major: u32,
    pub artifact_digest: String,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_assets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionContributionResolutionRequest {
    pub context: ManagedServiceContext,
    pub slot: String,
    pub slot_version: u32,
    #[serde(default)]
    pub slot_context: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionContributionResolution {
    pub protocol: String,
    pub context: ManagedServiceContext,
    pub slot: String,
    pub slot_version: u32,
    pub contributions: Vec<ResolvedActionContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedActionContribution {
    pub contributing_module_id: String,
    pub target: String,
    pub target_version: u32,
    pub label: String,
    pub action: ConsoleContributionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigReadRequest {
    pub context: ManagedServiceContext,
    pub module_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigReadResponse {
    pub protocol: String,
    pub context: ManagedServiceContext,
    pub module_id: String,
    pub values: Vec<ModuleConfigValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigValue {
    pub key: String,
    pub field_type: ModuleConfigFieldType,
    pub scope: ModuleConfigScope,
    pub mutability: ModuleConfigMutability,
    pub activation: ModuleConfigActivation,
    pub sensitive: bool,
    pub present: bool,
    /// Sensitive values are always omitted, including when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigWriteRequest {
    pub context: ManagedServiceContext,
    pub module_id: String,
    pub values: Vec<ModuleConfigWriteValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigWriteValue {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigWriteResponse {
    pub protocol: String,
    pub operation_id: String,
    pub context: ManagedServiceContext,
    pub module_id: String,
    pub target_revision_before: String,
    pub target_revision_after: String,
    pub authorization_digest: String,
    pub evidence: Vec<ModuleConfigAuditEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConfigAuditEvidence {
    pub sequence: u64,
    pub operation_id: String,
    pub module_id: String,
    pub key: String,
    pub sensitive: bool,
    pub old_value_digest: Option<String>,
    pub new_value_digest: String,
    pub recorded_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", content = "document", rename_all = "snake_case")]
pub enum ModuleOperationsMessage {
    InventoryRequest(ModuleInventoryRequest),
    Inventory(ModuleInventorySnapshot),
    ContributionsRequest(ActionContributionResolutionRequest),
    Contributions(ActionContributionResolution),
    ConfigReadRequest(ModuleConfigReadRequest),
    ConfigRead(ModuleConfigReadResponse),
    ConfigWriteRequest(ModuleConfigWriteRequest),
    ConfigWrite(ModuleConfigWriteResponse),
}

#[must_use]
pub fn module_operations_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ModuleOperationsMessage))
        .expect("Module Operations schema must serialize");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.module-operations.v1.schema.json"
            .to_owned(),
    );
    schema["title"] = Value::String("Lenso Managed Service Module Operations".to_owned());
    for definition in [
        "ModuleInventorySnapshot",
        "ActionContributionResolution",
        "ModuleConfigReadResponse",
        "ModuleConfigWriteResponse",
    ] {
        if let Some(properties) = schema["$defs"][definition]["properties"].as_object_mut() {
            properties.insert(
                "protocol".to_owned(),
                json!({ "type": "string", "const": MODULE_OPERATIONS_PROTOCOL }),
            );
        }
    }
    schema
}

#[must_use]
pub fn module_operations_schema_digest() -> String {
    digest_json(&module_operations_schema()).expect("Module Operations schema is digestible")
}
