//! Pure-data runtime declarations for module manifests.
//!
//! These declarations describe runtime behavior a module can provide without
//! carrying executable handlers. Loading sources decide how to bind them later.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

pub const WORKFLOW_DEFINITION_PROTOCOL: &str = "lenso.workflow-definition.v1";
pub const WORKFLOW_COMPATIBILITY_PROTOCOL: &str = "lenso.workflow-compatibility.v1";

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
    /// Optional durable retry schedule. `max_attempts` includes the original
    /// attempt, while each delay schedules the following attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<WorkflowRetryPolicyDeclaration>,
    /// Optional durable timeout applied independently to every attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl WorkflowStepDeclaration {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
            retry_policy: None,
            timeout_ms: None,
        }
    }

    #[must_use]
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    #[must_use]
    pub fn with_retry_policy(mut self, retry_policy: WorkflowRetryPolicyDeclaration) -> Self {
        self.retry_policy = Some(retry_policy);
        self
    }

    #[must_use]
    pub const fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowRetryPolicyDeclaration {
    /// Total attempts including the original execution.
    pub max_attempts: u32,
    /// Delay before attempts 2..N. The length must equal `max_attempts` - 1.
    pub delays_ms: Vec<u64>,
}

impl WorkflowRetryPolicyDeclaration {
    #[must_use]
    pub const fn new(max_attempts: u32, delays_ms: Vec<u64>) -> Self {
        Self {
            max_attempts,
            delays_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowCompatibilityCategory {
    Safe,
    NeedsAttention,
    Breaking,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDefinitionReference {
    pub owner: String,
    pub name: String,
    pub version: String,
}

impl From<&WorkflowDefinition> for WorkflowDefinitionReference {
    fn from(definition: &WorkflowDefinition) -> Self {
        Self {
            owner: definition.owner.clone(),
            name: definition.name.clone(),
            version: definition.version.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowCompatibilityReason {
    pub code: String,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowCompatibilityResult {
    pub protocol: String,
    pub category: WorkflowCompatibilityCategory,
    pub before: WorkflowDefinitionReference,
    pub after: WorkflowDefinitionReference,
    pub reasons: Vec<WorkflowCompatibilityReason>,
}

/// Compares two versions of one engine-neutral Workflow Definition.
///
/// The evaluator is deliberately conservative: changes to execution policy or
/// the ordered step graph never become `safe` merely because both declarations
/// deserialize. Reusing one version for different content is always blocked.
#[must_use]
pub fn evaluate_workflow_compatibility(
    before: &WorkflowDefinition,
    after: &WorkflowDefinition,
) -> WorkflowCompatibilityResult {
    let mut result = WorkflowCompatibilityResult {
        protocol: WORKFLOW_COMPATIBILITY_PROTOCOL.to_owned(),
        category: WorkflowCompatibilityCategory::Safe,
        before: before.into(),
        after: after.into(),
        reasons: Vec::new(),
    };
    if before.protocol != WORKFLOW_DEFINITION_PROTOCOL {
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::Blocked,
            "workflow_before_protocol_unsupported",
            "$.before.protocol",
            "The source Workflow Definition must use the supported declaration protocol.",
            "Regenerate the source definition with the supported Workflow Definition protocol.",
        );
    }
    if after.protocol != WORKFLOW_DEFINITION_PROTOCOL {
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::Blocked,
            "workflow_after_protocol_unsupported",
            "$.after.protocol",
            "The target Workflow Definition must use the supported declaration protocol.",
            "Regenerate the target definition with the supported Workflow Definition protocol.",
        );
    }
    if before.owner != after.owner || before.name != after.name {
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::Blocked,
            "workflow_identity_changed",
            "$.after",
            "Compatibility can only be evaluated between versions of one Workflow Definition.",
            "Compare definitions with the same owner and stable workflow name.",
        );
    }
    let before_version_missing = before.version.trim().is_empty();
    let after_version_missing = after.version.trim().is_empty();
    if before_version_missing {
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::Blocked,
            "workflow_before_version_missing",
            "$.before.version",
            "The source Workflow Definition must have an explicit version.",
            "Restore the source definition's immutable version identifier.",
        );
    }
    if after_version_missing {
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::Blocked,
            "workflow_after_version_missing",
            "$.after.version",
            "The target Workflow Definition must have an explicit version.",
            "Publish the target definition with a new explicit version identifier.",
        );
    }
    if before_version_missing || after_version_missing {
        finish_workflow_compatibility(&mut result);
        return result;
    }
    if before.version == after.version {
        if before == after {
            finish_workflow_compatibility(&mut result);
            return result;
        }
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::Blocked,
            "workflow_version_not_immutable",
            "$.after.version",
            "Changed Workflow Definition content must use a new explicit version.",
            "Restore the original version artifact and publish the changed definition under a new version.",
        );
        finish_workflow_compatibility(&mut result);
        return result;
    }

    compare_workflow_data_contract(
        &mut result,
        "inputContract",
        &before.input_contract,
        &after.input_contract,
    );
    compare_workflow_data_contract(
        &mut result,
        "resultContract",
        &before.result_contract,
        &after.result_contract,
    );
    for (old_index, old_step) in before.steps.iter().enumerate() {
        let Some(new_index) = after
            .steps
            .iter()
            .position(|candidate| candidate.name == old_step.name)
        else {
            add_workflow_compatibility_reason(
                &mut result,
                WorkflowCompatibilityCategory::Breaking,
                "workflow_step_removed",
                &format!("$.before.steps[{old_index}]"),
                "An existing ordered Workflow step was removed.",
                "Preserve the existing step or provide an explicit in-flight state mapping.",
            );
            continue;
        };
        let new_step = &after.steps[new_index];
        if old_index != new_index {
            add_workflow_compatibility_reason(
                &mut result,
                WorkflowCompatibilityCategory::Breaking,
                "workflow_step_moved",
                &format!("$.after.steps[{new_index}].name"),
                "An existing ordered Workflow step moved to a different position.",
                "Preserve step order or provide an explicit in-flight state mapping.",
            );
        }
        if old_step.retry_policy != new_step.retry_policy {
            add_workflow_compatibility_reason(
                &mut result,
                WorkflowCompatibilityCategory::NeedsAttention,
                "workflow_retry_policy_changed",
                &format!("$.after.steps[{new_index}].retryPolicy"),
                "The retry schedule for an existing Workflow step changed.",
                "Review retry and exhaustion effects for new instances and keep in-flight instances pinned.",
            );
        }
        if old_step.timeout_ms != new_step.timeout_ms {
            add_workflow_compatibility_reason(
                &mut result,
                WorkflowCompatibilityCategory::NeedsAttention,
                "workflow_timeout_changed",
                &format!("$.after.steps[{new_index}].timeoutMs"),
                "The timeout for an existing Workflow step changed.",
                "Review timer effects for new instances and keep in-flight timers pinned.",
            );
        }
        if old_step.display_name != new_step.display_name {
            add_workflow_compatibility_reason(
                &mut result,
                WorkflowCompatibilityCategory::Safe,
                "workflow_display_name_changed",
                &format!("$.after.steps[{new_index}].displayName"),
                "Only operator-facing Workflow step display metadata changed.",
                "Regenerate the version artifact and retain the old artifact for in-flight instances.",
            );
        }
    }
    for (new_index, new_step) in after.steps.iter().enumerate() {
        if before
            .steps
            .iter()
            .any(|candidate| candidate.name == new_step.name)
        {
            continue;
        }
        add_workflow_compatibility_reason(
            &mut result,
            WorkflowCompatibilityCategory::NeedsAttention,
            "workflow_step_added",
            &format!("$.after.steps[{new_index}]"),
            "A new ordered Workflow step was added.",
            "Review the new business effect and start it only through the new definition version.",
        );
    }
    finish_workflow_compatibility(&mut result);
    result
}

fn compare_workflow_data_contract(
    result: &mut WorkflowCompatibilityResult,
    field: &str,
    before: &WorkflowDataContract,
    after: &WorkflowDataContract,
) {
    if before.contract_id != after.contract_id {
        add_workflow_compatibility_reason(
            result,
            WorkflowCompatibilityCategory::Breaking,
            "workflow_data_contract_identity_changed",
            &format!("$.after.{field}.contractId"),
            "The stable Workflow data contract identity changed.",
            "Preserve the contract identity or coordinate an explicit state and payload migration.",
        );
    } else if before.version != after.version {
        add_workflow_compatibility_reason(
            result,
            WorkflowCompatibilityCategory::NeedsAttention,
            "workflow_data_contract_version_changed",
            &format!("$.after.{field}.version"),
            "A versioned Workflow data contract changed.",
            "Review payload compatibility and retain evidence for the selected contract version.",
        );
    }
}

fn add_workflow_compatibility_reason(
    result: &mut WorkflowCompatibilityResult,
    category: WorkflowCompatibilityCategory,
    code: &str,
    path: &str,
    message: &str,
    next_action: &str,
) {
    result.category = result.category.max(category);
    result.reasons.push(WorkflowCompatibilityReason {
        code: code.to_owned(),
        path: path.to_owned(),
        message: message.to_owned(),
        next_action: next_action.to_owned(),
    });
}

fn finish_workflow_compatibility(result: &mut WorkflowCompatibilityResult) {
    if result.reasons.is_empty() {
        add_workflow_compatibility_reason(
            result,
            WorkflowCompatibilityCategory::Safe,
            "workflow_definition_compatible",
            "$.after.version",
            "The new version preserves the existing Workflow execution contract.",
            "Publish the new immutable version and select it explicitly for new instances.",
        );
    }
    result.reasons.sort();
    result.reasons.dedup();
}

/// Stable generated examples for every public Workflow compatibility category.
#[must_use]
pub fn workflow_compatibility_artifact() -> Value {
    let before = WorkflowDefinition::new(
        "support-sla",
        "ticket_sla",
        "v1",
        WorkflowDataContract::new("support.sla.start", "v1"),
        WorkflowDataContract::new("support.sla.result", "v1"),
        vec![
            WorkflowStepDeclaration::new("acknowledge_ticket"),
            WorkflowStepDeclaration::new("await_resolution"),
        ],
    );
    let mut safe = before.clone();
    safe.version = "v2".to_owned();
    let mut needs_attention = safe.clone();
    needs_attention.steps[0].timeout_ms = Some(5_000);
    let mut breaking = safe.clone();
    breaking.steps.remove(0);
    let mut blocked = before.clone();
    blocked.steps[0].timeout_ms = Some(5_000);
    json!({
        "protocol": WORKFLOW_COMPATIBILITY_PROTOCOL,
        "cases": [
            {"name": "safe", "result": evaluate_workflow_compatibility(&before, &safe)},
            {"name": "needs_attention", "result": evaluate_workflow_compatibility(&before, &needs_attention)},
            {"name": "breaking", "result": evaluate_workflow_compatibility(&before, &breaking)},
            {"name": "blocked", "result": evaluate_workflow_compatibility(&before, &blocked)}
        ]
    })
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
                    "displayName": { "type": "string", "minLength": 1 },
                    "retryPolicy": { "$ref": "#/$defs/retryPolicy" },
                    "timeoutMs": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 9223372036854775807_i64
                    }
                }
            },
            "retryPolicy": {
                "type": "object",
                "additionalProperties": false,
                "required": ["maxAttempts", "delaysMs"],
                "properties": {
                    "maxAttempts": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 2147483647
                    },
                    "delaysMs": {
                        "type": "array",
                        "items": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 9223372036854775807_i64
                        }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(version: &str) -> WorkflowDefinition {
        WorkflowDefinition::new(
            "support-sla",
            "ticket_sla",
            version,
            WorkflowDataContract::new("support.sla.start", "v1"),
            WorkflowDataContract::new("support.sla.result", "v1"),
            vec![
                WorkflowStepDeclaration::new("acknowledge_ticket"),
                WorkflowStepDeclaration::new("await_resolution"),
            ],
        )
    }

    #[test]
    fn workflow_compatibility_categories_are_deterministic_and_actionable() {
        let before = definition("v1");
        let safe = definition("v2");
        let mut needs_attention = safe.clone();
        needs_attention.steps[0].timeout_ms = Some(5_000);
        assert_eq!(
            serde_json::to_value(evaluate_workflow_compatibility(&before, &needs_attention))
                .unwrap()["category"],
            "needs-attention"
        );
        let mut breaking = safe.clone();
        breaking.steps.remove(0);
        let mut blocked = before.clone();
        blocked.steps[0].timeout_ms = Some(5_000);

        for (expected, after) in [
            (WorkflowCompatibilityCategory::Safe, safe),
            (
                WorkflowCompatibilityCategory::NeedsAttention,
                needs_attention,
            ),
            (WorkflowCompatibilityCategory::Breaking, breaking),
            (WorkflowCompatibilityCategory::Blocked, blocked),
        ] {
            let first = evaluate_workflow_compatibility(&before, &after);
            let second = evaluate_workflow_compatibility(&before, &after);
            assert_eq!(first, second);
            assert_eq!(first.category, expected);
            assert!(first.reasons.iter().all(|reason| {
                !reason.code.is_empty()
                    && reason.path.starts_with('$')
                    && !reason.next_action.is_empty()
            }));
        }
    }

    #[test]
    fn workflow_compatibility_paths_identify_real_source_and_target_steps() {
        let before = definition("v1");
        let mut removed = definition("v2");
        removed.steps.pop();
        let removed_result = evaluate_workflow_compatibility(&before, &removed);
        assert!(removed_result.reasons.iter().any(|reason| {
            reason.code == "workflow_step_removed" && reason.path == "$.before.steps[1]"
        }));

        let mut inserted = definition("v2");
        inserted
            .steps
            .insert(0, WorkflowStepDeclaration::new("triage_ticket"));
        let inserted_result = evaluate_workflow_compatibility(&before, &inserted);
        assert!(inserted_result.reasons.iter().any(|reason| {
            reason.code == "workflow_step_added" && reason.path == "$.after.steps[0]"
        }));
    }

    #[test]
    fn workflow_compatibility_invalid_source_paths_point_to_the_source() {
        let mut before = definition("v1");
        let after = definition("v2");
        before.protocol = "unsupported.workflow-definition".to_owned();
        let unsupported = evaluate_workflow_compatibility(&before, &after);
        assert!(unsupported.reasons.iter().any(|reason| {
            reason.code == "workflow_before_protocol_unsupported"
                && reason.path == "$.before.protocol"
        }));
        assert!(
            !unsupported
                .reasons
                .iter()
                .any(|reason| reason.code == "workflow_after_protocol_unsupported")
        );

        before.protocol = WORKFLOW_DEFINITION_PROTOCOL.to_owned();
        before.version.clear();
        let missing = evaluate_workflow_compatibility(&before, &after);
        assert!(missing.reasons.iter().any(|reason| {
            reason.code == "workflow_before_version_missing" && reason.path == "$.before.version"
        }));
        assert!(
            !missing
                .reasons
                .iter()
                .any(|reason| reason.code == "workflow_after_version_missing")
        );
    }
}
