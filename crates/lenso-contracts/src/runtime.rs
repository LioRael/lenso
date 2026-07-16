//! Pure-data runtime declarations for module manifests.
//!
//! These declarations describe runtime behavior a module can provide without
//! carrying executable handlers. Loading sources decide how to bind them later.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

pub const WORKFLOW_DEFINITION_PROTOCOL: &str = "lenso.workflow-definition.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuntimeSurface {
    #[serde(default)]
    pub functions: Vec<RuntimeFunctionDeclaration>,
    #[serde(default)]
    pub schedules: Vec<ScheduledFunctionDeclaration>,
    /// Engine-neutral Durable Workflow definitions owned by this Module.
    /// Autonomous Service composition decides how to execute them; Host-managed
    /// Provider and Runtime Function behavior does not consume this metadata.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<WorkflowDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDefinition {
    /// Versioned declaration protocol, currently `lenso.workflow-definition.v1`.
    pub protocol: String,
    /// Stable Module identity that owns the workflow behavior.
    pub owner: String,
    /// Stable workflow name within the owning Module.
    pub name: String,
    /// Definition version pinned by every started instance.
    pub version: String,
    pub input_contract: WorkflowDataContract,
    pub result_contract: WorkflowDataContract,
    /// Ordered step metadata. Position in this vector is contract-significant.
    pub steps: Vec<WorkflowStepDeclaration>,
}

impl WorkflowDefinition {
    #[must_use]
    pub fn new(
        owner: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        input_contract: WorkflowDataContract,
        result_contract: WorkflowDataContract,
        steps: Vec<WorkflowStepDeclaration>,
    ) -> Self {
        Self {
            protocol: WORKFLOW_DEFINITION_PROTOCOL.to_owned(),
            owner: owner.into(),
            name: name.into(),
            version: version.into(),
            input_contract,
            result_contract,
            steps,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDataContract {
    pub contract_id: String,
    pub version: String,
}

impl WorkflowDataContract {
    #[must_use]
    pub fn new(contract_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            contract_id: contract_id.into(),
            version: version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowStepDeclaration {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl WorkflowStepDeclaration {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
        }
    }

    #[must_use]
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }
}

/// Deterministic JSON Schema published for engine-neutral Workflow Definitions.
#[must_use]
pub fn workflow_definition_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://contracts.lenso.local/workflows/lenso.workflow-definition.v1.schema.json",
        "title": "LensoWorkflowDefinition",
        "type": "object",
        "additionalProperties": false,
        "required": [
            "protocol",
            "owner",
            "name",
            "version",
            "inputContract",
            "resultContract",
            "steps"
        ],
        "properties": {
            "protocol": { "const": WORKFLOW_DEFINITION_PROTOCOL },
            "owner": { "type": "string", "minLength": 1 },
            "name": { "type": "string", "minLength": 1 },
            "version": { "type": "string", "minLength": 1 },
            "inputContract": { "$ref": "#/$defs/dataContract" },
            "resultContract": { "$ref": "#/$defs/dataContract" },
            "steps": {
                "type": "array",
                "minItems": 1,
                "items": { "$ref": "#/$defs/step" }
            }
        },
        "$defs": {
            "dataContract": {
                "type": "object",
                "additionalProperties": false,
                "required": ["contractId", "version"],
                "properties": {
                    "contractId": { "type": "string", "minLength": 1 },
                    "version": { "type": "string", "minLength": 1 }
                }
            },
            "step": {
                "type": "object",
                "additionalProperties": false,
                "required": ["name"],
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "displayName": { "type": "string", "minLength": 1 }
                }
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuntimeFunctionDeclaration {
    /// Stable function name used by `runtime.function_runs`.
    pub name: String,
    /// Version for the declared function contract.
    pub version: u16,
    /// Queue name the host should use when registering the function.
    pub queue: String,
    /// Optional schema identifier for the function input payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<String>,
    /// Optional retry policy requested by the module. The host still owns
    /// enforcement and may clamp values when behavior registration is added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RuntimeRetryPolicyDeclaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<crate::ServiceOperationMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuntimeRetryPolicyDeclaration {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScheduledFunctionDeclaration {
    /// Stable schedule name scoped to the owning module.
    pub name: String,
    /// Runtime function enqueued when this schedule is due.
    pub function_name: String,
    /// Standard 5-field cron expression in UTC.
    pub cron: String,
    #[serde(default)]
    pub input: Value,
}
