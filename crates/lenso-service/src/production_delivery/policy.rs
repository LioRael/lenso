use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    ConfigContractDefinition, ConfigRevision, DeliveryDecision, DeliveryEffects, DeliveryIssue,
    DeliveryIssueCode, ProductionEligibilityEvidence, ProductionEligibilityInput,
    ReleaseTrustEvidence, ReleaseTrustProvider, SecretProvider, ServiceRelease,
    config_revision_matches_contract, production_eligibility_evidence_integrity_is_valid,
    release_trust_evidence_integrity_is_valid, service_release_integrity_is_valid,
};

pub const POLICY_PACK_PROTOCOL: &str = "lenso.policy-pack.v1";
pub const POLICY_EVIDENCE_PROTOCOL: &str = "lenso.policy-evidence.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEnvironmentProfile {
    Development,
    Production,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyRuleSeverity {
    Required,
    Advisory,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    pub rule_id: String,
    pub evidence_key: String,
    pub severity: PolicyRuleSeverity,
    pub advisory_in_development: bool,
    pub remediation: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyPack {
    pub protocol: String,
    pub pack_id: String,
    pub pack_digest: String,
    pub version: String,
    pub environment_profile: PolicyEnvironmentProfile,
    pub rules: Vec<PolicyRule>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEvaluationSurface {
    Local,
    Ci,
    Cli,
    SystemPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRuleResult {
    pub rule_id: String,
    pub severity: PolicyRuleSeverity,
    pub decision: DeliveryDecision,
    pub evidence_references: Vec<String>,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub pack_id: String,
    pub pack_digest: String,
    pub evaluated_subject: String,
    pub input_digests: BTreeMap<String, String>,
    pub decision: DeliveryDecision,
    pub rule_results: Vec<PolicyRuleResult>,
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPolicyInputs {
    pub release: ServiceRelease,
    pub trust: ReleaseTrustEvidence,
    pub config_contract: ConfigContractDefinition,
    pub config: ConfigRevision,
    pub eligibility: ProductionEligibilityEvidence,
    pub eligibility_input: ProductionEligibilityInput,
}

#[must_use]
pub fn production_policy_pack() -> PolicyPack {
    build_policy_pack(PolicyEnvironmentProfile::Production)
}

#[must_use]
pub fn development_policy_pack() -> PolicyPack {
    build_policy_pack(PolicyEnvironmentProfile::Development)
}

#[must_use]
pub fn evaluate_delivery_policy(
    pack: &PolicyPack,
    inputs: &DeliveryPolicyInputs,
    trust_provider: &dyn ReleaseTrustProvider,
    secret_provider: &dyn SecretProvider,
    _surface: PolicyEvaluationSurface,
) -> PolicyEvidence {
    let pack_valid = policy_pack_integrity_is_valid(pack);
    let release_valid = service_release_integrity_is_valid(&inputs.release);
    let trust_valid =
        release_trust_evidence_integrity_is_valid(&inputs.trust, &inputs.release, trust_provider);
    let config_valid =
        config_revision_matches_contract(&inputs.config, &inputs.config_contract, secret_provider)
            && inputs.config.service_id == inputs.release.service_id
            && inputs.config_contract.reference == inputs.release.config_contract.reference
            && inputs.config_contract.digest == inputs.release.config_contract.digest
            && inputs.config.contract_digest == inputs.release.config_contract.digest;
    let eligibility_valid = production_eligibility_evidence_integrity_is_valid(
        &inputs.eligibility,
        &inputs.eligibility_input,
        &inputs.release,
        trust_provider,
    );
    let facts = BTreeMap::from([
        ("release.integrity".to_owned(), Some(release_valid)),
        (
            "supply_chain.trusted".to_owned(),
            Some(trust_valid && inputs.trust.decision == DeliveryDecision::Passed),
        ),
        ("config.valid".to_owned(), Some(config_valid)),
    ])
    .into_iter()
    .chain(inputs.eligibility.facts.iter().map(|(key, value)| {
        (
            key.clone(),
            if eligibility_valid {
                *value
            } else {
                Some(false)
            },
        )
    }))
    .collect::<BTreeMap<_, _>>();

    let mut issues = Vec::new();
    if !pack_valid {
        issues.push(DeliveryIssue {
            code: DeliveryIssueCode::PolicyRuleBlocked,
            message: "The Policy Pack identity, digest, environment profile, or required rule set is invalid.".to_owned(),
            evidence_references: vec![pack.pack_id.clone()],
            remediation: "Use the canonical versioned Policy Pack for the selected environment profile.".to_owned(),
            next_actions: vec!["Reload the canonical Policy Pack and evaluate the unchanged evidence again.".to_owned()],
        });
    }
    let rule_results = pack
        .rules
        .iter()
        .map(|rule| {
            let passed = facts
                .get(&rule.evidence_key)
                .copied()
                .flatten()
                .unwrap_or(false);
            let decision = if passed {
                DeliveryDecision::Passed
            } else if pack.environment_profile == PolicyEnvironmentProfile::Development
                && rule.advisory_in_development
            {
                DeliveryDecision::Advisory
            } else {
                DeliveryDecision::Blocked
            };
            if decision == DeliveryDecision::Blocked {
                issues.push(DeliveryIssue {
                    code: DeliveryIssueCode::PolicyRuleBlocked,
                    message: format!(
                        "Policy rule `{}` is blocked because `{}` is false or unknown.",
                        rule.rule_id, rule.evidence_key
                    ),
                    evidence_references: vec![rule.evidence_key.clone()],
                    remediation: rule.remediation.clone(),
                    next_actions: vec![rule.next_action.clone()],
                });
            }
            PolicyRuleResult {
                rule_id: rule.rule_id.clone(),
                severity: rule.severity,
                decision,
                evidence_references: vec![rule.evidence_key.clone()],
                remediation: rule.remediation.clone(),
                next_actions: vec![rule.next_action.clone()],
            }
        })
        .collect::<Vec<_>>();
    let decision = if !pack_valid
        || rule_results
            .iter()
            .any(|result| result.decision == DeliveryDecision::Blocked)
    {
        DeliveryDecision::Blocked
    } else if rule_results
        .iter()
        .any(|result| result.decision == DeliveryDecision::Advisory)
    {
        DeliveryDecision::Advisory
    } else {
        DeliveryDecision::Passed
    };
    let input_digests = BTreeMap::from([
        ("release".to_owned(), inputs.release.release_digest.clone()),
        ("trust".to_owned(), inputs.trust.evidence_digest.clone()),
        ("config".to_owned(), inputs.config.revision_digest.clone()),
        (
            "eligibility".to_owned(),
            inputs.eligibility.evidence_digest.clone(),
        ),
    ]);
    #[derive(Serialize)]
    struct EvidenceContent<'a> {
        protocol: &'a str,
        pack_id: &'a str,
        pack_digest: &'a str,
        evaluated_subject: &'a str,
        input_digests: &'a BTreeMap<String, String>,
        decision: DeliveryDecision,
        rule_results: &'a [PolicyRuleResult],
        issues: &'a [DeliveryIssue],
    }
    let content = EvidenceContent {
        protocol: POLICY_EVIDENCE_PROTOCOL,
        pack_id: &pack.pack_id,
        pack_digest: &pack.pack_digest,
        evaluated_subject: &inputs.release.release_id,
        input_digests: &input_digests,
        decision,
        rule_results: &rule_results,
        issues: &issues,
    };
    let evidence_digest = extraction_input_digest(
        serde_json::to_vec(&content).expect("policy evidence must serialize"),
    );
    PolicyEvidence {
        protocol: POLICY_EVIDENCE_PROTOCOL.to_owned(),
        evidence_id: format!("policy-evidence:{evidence_digest}"),
        evidence_digest,
        pack_id: pack.pack_id.clone(),
        pack_digest: pack.pack_digest.clone(),
        evaluated_subject: inputs.release.release_id.clone(),
        input_digests,
        decision,
        rule_results,
        issues,
        effects: DeliveryEffects::default(),
    }
}

#[must_use]
pub fn policy_pack_integrity_is_valid(pack: &PolicyPack) -> bool {
    *pack == build_policy_pack(pack.environment_profile)
}

#[must_use]
pub fn policy_evidence_integrity_is_valid(evidence: &PolicyEvidence) -> bool {
    #[derive(Serialize)]
    struct EvidenceContent<'a> {
        protocol: &'a str,
        pack_id: &'a str,
        pack_digest: &'a str,
        evaluated_subject: &'a str,
        input_digests: &'a BTreeMap<String, String>,
        decision: DeliveryDecision,
        rule_results: &'a [PolicyRuleResult],
        issues: &'a [DeliveryIssue],
    }
    let content = EvidenceContent {
        protocol: evidence.protocol.as_str(),
        pack_id: evidence.pack_id.as_str(),
        pack_digest: evidence.pack_digest.as_str(),
        evaluated_subject: evidence.evaluated_subject.as_str(),
        input_digests: &evidence.input_digests,
        decision: evidence.decision,
        rule_results: &evidence.rule_results,
        issues: &evidence.issues,
    };
    evidence.protocol == POLICY_EVIDENCE_PROTOCOL
        && evidence.evidence_id == format!("policy-evidence:{}", evidence.evidence_digest)
        && extraction_input_digest(
            serde_json::to_vec(&content).expect("policy evidence must serialize"),
        ) == evidence.evidence_digest
}

#[must_use]
pub fn production_policy_evidence_is_valid(
    evidence: &PolicyEvidence,
    inputs: &DeliveryPolicyInputs,
    trust_provider: &dyn ReleaseTrustProvider,
    secret_provider: &dyn SecretProvider,
) -> bool {
    evidence
        == &evaluate_delivery_policy(
            &production_policy_pack(),
            inputs,
            trust_provider,
            secret_provider,
            PolicyEvaluationSurface::SystemPlane,
        )
}

fn build_policy_pack(environment_profile: PolicyEnvironmentProfile) -> PolicyPack {
    let required_keys = [
        "release.integrity",
        "supply_chain.trusted",
        "config.valid",
        "contracts.compatible",
        "migrations.safe",
        "workflows.compatible",
        "rollback.safe",
        "providers.compatible",
        "identity.production",
        "tenancy.mode.production",
        "tenancy.enforced",
        "call_policies.declared",
        "dependencies.ready",
        "resilience.declared",
        "reliability.complete",
        "edge.valid",
        "environment.verification_fresh",
        "production.eligible",
    ];
    let rules = required_keys
        .into_iter()
        .map(|key| PolicyRule {
            rule_id: format!("lenso.production.{}", key.replace('_', "-")),
            evidence_key: key.to_owned(),
            severity: PolicyRuleSeverity::Required,
            advisory_in_development: matches!(
                key,
                "rollback.safe"
                    | "providers.compatible"
                    | "identity.production"
                    | "tenancy.mode.production"
                    | "environment.verification_fresh"
                    | "production.eligible"
            ),
            remediation: format!("Provide passing canonical evidence for `{key}`."),
            next_action: "Refresh canonical evidence and evaluate the same Policy Pack again."
                .to_owned(),
        })
        .collect::<Vec<_>>();
    let version = "v1".to_owned();
    let pack_digest = extraction_input_digest(
        serde_json::to_vec(&(
            POLICY_PACK_PROTOCOL,
            version.as_str(),
            environment_profile,
            rules.as_slice(),
        ))
        .expect("Policy Pack must serialize"),
    );
    PolicyPack {
        protocol: POLICY_PACK_PROTOCOL.to_owned(),
        pack_id: format!("policy-pack:{pack_digest}"),
        pack_digest,
        version,
        environment_profile,
        rules,
    }
}
