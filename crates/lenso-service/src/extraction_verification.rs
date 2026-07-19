use crate::{
    ExtractionReconciliationResult, ExtractionReconciliationStatus, extraction_input_digest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXTRACTION_VERIFICATION_PROTOCOL: &str = "lenso.extraction-verification.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBehaviorObservation {
    pub implementation: String,
    pub module_id: String,
    pub operation_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub response: Value,
    pub durable_state: Value,
    #[serde(default)]
    pub event_effects: Vec<String>,
    #[serde(default)]
    pub workflow_outcomes: Vec<String>,
    #[serde(default)]
    pub story_evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionCompatibilityEvidence {
    pub consumer_id: String,
    pub contract_id: String,
    pub pinned_version: String,
    pub compatible: bool,
    pub detail: String,
}

impl ExtractionCompatibilityEvidence {
    #[must_use]
    pub fn compatible(
        consumer_id: impl Into<String>,
        contract_id: impl Into<String>,
        pinned_version: impl Into<String>,
    ) -> Self {
        Self {
            consumer_id: consumer_id.into(),
            contract_id: contract_id.into(),
            pinned_version: pinned_version.into(),
            compatible: true,
            detail: "Consumer is compatible with the pinned Contract Version.".to_owned(),
        }
    }

    #[must_use]
    pub fn incompatible(
        consumer_id: impl Into<String>,
        contract_id: impl Into<String>,
        pinned_version: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            consumer_id: consumer_id.into(),
            contract_id: contract_id.into(),
            pinned_version: pinned_version.into(),
            compatible: false,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPolicyEvidence {
    pub rule_id: String,
    pub passed: bool,
    pub inputs_digest: String,
    pub detail: String,
}

impl ExtractionPolicyEvidence {
    #[must_use]
    pub fn passed(rule_id: impl Into<String>) -> Self {
        let rule_id = rule_id.into();
        Self {
            inputs_digest: extraction_input_digest(rule_id.as_bytes()),
            rule_id,
            passed: true,
            detail: "Built-in extraction safety rule passed.".to_owned(),
        }
    }

    #[must_use]
    pub fn failed(rule_id: impl Into<String>, detail: impl Into<String>) -> Self {
        let rule_id = rule_id.into();
        Self {
            inputs_digest: extraction_input_digest(rule_id.as_bytes()),
            rule_id,
            passed: false,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionVerificationInputs {
    pub reconciliation: ExtractionReconciliationResult,
    pub linked: ExtractionBehaviorObservation,
    pub candidate: ExtractionBehaviorObservation,
    #[serde(default)]
    pub compatibility: Vec<ExtractionCompatibilityEvidence>,
    #[serde(default)]
    pub policy: Vec<ExtractionPolicyEvidence>,
    #[serde(default)]
    pub volatile_json_pointers: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionVerificationStatus {
    Verified,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionVerificationIssueCode {
    ReconciliationNotMatched,
    BehaviorMismatch,
    DurableStateMismatch,
    EventEffectMismatch,
    WorkflowOutcomeMismatch,
    ContextMismatch,
    StoryMismatch,
    ConsumerIncompatible,
    PolicyRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionVerificationIssue {
    pub code: ExtractionVerificationIssueCode,
    pub subject: String,
    pub detail: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionVerificationEvidence {
    pub kind: String,
    pub subject: String,
    pub digest: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionVerificationEffects {
    pub invokes_linked_public_contract: bool,
    pub invokes_candidate_public_contract: bool,
    pub routes_external_mutations: bool,
    pub changes_authority: bool,
    pub requires_runtime_console: bool,
    pub requires_system_plane_for_business_execution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionVerificationResult {
    pub protocol: String,
    pub verification_id: String,
    pub verification_digest: String,
    pub status: ExtractionVerificationStatus,
    pub plan_id: String,
    pub reconciliation_id: String,
    pub issues: Vec<ExtractionVerificationIssue>,
    pub evidence: Vec<ExtractionVerificationEvidence>,
    pub compatibility: Vec<ExtractionCompatibilityEvidence>,
    pub policy: Vec<ExtractionPolicyEvidence>,
    pub volatile_json_pointers: Vec<String>,
    pub provisional_cutover_eligible: bool,
    pub linked_authority_remains_authoritative: bool,
    pub effects: ExtractionVerificationEffects,
}

#[must_use]
pub fn verify_extraction_behavior(
    mut inputs: ExtractionVerificationInputs,
) -> ExtractionVerificationResult {
    inputs.compatibility.sort_by(|a, b| {
        (&a.consumer_id, &a.contract_id, &a.pinned_version).cmp(&(
            &b.consumer_id,
            &b.contract_id,
            &b.pinned_version,
        ))
    });
    inputs.policy.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
    inputs.volatile_json_pointers.sort();
    inputs.volatile_json_pointers.dedup();
    let mut issues = Vec::new();
    let mut evidence = Vec::new();
    if inputs.reconciliation.status != ExtractionReconciliationStatus::Matched {
        issue(
            &mut issues,
            ExtractionVerificationIssueCode::ReconciliationNotMatched,
            "reconciliation",
            "Data reconciliation is not matched.",
            "Remediate reconciliation blockers before behavior verification.",
        );
    }
    if inputs.linked.module_id != inputs.candidate.module_id
        || inputs.linked.operation_id != inputs.candidate.operation_id
    {
        issue(
            &mut issues,
            ExtractionVerificationIssueCode::BehaviorMismatch,
            "business-identity",
            "Module or operation identity changed between implementations.",
            "Preserve the declared Module and operation identities.",
        );
    }
    if inputs.linked.tenant_id != inputs.candidate.tenant_id
        || inputs.linked.actor_id != inputs.candidate.actor_id
    {
        issue(
            &mut issues,
            ExtractionVerificationIssueCode::ContextMismatch,
            "actor-tenant-context",
            "Actor or tenant scope changed between implementations.",
            "Preserve verified actor and tenant context at the Service boundary.",
        );
    }
    let linked_response = normalize(
        inputs.linked.response.clone(),
        &inputs.volatile_json_pointers,
    );
    let candidate_response = normalize(
        inputs.candidate.response.clone(),
        &inputs.volatile_json_pointers,
    );
    compare_value(
        &mut issues,
        ExtractionVerificationIssueCode::BehaviorMismatch,
        "response",
        &linked_response,
        &candidate_response,
    );
    let linked_state = normalize(
        inputs.linked.durable_state.clone(),
        &inputs.volatile_json_pointers,
    );
    let candidate_state = normalize(
        inputs.candidate.durable_state.clone(),
        &inputs.volatile_json_pointers,
    );
    compare_value(
        &mut issues,
        ExtractionVerificationIssueCode::DurableStateMismatch,
        "durable-state",
        &linked_state,
        &candidate_state,
    );
    compare_list(
        &mut issues,
        ExtractionVerificationIssueCode::EventEffectMismatch,
        "event-effects",
        &inputs.linked.event_effects,
        &inputs.candidate.event_effects,
    );
    compare_list(
        &mut issues,
        ExtractionVerificationIssueCode::WorkflowOutcomeMismatch,
        "workflow-outcomes",
        &inputs.linked.workflow_outcomes,
        &inputs.candidate.workflow_outcomes,
    );
    compare_list(
        &mut issues,
        ExtractionVerificationIssueCode::StoryMismatch,
        "runtime-stories",
        &inputs.linked.story_evidence,
        &inputs.candidate.story_evidence,
    );
    for compatibility in &inputs.compatibility {
        evidence.push(evidence_for(
            "consumer_compatibility",
            &compatibility.consumer_id,
            compatibility,
            &compatibility.detail,
        ));
        if !compatibility.compatible {
            issue(
                &mut issues,
                ExtractionVerificationIssueCode::ConsumerIncompatible,
                &compatibility.consumer_id,
                &compatibility.detail,
                "Restore compatibility with the pinned active Consumer Contract Version.",
            );
        }
    }
    for policy in &inputs.policy {
        evidence.push(evidence_for(
            "policy",
            &policy.rule_id,
            policy,
            &policy.detail,
        ));
        if !policy.passed {
            issue(
                &mut issues,
                ExtractionVerificationIssueCode::PolicyRejected,
                &policy.rule_id,
                &policy.detail,
                "Apply the rule-specific remediation and rerun verification.",
            );
        }
    }
    evidence.push(evidence_for(
        "behavior_comparison",
        &inputs.linked.operation_id,
        &(
            linked_response,
            candidate_response,
            linked_state,
            candidate_state,
            &inputs.linked.event_effects,
            &inputs.candidate.event_effects,
            &inputs.linked.workflow_outcomes,
            &inputs.candidate.workflow_outcomes,
            &inputs.linked.story_evidence,
            &inputs.candidate.story_evidence,
        ),
        "Linked and candidate observations were compared through public contracts.",
    ));
    issues.sort();
    evidence.sort();
    let status = if issues.is_empty() {
        ExtractionVerificationStatus::Verified
    } else {
        ExtractionVerificationStatus::Blocked
    };
    let identity_digest = digest(&(
        inputs.reconciliation.reconciliation_id.as_str(),
        inputs.linked.operation_id.as_str(),
        inputs.linked.tenant_id.as_str(),
        inputs.linked.actor_id.as_str(),
    ));
    let mut result = ExtractionVerificationResult {
        protocol: EXTRACTION_VERIFICATION_PROTOCOL.to_owned(),
        verification_id: format!("extraction-verification:{identity_digest}"),
        verification_digest: String::new(),
        status,
        plan_id: inputs.reconciliation.plan_id,
        reconciliation_id: inputs.reconciliation.reconciliation_id,
        issues,
        evidence,
        compatibility: inputs.compatibility,
        policy: inputs.policy,
        volatile_json_pointers: inputs.volatile_json_pointers,
        provisional_cutover_eligible: status == ExtractionVerificationStatus::Verified,
        linked_authority_remains_authoritative: true,
        effects: ExtractionVerificationEffects {
            invokes_linked_public_contract: true,
            invokes_candidate_public_contract: true,
            ..ExtractionVerificationEffects::default()
        },
    };
    result.verification_digest = digest(&without_digest(&result));
    result
}

fn compare_value(
    issues: &mut Vec<ExtractionVerificationIssue>,
    code: ExtractionVerificationIssueCode,
    subject: &str,
    linked: &Value,
    candidate: &Value,
) {
    if linked != candidate {
        issue(
            issues,
            code,
            subject,
            format!("Linked and candidate {subject} differ."),
            format!("Inspect the declared {subject} difference and rerun verification."),
        );
    }
}

fn compare_list(
    issues: &mut Vec<ExtractionVerificationIssue>,
    code: ExtractionVerificationIssueCode,
    subject: &str,
    linked: &[String],
    candidate: &[String],
) {
    if linked != candidate {
        issue(
            issues,
            code,
            subject,
            format!("Linked and candidate {subject} differ."),
            format!("Preserve declared {subject} identities and business effects."),
        );
    }
}

fn normalize(mut value: Value, pointers: &[String]) -> Value {
    for pointer in pointers {
        if let Some((parent, key)) = pointer.rsplit_once('/') {
            if let Some(Value::Object(object)) = value.pointer_mut(parent) {
                object.remove(key);
            }
        }
    }
    value
}

fn issue(
    issues: &mut Vec<ExtractionVerificationIssue>,
    code: ExtractionVerificationIssueCode,
    subject: impl Into<String>,
    detail: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(ExtractionVerificationIssue {
        code,
        subject: subject.into(),
        detail: detail.into(),
        next_actions: vec![next_action.into()],
    });
}

fn evidence_for(
    kind: &str,
    subject: &str,
    value: &impl Serialize,
    detail: &str,
) -> ExtractionVerificationEvidence {
    ExtractionVerificationEvidence {
        kind: kind.to_owned(),
        subject: subject.to_owned(),
        digest: digest(value),
        detail: detail.to_owned(),
    }
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(
        &serde_json::to_vec(value).expect("Extraction verification values must serialize"),
    )
}

fn without_digest(result: &ExtractionVerificationResult) -> ExtractionVerificationResult {
    let mut value = result.clone();
    value.verification_digest.clear();
    value
}
