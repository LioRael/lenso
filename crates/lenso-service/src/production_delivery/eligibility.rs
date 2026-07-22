use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    DeliveryDecision, DeliveryEffects, DeliveryIssue, DeliveryIssueCode, ReleaseSignerStatus,
    ReleaseTrustProvider, ServiceRelease, issue, service_release_integrity_is_valid,
};

pub const PRODUCTION_ELIGIBILITY_PROTOCOL: &str = "lenso.production-eligibility.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractCompatibilityInput {
    pub contract_id: String,
    pub current_major: u32,
    pub candidate_major: u32,
    pub compatible: Option<bool>,
    #[serde(default)]
    pub active_consumers: Vec<String>,
    pub consumer_migration_evidence: bool,
    pub retiring: bool,
    pub deprecation_window_complete: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MigrationPhase {
    Expand,
    Backfill,
    Verify,
    Contract,
    Irreversible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCompatibilityInput {
    pub migration_id: String,
    pub lineage_id: String,
    pub sequence: u32,
    pub phase: MigrationPhase,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompatibilityInput {
    pub new_starts_compatible: Option<bool>,
    pub in_flight_compatible: Option<bool>,
    pub downgrade_safe: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RollbackCompatibilityInput {
    pub prior_release_compatible: Option<bool>,
    pub schema_compatible: Option<bool>,
    pub workflow_compatible: Option<bool>,
    pub config_compatible: Option<bool>,
    pub secret_references_compatible: Option<bool>,
    pub edge_compatible: Option<bool>,
    pub adapter_capable: Option<bool>,
    pub previous_release_id: String,
    pub previous_release_digest: String,
    pub previous_deployment_plan_id: String,
    pub previous_deployment_plan_digest: String,
    pub previous_config_revision_id: String,
    pub previous_config_revision_digest: String,
    #[serde(default)]
    pub previous_secret_reference_ids: Vec<String>,
    pub previous_gateway_plan_id: String,
    pub previous_gateway_plan_digest: String,
    pub previous_gateway_configuration_identity: String,
    pub previous_adapter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductionEligibilityInput {
    pub release_id: String,
    pub release_digest: String,
    pub provider_id: String,
    pub provider_proof: String,
    pub system_graph_digest: String,
    #[serde(default)]
    pub contracts: Vec<ContractCompatibilityInput>,
    #[serde(default)]
    pub migrations: Vec<MigrationCompatibilityInput>,
    pub workflows: WorkflowCompatibilityInput,
    pub rollback: RollbackCompatibilityInput,
    pub provider_compatibility_verified: Option<bool>,
    pub workload_identity_production: Option<bool>,
    pub tenancy_mode_production: Option<bool>,
    pub tenant_context_enforced: Option<bool>,
    pub call_policies_declared: Option<bool>,
    pub dependencies_ready: Option<bool>,
    pub resilience_declared: Option<bool>,
    pub reliability_contract_complete: Option<bool>,
    pub edge_contract_valid: Option<bool>,
    pub environment_verification_fresh: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetirementEvidence {
    pub contract_id: String,
    pub ready: bool,
    pub active_consumers: Vec<String>,
    pub deprecation_window_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductionEligibilityEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub release_id: String,
    pub release_digest: String,
    pub provider_id: String,
    pub input_digest: String,
    pub system_graph_digest: String,
    pub decision: DeliveryDecision,
    pub facts: BTreeMap<String, Option<bool>>,
    pub contract_retirement: Vec<ContractRetirementEvidence>,
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

#[must_use]
pub fn evaluate_production_eligibility(
    input: &ProductionEligibilityInput,
    release: &ServiceRelease,
    provider: &dyn ReleaseTrustProvider,
) -> ProductionEligibilityEvidence {
    let mut issues = Vec::new();
    let mut retirement = Vec::new();
    let input_digest = production_eligibility_input_digest(input);
    let authority_valid = service_release_integrity_is_valid(release)
        && input.release_id == release.release_id
        && input.release_digest == release.release_digest
        && !input.system_graph_digest.trim().is_empty()
        && provider.verify(
            input.provider_id.as_str(),
            input_digest.as_str(),
            input.provider_proof.as_str(),
        ) == ReleaseSignerStatus::Trusted;
    if !authority_valid {
        issues.push(issue(
            DeliveryIssueCode::PolicyEvidenceMissing,
            "Production Eligibility is not attested for the exact Service Release and System graph.",
            "Collect release-bound compatibility facts through a trusted eligibility provider.",
            "Refresh and attest the exact eligibility input before policy evaluation.",
        ));
    }
    let release_contracts = release
        .contract_versions
        .iter()
        .map(|contract| contract.contract_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let input_contracts = input
        .contracts
        .iter()
        .map(|contract| contract.contract_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let contracts_bind_release = release_contracts.len() == release.contract_versions.len()
        && input_contracts.len() == input.contracts.len()
        && release_contracts == input_contracts
        && input.contracts.iter().all(|candidate| {
            release.contract_versions.iter().any(|declared| {
                declared.contract_id == candidate.contract_id
                    && major_version(&declared.version) == Some(candidate.candidate_major)
            })
        });
    if !contracts_bind_release {
        issues.push(issue(
            DeliveryIssueCode::ContractIncompatible,
            "Eligibility Contract evidence does not cover the exact candidate Contract Versions.",
            "Bind every candidate Contract identity and major version from the Service Release.",
            "Refresh Contract compatibility evidence for the exact release.",
        ));
    }
    let release_migrations = release
        .migrations
        .iter()
        .map(|migration| migration.migration_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let input_migrations = input
        .migrations
        .iter()
        .map(|migration| migration.migration_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let migrations_bind_release = release_migrations.len() == release.migrations.len()
        && input_migrations.len() == input.migrations.len()
        && release_migrations == input_migrations
        && input.migrations.iter().all(|candidate| {
            release.migrations.iter().any(|declared| {
                declared.migration_id == candidate.migration_id
                    && migration_phase(&declared.phase) == Some(candidate.phase)
                    && (declared.reversible || candidate.phase != MigrationPhase::Irreversible)
            })
        });
    if !migrations_bind_release {
        issues.push(issue(
            DeliveryIssueCode::MigrationUnsafe,
            "Eligibility Migration evidence does not cover the exact candidate migration set.",
            "Bind every migration identity, phase, and reversibility boundary from the Service Release.",
            "Refresh Migration compatibility evidence for the exact release.",
        ));
    }
    let contracts_safe = contracts_bind_release && input.contracts.iter().all(|contract| {
        let compatible = match contract.compatible {
            Some(true) => true,
            Some(false) => {
                contract.candidate_major > contract.current_major
                    && contract.consumer_migration_evidence
            }
            None => false,
        };
        if !compatible {
            issues.push(DeliveryIssue {
                code: DeliveryIssueCode::ContractIncompatible,
                message: format!(
                    "Contract `{}` is incompatible or has unknown compatibility evidence.",
                    contract.contract_id
                ),
                evidence_references: vec![format!("contract:{}", contract.contract_id)],
                remediation: "Keep compatible additions on the current major version or publish a parallel major with explicit Consumer migration evidence.".to_owned(),
                next_actions: vec!["Correct the Contract Version and rerun can-I-deploy.".to_owned()],
            });
        }
        if contract.retiring {
            let ready = contract.active_consumers.is_empty()
                && contract.deprecation_window_complete;
            retirement.push(ContractRetirementEvidence {
                contract_id: contract.contract_id.clone(),
                ready,
                active_consumers: contract.active_consumers.clone(),
                deprecation_window_complete: contract.deprecation_window_complete,
            });
            if !ready {
                issues.push(DeliveryIssue {
                    code: DeliveryIssueCode::ContractIncompatible,
                    message: format!(
                        "Contract `{}` cannot retire while Consumers or its deprecation window remain active.",
                        contract.contract_id
                    ),
                    evidence_references: contract.active_consumers.clone(),
                    remediation: "Migrate every active Consumer and satisfy the declared deprecation window.".to_owned(),
                    next_actions: vec!["Report retirement readiness without retiring the Contract during Promotion.".to_owned()],
                });
            }
        }
        compatible
    });

    let mut migrations_safe = migrations_bind_release;
    let mut irreversible = false;
    let mut lineages = BTreeMap::<&str, Vec<&MigrationCompatibilityInput>>::new();
    for migration in &input.migrations {
        lineages
            .entry(migration.lineage_id.as_str())
            .or_default()
            .push(migration);
    }
    for (lineage_id, mut migrations) in lineages {
        migrations.sort_by_key(|migration| {
            (migration.sequence, migration.phase, &migration.migration_id)
        });
        let lineage_identity_valid = !lineage_id.trim().is_empty()
            && migrations.iter().all(|migration| {
                !migration.migration_id.trim().is_empty() && migration.sequence > 0
            })
            && migrations
                .windows(2)
                .all(|pair| pair[0].sequence != pair[1].sequence);
        if !lineage_identity_valid {
            migrations_safe = false;
            issues.push(issue(
                DeliveryIssueCode::MigrationUnsafe,
                format!(
                    "Migration lineage `{lineage_id}` has missing or duplicate sequence identity."
                ),
                "Declare one non-zero sequence position for every migration step in the lineage.",
                "Correct the migration lineage and rerun can-I-deploy.",
            ));
        }
        for migration in &migrations {
            if !migration.verified {
                migrations_safe = false;
                issues.push(issue(
                    DeliveryIssueCode::MigrationUnsafe,
                    format!("Migration `{}` is not verified.", migration.migration_id),
                    "Provide verified migration evidence before production eligibility.",
                    "Verify the migration step and rerun can-I-deploy.",
                ));
            }
            if migration.phase == MigrationPhase::Irreversible {
                irreversible = true;
            }
            if migration.phase != MigrationPhase::Contract {
                continue;
            }
            let expand_verified = migrations.iter().any(|candidate| {
                candidate.phase == MigrationPhase::Expand
                    && candidate.verified
                    && candidate.sequence < migration.sequence
            });
            let verify_verified = migrations.iter().any(|candidate| {
                candidate.phase == MigrationPhase::Verify
                    && candidate.verified
                    && candidate.sequence < migration.sequence
            });
            if !migration.verified || !expand_verified || !verify_verified {
                migrations_safe = false;
                issues.push(issue(
                    DeliveryIssueCode::MigrationUnsafe,
                    format!(
                        "Contract migration `{}` lacks verified expand-before-contract evidence in lineage `{lineage_id}`.",
                        migration.migration_id
                    ),
                    "Complete verified expand, backfill where needed, and verification in the same lineage before contract.",
                    "Correct the migration sequence and rerun can-I-deploy.",
                ));
            }
        }
    }

    let workflows_safe = option_true(input.workflows.new_starts_compatible)
        && option_true(input.workflows.in_flight_compatible)
        && option_true(input.workflows.downgrade_safe);
    if !workflows_safe {
        issues.push(issue(
            DeliveryIssueCode::WorkflowIncompatible,
            "Durable Workflow compatibility for new or in-flight instances is unsafe or unknown.",
            "Provide version-pinned compatibility and downgrade evidence for every active Workflow.",
            "Correct Workflow compatibility before production Promotion.",
        ));
    }

    let previous_secret_reference_ids = input
        .rollback
        .previous_secret_reference_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let rollback_target_identified = input
        .rollback
        .previous_release_id
        .starts_with("service-release:")
        && !input.rollback.previous_release_digest.trim().is_empty()
        && input.rollback.previous_release_id != release.release_id
        && input.rollback.previous_release_digest != release.release_digest
        && input
            .rollback
            .previous_deployment_plan_id
            .starts_with("deployment-plan:")
        && !input
            .rollback
            .previous_deployment_plan_digest
            .trim()
            .is_empty()
        && input
            .rollback
            .previous_config_revision_id
            .starts_with("config-revision:")
        && !input
            .rollback
            .previous_config_revision_digest
            .trim()
            .is_empty()
        && !input.rollback.previous_secret_reference_ids.is_empty()
        && previous_secret_reference_ids.len()
            == input.rollback.previous_secret_reference_ids.len()
        && input
            .rollback
            .previous_gateway_plan_id
            .starts_with("gateway-plan:")
        && !input
            .rollback
            .previous_gateway_plan_digest
            .trim()
            .is_empty()
        && !input
            .rollback
            .previous_gateway_configuration_identity
            .trim()
            .is_empty()
        && !input.rollback.previous_adapter.trim().is_empty();
    let rollback_safe = !irreversible
        && rollback_target_identified
        && (!release.rollback.previous_release_required
            || option_true(input.rollback.prior_release_compatible))
        && (!release.rollback.automatic_allowed
            || !release
                .migrations
                .iter()
                .any(|migration| !migration.reversible))
        && option_true(input.rollback.prior_release_compatible)
        && option_true(input.rollback.schema_compatible)
        && option_true(input.rollback.workflow_compatible)
        && option_true(input.rollback.config_compatible)
        && option_true(input.rollback.secret_references_compatible)
        && option_true(input.rollback.edge_compatible)
        && option_true(input.rollback.adapter_capable);
    if !rollback_safe {
        issues.push(issue(
            DeliveryIssueCode::RollbackUnsafe,
            "Automatic rollback is unsafe or lacks required prior release, schema, Workflow, configuration, Secret Reference, edge, or adapter evidence.",
            "Declare an honest rollback boundary and remove irreversible or destructive automatic recovery claims.",
            "Provide a safe rollback target or require explicit intervention.",
        ));
    }

    let mut facts = BTreeMap::from([
        ("contracts.compatible".to_owned(), Some(contracts_safe)),
        ("migrations.safe".to_owned(), Some(migrations_safe)),
        ("workflows.compatible".to_owned(), Some(workflows_safe)),
        ("rollback.safe".to_owned(), Some(rollback_safe)),
        (
            "providers.compatible".to_owned(),
            input.provider_compatibility_verified,
        ),
        (
            "identity.production".to_owned(),
            input.workload_identity_production,
        ),
        (
            "tenancy.mode.production".to_owned(),
            input.tenancy_mode_production,
        ),
        ("tenancy.enforced".to_owned(), input.tenant_context_enforced),
        (
            "call_policies.declared".to_owned(),
            input.call_policies_declared,
        ),
        ("dependencies.ready".to_owned(), input.dependencies_ready),
        ("resilience.declared".to_owned(), input.resilience_declared),
        (
            "reliability.complete".to_owned(),
            input.reliability_contract_complete,
        ),
        ("edge.valid".to_owned(), input.edge_contract_valid),
        (
            "environment.verification_fresh".to_owned(),
            input.environment_verification_fresh,
        ),
    ]);
    for (key, code, subject) in [
        (
            "providers.compatible",
            DeliveryIssueCode::PolicyRuleBlocked,
            "Provider compatibility evidence",
        ),
        (
            "identity.production",
            DeliveryIssueCode::PolicyRuleBlocked,
            "production Workload Identity",
        ),
        (
            "tenancy.mode.production",
            DeliveryIssueCode::PolicyRuleBlocked,
            "production Tenancy Mode",
        ),
        (
            "tenancy.enforced",
            DeliveryIssueCode::PolicyRuleBlocked,
            "Tenant Context enforcement",
        ),
        (
            "call_policies.declared",
            DeliveryIssueCode::PolicyRuleBlocked,
            "Call Policy declarations",
        ),
        (
            "dependencies.ready",
            DeliveryIssueCode::PolicyRuleBlocked,
            "dependency readiness",
        ),
        (
            "resilience.declared",
            DeliveryIssueCode::PolicyRuleBlocked,
            "resilience declarations",
        ),
        (
            "reliability.complete",
            DeliveryIssueCode::ReliabilityEvidenceMissing,
            "Reliability Contract evidence",
        ),
        (
            "edge.valid",
            DeliveryIssueCode::EdgeExposureUnsafe,
            "Edge Contract evidence",
        ),
        (
            "environment.verification_fresh",
            DeliveryIssueCode::ObservationStale,
            "Environment Verification",
        ),
    ] {
        if !facts.get(key).copied().flatten().unwrap_or(false) {
            issues.push(issue(
                code,
                format!("Required production {subject} is false or unknown."),
                format!("Provide current {subject} before production eligibility."),
                "Refresh the missing evidence and rerun can-I-deploy.",
            ));
        }
    }
    if !authority_valid {
        for value in facts.values_mut() {
            *value = Some(false);
        }
    }
    facts.insert("production.eligible".to_owned(), Some(issues.is_empty()));

    #[derive(Serialize)]
    struct EvidenceContent<'a> {
        protocol: &'a str,
        release_id: &'a str,
        release_digest: &'a str,
        provider_id: &'a str,
        input_digest: &'a str,
        system_graph_digest: &'a str,
        facts: &'a BTreeMap<String, Option<bool>>,
        contract_retirement: &'a [ContractRetirementEvidence],
        issues: &'a [DeliveryIssue],
    }
    let content = EvidenceContent {
        protocol: PRODUCTION_ELIGIBILITY_PROTOCOL,
        release_id: &input.release_id,
        release_digest: &input.release_digest,
        provider_id: &input.provider_id,
        input_digest: &input_digest,
        system_graph_digest: &input.system_graph_digest,
        facts: &facts,
        contract_retirement: &retirement,
        issues: &issues,
    };
    let evidence_digest = extraction_input_digest(
        serde_json::to_vec(&content).expect("eligibility evidence must serialize"),
    );
    ProductionEligibilityEvidence {
        protocol: PRODUCTION_ELIGIBILITY_PROTOCOL.to_owned(),
        evidence_id: format!("production-eligibility:{evidence_digest}"),
        evidence_digest,
        release_id: input.release_id.clone(),
        release_digest: input.release_digest.clone(),
        provider_id: input.provider_id.clone(),
        input_digest,
        system_graph_digest: input.system_graph_digest.clone(),
        decision: if issues.is_empty() {
            DeliveryDecision::Passed
        } else {
            DeliveryDecision::Blocked
        },
        facts,
        contract_retirement: retirement,
        issues,
        effects: DeliveryEffects::default(),
    }
}

const fn option_true(value: Option<bool>) -> bool {
    matches!(value, Some(true))
}

#[must_use]
pub fn production_eligibility_evidence_integrity_is_valid(
    evidence: &ProductionEligibilityEvidence,
    input: &ProductionEligibilityInput,
    release: &ServiceRelease,
    provider: &dyn ReleaseTrustProvider,
) -> bool {
    evidence == &evaluate_production_eligibility(input, release, provider)
}

pub fn attest_production_eligibility_input(
    release: &ServiceRelease,
    provider: &dyn ReleaseTrustProvider,
    provider_id: impl Into<String>,
    mut input: ProductionEligibilityInput,
) -> Result<ProductionEligibilityInput, DeliveryIssue> {
    if !service_release_integrity_is_valid(release) {
        return Err(issue(
            DeliveryIssueCode::ReleaseTampered,
            "Production Eligibility cannot attest an invalid Service Release.",
            "Use the exact canonical Service Release as the eligibility subject.",
            "Reassemble the release and collect eligibility evidence again.",
        ));
    }
    input.release_id = release.release_id.clone();
    input.release_digest = release.release_digest.clone();
    input.provider_id = provider_id.into();
    input.provider_proof.clear();
    let subject = production_eligibility_input_digest(&input);
    input.provider_proof = provider
        .sign(input.provider_id.as_str(), subject.as_str())
        .ok_or_else(|| {
            issue(
                DeliveryIssueCode::PolicyEvidenceMissing,
                "The selected Production Eligibility provider is not trusted.",
                "Use a configured evidence provider without exposing signing material.",
                "Configure the provider and attest the eligibility input again.",
            )
        })?;
    Ok(input)
}

fn production_eligibility_input_digest(input: &ProductionEligibilityInput) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Content<'a> {
        release_id: &'a str,
        release_digest: &'a str,
        provider_id: &'a str,
        system_graph_digest: &'a str,
        contracts: &'a [ContractCompatibilityInput],
        migrations: &'a [MigrationCompatibilityInput],
        workflows: &'a WorkflowCompatibilityInput,
        rollback: &'a RollbackCompatibilityInput,
        provider_compatibility_verified: Option<bool>,
        workload_identity_production: Option<bool>,
        tenancy_mode_production: Option<bool>,
        tenant_context_enforced: Option<bool>,
        call_policies_declared: Option<bool>,
        dependencies_ready: Option<bool>,
        resilience_declared: Option<bool>,
        reliability_contract_complete: Option<bool>,
        edge_contract_valid: Option<bool>,
        environment_verification_fresh: Option<bool>,
    }
    let content = Content {
        release_id: &input.release_id,
        release_digest: &input.release_digest,
        provider_id: &input.provider_id,
        system_graph_digest: &input.system_graph_digest,
        contracts: &input.contracts,
        migrations: &input.migrations,
        workflows: &input.workflows,
        rollback: &input.rollback,
        provider_compatibility_verified: input.provider_compatibility_verified,
        workload_identity_production: input.workload_identity_production,
        tenancy_mode_production: input.tenancy_mode_production,
        tenant_context_enforced: input.tenant_context_enforced,
        call_policies_declared: input.call_policies_declared,
        dependencies_ready: input.dependencies_ready,
        resilience_declared: input.resilience_declared,
        reliability_contract_complete: input.reliability_contract_complete,
        edge_contract_valid: input.edge_contract_valid,
        environment_verification_fresh: input.environment_verification_fresh,
    };
    extraction_input_digest(serde_json::to_vec(&content).expect("eligibility input must serialize"))
}

fn major_version(version: &str) -> Option<u32> {
    version
        .trim_start_matches('v')
        .split('.')
        .next()?
        .parse()
        .ok()
}

fn migration_phase(phase: &str) -> Option<MigrationPhase> {
    match phase {
        "expand" => Some(MigrationPhase::Expand),
        "backfill" => Some(MigrationPhase::Backfill),
        "verify" => Some(MigrationPhase::Verify),
        "contract" => Some(MigrationPhase::Contract),
        "irreversible" => Some(MigrationPhase::Irreversible),
        _ => None,
    }
}
