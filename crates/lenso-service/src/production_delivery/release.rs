use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{DeliveryIssue, DeliveryIssueCode, issue, valid_sha256_digest};

pub const SERVICE_RELEASE_PROTOCOL: &str = "lenso.service-release.v1";
const SERVICE_RELEASE_SCHEMA_ID: &str =
    "https://contracts.lenso.local/delivery/lenso.service-release.v1.schema.json";

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryEvidenceReference {
    pub reference: String,
    pub digest: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseModule {
    pub module_id: String,
    pub module_version: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseWorkloadRole {
    Api,
    Worker,
    Migration,
    Extension,
}

impl ReleaseWorkloadRole {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Worker => "worker",
            Self::Migration => "migration",
            Self::Extension => "extension",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseProvenance {
    pub reference: String,
    pub digest: String,
    pub source: String,
    pub builder: String,
    #[serde(default)]
    pub input_digests: Vec<String>,
    #[serde(default)]
    pub subject_digests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadArtifact {
    pub workload_id: String,
    pub role: ReleaseWorkloadRole,
    pub artifact_reference: String,
    pub artifact_digest: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_tag: Option<String>,
    pub sbom: DeliveryEvidenceReference,
    pub provenance: ReleaseProvenance,
    pub signature_subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseContractVersion {
    pub contract_id: String,
    pub version: String,
    pub kind: String,
    pub artifact: DeliveryEvidenceReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseMigration {
    pub migration_id: String,
    pub phase: String,
    pub artifact: DeliveryEvidenceReference,
    pub reversible: bool,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRolloutGate {
    pub gate_id: String,
    pub evidence_kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRollbackConstraints {
    pub previous_release_required: bool,
    pub automatic_allowed: bool,
    pub blocked_by_irreversible_migration: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRetention {
    pub evidence_days: u32,
    pub artifact_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSignature {
    pub signer: String,
    pub subject_digest: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseInput {
    pub service_id: String,
    pub service_version: String,
    pub modules: Vec<ReleaseModule>,
    pub workloads: Vec<WorkloadArtifact>,
    pub contract_versions: Vec<ReleaseContractVersion>,
    pub config_contract: DeliveryEvidenceReference,
    pub reliability_contract: DeliveryEvidenceReference,
    pub migrations: Vec<ReleaseMigration>,
    pub workflow_compatibility: Vec<DeliveryEvidenceReference>,
    pub verification_evidence: Vec<DeliveryEvidenceReference>,
    pub rollout_gates: Vec<ReleaseRolloutGate>,
    pub rollback: ReleaseRollbackConstraints,
    pub retention: ReleaseRetention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRelease {
    pub protocol: String,
    pub release_id: String,
    pub release_digest: String,
    pub service_id: String,
    pub service_version: String,
    pub modules: Vec<ReleaseModule>,
    pub workloads: Vec<WorkloadArtifact>,
    pub contract_versions: Vec<ReleaseContractVersion>,
    pub config_contract: DeliveryEvidenceReference,
    pub reliability_contract: DeliveryEvidenceReference,
    pub migrations: Vec<ReleaseMigration>,
    pub workflow_compatibility: Vec<DeliveryEvidenceReference>,
    pub verification_evidence: Vec<DeliveryEvidenceReference>,
    pub rollout_gates: Vec<ReleaseRolloutGate>,
    pub rollback: ReleaseRollbackConstraints,
    pub retention: ReleaseRetention,
    #[serde(default)]
    pub signatures: Vec<ReleaseSignature>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceReleaseContent<'a> {
    protocol: &'a str,
    service_id: &'a str,
    service_version: &'a str,
    modules: &'a [ReleaseModule],
    workloads: &'a [WorkloadArtifact],
    contract_versions: &'a [ReleaseContractVersion],
    config_contract: &'a DeliveryEvidenceReference,
    reliability_contract: &'a DeliveryEvidenceReference,
    migrations: &'a [ReleaseMigration],
    workflow_compatibility: &'a [DeliveryEvidenceReference],
    verification_evidence: &'a [DeliveryEvidenceReference],
    rollout_gates: &'a [ReleaseRolloutGate],
    rollback: ReleaseRollbackConstraints,
    retention: ReleaseRetention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseDiffEntry {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseDiff {
    pub protocol: String,
    pub from_release_id: String,
    pub to_release_id: String,
    pub entries: Vec<ServiceReleaseDiffEntry>,
}

pub fn assemble_service_release(
    mut input: ServiceReleaseInput,
) -> Result<ServiceRelease, Vec<DeliveryIssue>> {
    normalize_release_input(&mut input);
    let issues = validate_release_input(&input);
    if !issues.is_empty() {
        return Err(issues);
    }

    let content = ServiceReleaseContent {
        protocol: SERVICE_RELEASE_PROTOCOL,
        service_id: &input.service_id,
        service_version: &input.service_version,
        modules: &input.modules,
        workloads: &input.workloads,
        contract_versions: &input.contract_versions,
        config_contract: &input.config_contract,
        reliability_contract: &input.reliability_contract,
        migrations: &input.migrations,
        workflow_compatibility: &input.workflow_compatibility,
        verification_evidence: &input.verification_evidence,
        rollout_gates: &input.rollout_gates,
        rollback: input.rollback,
        retention: input.retention,
    };
    let release_digest = extraction_input_digest(
        serde_json::to_vec(&content).expect("validated Service Release content must serialize"),
    );
    Ok(ServiceRelease {
        protocol: SERVICE_RELEASE_PROTOCOL.to_owned(),
        release_id: format!("service-release:{release_digest}"),
        release_digest,
        service_id: input.service_id,
        service_version: input.service_version,
        modules: input.modules,
        workloads: input.workloads,
        contract_versions: input.contract_versions,
        config_contract: input.config_contract,
        reliability_contract: input.reliability_contract,
        migrations: input.migrations,
        workflow_compatibility: input.workflow_compatibility,
        verification_evidence: input.verification_evidence,
        rollout_gates: input.rollout_gates,
        rollback: input.rollback,
        retention: input.retention,
        signatures: Vec::new(),
    })
}

#[must_use]
pub fn service_release_integrity_is_valid(release: &ServiceRelease) -> bool {
    if release.protocol != SERVICE_RELEASE_PROTOCOL
        || release.release_id != format!("service-release:{}", release.release_digest)
    {
        return false;
    }
    let content = ServiceReleaseContent {
        protocol: &release.protocol,
        service_id: &release.service_id,
        service_version: &release.service_version,
        modules: &release.modules,
        workloads: &release.workloads,
        contract_versions: &release.contract_versions,
        config_contract: &release.config_contract,
        reliability_contract: &release.reliability_contract,
        migrations: &release.migrations,
        workflow_compatibility: &release.workflow_compatibility,
        verification_evidence: &release.verification_evidence,
        rollout_gates: &release.rollout_gates,
        rollback: release.rollback,
        retention: release.retention,
    };
    let input = ServiceReleaseInput {
        service_id: release.service_id.clone(),
        service_version: release.service_version.clone(),
        modules: release.modules.clone(),
        workloads: release.workloads.clone(),
        contract_versions: release.contract_versions.clone(),
        config_contract: release.config_contract.clone(),
        reliability_contract: release.reliability_contract.clone(),
        migrations: release.migrations.clone(),
        workflow_compatibility: release.workflow_compatibility.clone(),
        verification_evidence: release.verification_evidence.clone(),
        rollout_gates: release.rollout_gates.clone(),
        rollback: release.rollback,
        retention: release.retention,
    };
    let mut normalized = input.clone();
    normalize_release_input(&mut normalized);
    input == normalized
        && validate_release_input(&input).is_empty()
        && serde_json::to_vec(&content)
            .map(extraction_input_digest)
            .is_ok_and(|digest| digest == release.release_digest)
}

#[must_use]
pub fn diff_service_releases(from: &ServiceRelease, to: &ServiceRelease) -> ServiceReleaseDiff {
    let mut entries = Vec::new();
    let from_content = release_content_value(from);
    let to_content = release_content_value(to);
    for (field, subject) in [
        ("serviceId", "service.identity"),
        ("serviceVersion", "service.version"),
        ("modules", "modules"),
        ("workloads", "workloads"),
        ("contractVersions", "contracts"),
        ("configContract", "config.contract"),
        ("reliabilityContract", "reliability.contract"),
        ("migrations", "migrations"),
        ("workflowCompatibility", "workflow.compatibility"),
        ("verificationEvidence", "verification.evidence"),
        ("rolloutGates", "rollout.gates"),
        ("rollback", "rollback.constraints"),
        ("retention", "retention"),
    ] {
        let before = from_content.get(field);
        let after = to_content.get(field);
        if before != after {
            entries.push(ServiceReleaseDiffEntry {
                subject: subject.to_owned(),
                before: before.map(stable_json),
                after: after.map(stable_json),
            });
        }
    }
    ServiceReleaseDiff {
        protocol: "lenso.service-release-diff.v1".to_owned(),
        from_release_id: from.release_id.clone(),
        to_release_id: to.release_id.clone(),
        entries,
    }
}

fn release_content_value(release: &ServiceRelease) -> Value {
    serde_json::to_value(ServiceReleaseContent {
        protocol: &release.protocol,
        service_id: &release.service_id,
        service_version: &release.service_version,
        modules: &release.modules,
        workloads: &release.workloads,
        contract_versions: &release.contract_versions,
        config_contract: &release.config_contract,
        reliability_contract: &release.reliability_contract,
        migrations: &release.migrations,
        workflow_compatibility: &release.workflow_compatibility,
        verification_evidence: &release.verification_evidence,
        rollout_gates: &release.rollout_gates,
        rollback: release.rollback,
        retention: release.retention,
    })
    .expect("Service Release content must serialize")
}

fn stable_json(value: &Value) -> String {
    serde_json::to_string(value).expect("Service Release diff value must serialize")
}

#[must_use]
pub fn service_release_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ServiceRelease))
        .expect("Service Release schema must serialize");
    if let Some(object) = schema.as_object_mut() {
        object.insert("$id".to_owned(), json!(SERVICE_RELEASE_SCHEMA_ID));
        object.insert("title".to_owned(), json!("Lenso Service Release v1"));
    }
    schema
}

fn normalize_release_input(input: &mut ServiceReleaseInput) {
    input.modules.sort();
    input.workloads.sort_by(|left, right| {
        (left.role.as_str(), left.workload_id.as_str())
            .cmp(&(right.role.as_str(), right.workload_id.as_str()))
    });
    input.contract_versions.sort_by(|left, right| {
        (&left.contract_id, &left.version, &left.kind).cmp(&(
            &right.contract_id,
            &right.version,
            &right.kind,
        ))
    });
    input.migrations.sort_by(|left, right| {
        (&left.migration_id, &left.phase).cmp(&(&right.migration_id, &right.phase))
    });
    input.workflow_compatibility.sort();
    input.verification_evidence.sort();
    input.rollout_gates.sort();
    for workload in &mut input.workloads {
        workload.provenance.input_digests.sort();
        workload.provenance.subject_digests.sort();
    }
}

fn validate_release_input(input: &ServiceReleaseInput) -> Vec<DeliveryIssue> {
    let mut issues = Vec::new();
    if input.service_id.trim().is_empty()
        || input.service_version.trim().is_empty()
        || input.modules.is_empty()
        || input.workloads.is_empty()
        || input.contract_versions.is_empty()
    {
        issues.push(issue(
            DeliveryIssueCode::ReleaseInputInvalid,
            "A Service Release requires Service identity, version, Modules, Workloads, and Contract Versions.",
            "Supply the complete environment-independent Service Release inputs.",
            "Correct the release input and assemble it again.",
        ));
    }

    let mut workload_ids = BTreeSet::new();
    for workload in &input.workloads {
        if workload.workload_id.trim().is_empty()
            || workload.artifact_reference.trim().is_empty()
            || workload.media_type.trim().is_empty()
            || workload.signature_subject.trim().is_empty()
        {
            issues.push(issue(
                DeliveryIssueCode::ReleaseInputInvalid,
                "A Workload is missing its identity, media type, or signature subject.",
                "Declare every Workload artifact completely.",
                "Correct the Workload declaration and assemble the release again.",
            ));
        }
        if !workload_ids.insert(workload.workload_id.as_str()) {
            issues.push(issue(
                DeliveryIssueCode::ReleaseInputInvalid,
                format!(
                    "Workload `{}` is declared more than once.",
                    workload.workload_id
                ),
                "Keep exactly one artifact declaration per Workload identity.",
                "Remove the duplicate Workload and assemble the release again.",
            ));
        }
        if !valid_sha256_digest(&workload.artifact_digest) {
            issues.push(issue(
                DeliveryIssueCode::MutableArtifactReference,
                format!(
                    "Workload `{}` is not pinned by an immutable sha256 digest.",
                    workload.workload_id
                ),
                "Resolve the artifact through existing build infrastructure and supply its immutable digest.",
                "Replace the mutable artifact reference and assemble the release again.",
            ));
        }
        if workload.sbom.reference.trim().is_empty() || !valid_sha256_digest(&workload.sbom.digest)
        {
            issues.push(issue(
                DeliveryIssueCode::MissingSbom,
                format!(
                    "Workload `{}` has no digest-pinned SBOM.",
                    workload.workload_id
                ),
                "Attach an addressable SBOM produced by the build pipeline.",
                "Generate and attach the Workload SBOM.",
            ));
        }
        if workload.provenance.reference.trim().is_empty()
            || workload.provenance.source.trim().is_empty()
            || workload.provenance.builder.trim().is_empty()
            || !valid_sha256_digest(&workload.provenance.digest)
            || workload.provenance.input_digests.is_empty()
            || workload
                .provenance
                .input_digests
                .iter()
                .any(|digest| !valid_sha256_digest(digest))
        {
            issues.push(issue(
                DeliveryIssueCode::MissingProvenance,
                format!(
                    "Workload `{}` has incomplete provenance.",
                    workload.workload_id
                ),
                "Attach provenance with source, builder, inputs, and subjects.",
                "Generate and attach complete Workload provenance.",
            ));
        }
    }

    for evidence in input
        .contract_versions
        .iter()
        .map(|contract| &contract.artifact)
        .chain(std::iter::once(&input.config_contract))
        .chain(std::iter::once(&input.reliability_contract))
        .chain(input.migrations.iter().map(|migration| &migration.artifact))
        .chain(input.workflow_compatibility.iter())
        .chain(input.verification_evidence.iter())
    {
        if evidence.reference.trim().is_empty() || !valid_sha256_digest(&evidence.digest) {
            issues.push(issue(
                DeliveryIssueCode::ReleaseInputInvalid,
                "Release evidence must have a stable reference and sha256 digest.",
                "Regenerate the versioned evidence and pin its digest.",
                "Correct the evidence reference and assemble the release again.",
            ));
        }
    }
    issues
}
