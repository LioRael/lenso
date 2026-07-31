use crate::{ArtifactReference, AttestationReference, ModuleDelivery, ModuleRelease, digest_json};
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG_SNAPSHOT_PROTOCOL: &str = "lenso.catalog-snapshot.v1";
pub const VERIFICATION_PROFILE_PROTOCOL: &str = "lenso.module-verification-profile.v1";
pub const VERIFICATION_RECEIPT_PROTOCOL: &str = "lenso.module-verification-receipt.v1";
pub const LINKED_PROVENANCE_RECEIPT_PROTOCOL: &str = "lenso.linked-provenance-receipt.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GitHubIdentity {
    pub login: String,
    pub user_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PublisherStatus {
    Active,
    Suspended,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublisherRecord {
    pub publisher_id: String,
    pub namespaces: Vec<String>,
    pub owner: GitHubIdentity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<GitHubIdentity>,
    pub github_owner_id: u64,
    pub github_repository_id: u64,
    pub repository: String,
    pub publishing_workflow: String,
    pub security_contact: String,
    pub status: PublisherStatus,
    pub source_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogApproval {
    pub actor: GitHubIdentity,
    pub source_revision: String,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublisherGovernanceAction {
    OrdinaryChange,
    NormalTransfer {
        receiving_owner: GitHubIdentity,
    },
    RecoveryTransfer {
        receiving_owner: GitHubIdentity,
        waiting_started_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublisherGovernanceDecision {
    pub approved: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
}

pub fn evaluate_publisher_governance(
    publisher: &PublisherRecord,
    submitter_user_id: u64,
    catalog_maintainer_ids: &BTreeSet<u64>,
    proposed_source_revision: &str,
    action: &PublisherGovernanceAction,
    approvals: &[CatalogApproval],
    now: DateTime<Utc>,
) -> PublisherGovernanceDecision {
    let publisher_actor_ids = std::iter::once(publisher.owner.user_id)
        .chain(
            publisher
                .maintainers
                .iter()
                .map(|maintainer| maintainer.user_id),
        )
        .collect::<BTreeSet<_>>();
    let valid_approvers = approvals
        .iter()
        .filter(|approval| approval.source_revision == proposed_source_revision)
        .map(|approval| approval.actor.user_id)
        .collect::<BTreeSet<_>>();
    let independent_catalog_approvers = valid_approvers
        .iter()
        .filter(|actor| {
            catalog_maintainer_ids.contains(actor)
                && !publisher_actor_ids.contains(actor)
                && **actor != submitter_user_id
        })
        .copied()
        .collect::<BTreeSet<_>>();
    let mut reasons = Vec::new();

    if proposed_source_revision.trim().is_empty() {
        reasons.push("source_revision_missing".to_owned());
    }
    let self_approval_attempted = catalog_maintainer_ids.contains(&submitter_user_id)
        && valid_approvers.contains(&submitter_user_id);

    match action {
        PublisherGovernanceAction::OrdinaryChange => {
            if independent_catalog_approvers.is_empty() {
                reasons.push("independent_catalog_approval_missing".to_owned());
                if self_approval_attempted {
                    reasons.push("self_approval_forbidden".to_owned());
                }
            }
        }
        PublisherGovernanceAction::NormalTransfer { receiving_owner } => {
            if submitter_user_id != publisher.owner.user_id {
                reasons.push("only_current_owner_may_transfer".to_owned());
            }
            if !valid_approvers.contains(&publisher.owner.user_id) {
                reasons.push("current_owner_approval_missing".to_owned());
            }
            if !valid_approvers.contains(&receiving_owner.user_id) {
                reasons.push("receiving_owner_approval_missing".to_owned());
            }
            if independent_catalog_approvers.is_empty() {
                reasons.push("independent_catalog_approval_missing".to_owned());
                if self_approval_attempted {
                    reasons.push("self_approval_forbidden".to_owned());
                }
            }
        }
        PublisherGovernanceAction::RecoveryTransfer {
            waiting_started_at, ..
        } => {
            let catalog_approvers = valid_approvers
                .intersection(catalog_maintainer_ids)
                .filter(|actor| **actor != submitter_user_id)
                .count();
            if catalog_approvers < 2 {
                reasons.push("two_catalog_approvals_required".to_owned());
                if self_approval_attempted {
                    reasons.push("self_approval_forbidden".to_owned());
                }
            }
            if now.signed_duration_since(*waiting_started_at) < Duration::days(14) {
                reasons.push("recovery_waiting_period_incomplete".to_owned());
            }
        }
    }
    reasons.sort();
    reasons.dedup();
    PublisherGovernanceDecision {
        approved: reasons.is_empty(),
        reason_codes: reasons,
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleDeliveryKind {
    Linked,
    Service,
}

impl From<&ModuleDelivery> for ModuleDeliveryKind {
    fn from(delivery: &ModuleDelivery) -> Self {
        match delivery {
            ModuleDelivery::Linked(_) => Self::Linked,
            ModuleDelivery::Service(_) => Self::Service,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogReleaseRecord {
    pub publisher_id: String,
    pub module_id: String,
    pub version: String,
    pub release_digest: String,
    pub release: ArtifactReference,
    pub delivery_kind: ModuleDeliveryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogModuleMetadata {
    pub module_id: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<String>,
    pub source_revision: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleLifecycleFacet {
    Deprecated,
    Yanked,
    SecurityBlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleLifecycleChange {
    Set,
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleLifecycleRecord {
    pub release_digest: String,
    pub facet: ModuleLifecycleFacet,
    pub change: ModuleLifecycleChange,
    pub reason_code: String,
    pub evidence_reference: String,
    pub actor: GitHubIdentity,
    pub source_revision: String,
    pub sequence: u64,
    pub effective_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement_module_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_conditions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrustedPublishingEvidence {
    pub repository: String,
    pub repository_id: u64,
    pub workflow: String,
    pub run_id: u64,
    pub commit_sha: String,
    pub oidc_issuer: String,
    pub runner_environment: String,
    pub runner_image_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAttestationEvidence {
    pub attestation: AttestationReference,
    pub repository: String,
    pub repository_id: u64,
    pub workflow: String,
    pub run_id: u64,
    pub commit_sha: String,
    pub oidc_issuer: String,
    pub runner_environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanBuildEvidence {
    pub lenso_version: String,
    pub cli_version: String,
    pub starter_digest: String,
    pub toolchain: String,
    pub application_lock_digest: String,
    pub runner_image_digest: String,
    pub builder: VerifierIdentity,
    pub commands: Vec<String>,
    pub checks: Vec<VerificationCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkedProvenanceReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub issued_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_receipt_digest: Option<String>,
    pub publisher_id: String,
    pub module_release_digest: String,
    pub package: String,
    pub crate_version: String,
    pub archive_size: u64,
    pub archive_checksum: String,
    pub trusted_publishing: TrustedPublishingEvidence,
    pub artifact_attestation: ArtifactAttestationEvidence,
    pub clean_build: CleanBuildEvidence,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOperation {
    FreshInstall,
    Upgrade,
    Restore,
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationCheckOutcome {
    Passed,
    Failed,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
    pub check_id: String,
    pub outcome: VerificationCheckOutcome,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationProfile {
    pub protocol: String,
    pub profile_id: String,
    pub policy_revision: String,
    pub required_checks: BTreeMap<VerificationOperation, Vec<String>>,
    pub accepted_verifier_repository_ids: Vec<u64>,
    pub accepted_verifier_workflows: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleVerificationCell {
    pub module_release_digest: String,
    pub operation: VerificationOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_release_digest: Option<String>,
    pub lenso_version: String,
    pub host_version: String,
    pub cli_version: String,
    pub starter_digest: String,
    pub management_engine_version: String,
    pub delivery_digest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    pub target: String,
    pub os: String,
    pub architecture: String,
    pub runner_image_digest: String,
    pub rust_version: String,
    pub cargo_version: String,
    pub store_engine: String,
    pub store_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol_digests: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_artifact_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_host_api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_manager_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_lock_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationToolchainEvidence {
    pub application_lock_digest: String,
    pub cargo_lock_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_lock_digest: Option<String>,
    pub config_input_digest: String,
    pub migration_history_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifierIdentity {
    pub repository: String,
    pub repository_id: u64,
    pub workflow: String,
    pub run_id: u64,
    pub commit_sha: String,
    pub oidc_issuer: String,
    pub signer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleVerificationReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub publisher_id: String,
    pub manifest_digest: String,
    pub catalog_snapshot_digest: String,
    pub verification_profile_digest: String,
    pub cell: ModuleVerificationCell,
    pub outcome: VerificationOutcome,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
    pub toolchain_evidence: VerificationToolchainEvidence,
    pub commands: Vec<String>,
    pub checks: Vec<VerificationCheck>,
    pub verifier: VerifierIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttestedVerificationReceipt {
    pub receipt: ModuleVerificationReceipt,
    pub receipt_digest: String,
    pub attestation: AttestationReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationRevocation {
    pub receipt_digest: String,
    pub reason_code: String,
    pub evidence_reference: String,
    pub actor: GitHubIdentity,
    pub source_revision: String,
    pub effective_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogSnapshot {
    pub protocol: String,
    pub source_revision: String,
    pub generated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_snapshot_digest: Option<String>,
    pub verification_profile: VerificationProfile,
    pub verification_profile_digest: String,
    pub publishers: Vec<PublisherRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<CatalogModuleMetadata>,
    pub releases: Vec<CatalogReleaseRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifecycle: Vec<ModuleLifecycleRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_provenance: Vec<LinkedProvenanceReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verification_receipts: Vec<AttestedVerificationReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verification_revocations: Vec<VerificationRevocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogSnapshotEnvelope {
    pub snapshot: CatalogSnapshot,
    pub snapshot_digest: String,
    pub attestation: AttestationReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogAttestationTrustPolicy {
    pub trusted_issuers: Vec<String>,
    pub trusted_signers: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct VerifiedCatalogSnapshot<'a> {
    snapshot: &'a CatalogSnapshot,
    snapshot_digest: &'a str,
}

impl<'a> VerifiedCatalogSnapshot<'a> {
    #[must_use]
    pub fn snapshot(&self) -> &'a CatalogSnapshot {
        self.snapshot
    }

    #[must_use]
    pub fn snapshot_digest(&self) -> &'a str {
        self.snapshot_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogContractIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredCompatibilityState {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    Verified,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationEvaluation {
    pub state: VerificationState,
    pub reason_code: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub receipt_digests: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleLifecycleState {
    pub deprecated: bool,
    pub yanked: bool,
    pub security_blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogAction {
    Discover,
    Install,
    Update,
    Restore,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleEligibilityState {
    Eligible,
    EligibleWithWarning,
    Blocked,
    BreakGlassOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleTrustPolicy {
    pub verification: VerificationRequirement,
    pub maximum_mutation_age_seconds: u64,
    pub stale_snapshot: StaleSnapshotPolicy,
    pub compatibility: CompatibilityPolicy,
    pub security_restore: SecurityRestorePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationRequirement {
    AllowUnknown,
    RequireVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StaleSnapshotPolicy {
    Reject,
    AllowOffline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityPolicy {
    Strict,
    AllowOverride,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecurityRestorePolicy {
    Block,
    BreakGlass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleEligibility {
    pub action: CatalogAction,
    pub state: ModuleEligibilityState,
    pub declared_compatibility: DeclaredCompatibilityState,
    pub verification: VerificationEvaluation,
    pub lifecycle: ModuleLifecycleState,
    pub snapshot_age_seconds: u64,
    pub snapshot_fresh: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
}

pub fn validate_catalog_snapshot(
    envelope: &CatalogSnapshotEnvelope,
    releases: &[ModuleRelease],
) -> Vec<CatalogContractIssue> {
    let mut issues = Vec::new();
    let snapshot = &envelope.snapshot;
    if snapshot.protocol != CATALOG_SNAPSHOT_PROTOCOL {
        push_issue(
            &mut issues,
            "$.snapshot.protocol",
            "unsupported catalog protocol",
        );
    }
    if snapshot.source_revision.trim().is_empty() {
        push_issue(
            &mut issues,
            "$.snapshot.source_revision",
            "Catalog source revision must be non-empty",
        );
    }
    if let Some(previous) = &snapshot.previous_snapshot_digest {
        validate_digest(previous, "$.snapshot.previous_snapshot_digest", &mut issues);
    }
    match digest_json(snapshot) {
        Ok(digest) if digest == envelope.snapshot_digest => {}
        Ok(_) => push_issue(&mut issues, "$.snapshot_digest", "snapshot digest mismatch"),
        Err(error) => push_issue(
            &mut issues,
            "$.snapshot",
            format!("snapshot cannot be canonicalized: {error}"),
        ),
    }
    validate_digest(&envelope.snapshot_digest, "$.snapshot_digest", &mut issues);
    if envelope.attestation.digest != envelope.snapshot_digest {
        push_issue(
            &mut issues,
            "$.attestation.digest",
            "snapshot attestation must bind the exact snapshot digest",
        );
    }
    if envelope.attestation.locator.trim().is_empty()
        || envelope.attestation.issuer.trim().is_empty()
        || envelope.attestation.signer.trim().is_empty()
    {
        push_issue(
            &mut issues,
            "$.attestation",
            "snapshot attestation identity fields must be non-empty",
        );
    }
    if snapshot.verification_profile.protocol != VERIFICATION_PROFILE_PROTOCOL {
        push_issue(
            &mut issues,
            "$.snapshot.verification_profile.protocol",
            "unsupported verification profile protocol",
        );
    }
    match digest_json(&snapshot.verification_profile) {
        Ok(digest) if digest == snapshot.verification_profile_digest => {}
        _ => push_issue(
            &mut issues,
            "$.snapshot.verification_profile_digest",
            "verification profile digest mismatch",
        ),
    }
    validate_digest(
        &snapshot.verification_profile_digest,
        "$.snapshot.verification_profile_digest",
        &mut issues,
    );
    validate_verification_profile(&snapshot.verification_profile, &mut issues);

    validate_publishers(snapshot, &mut issues);
    validate_metadata(snapshot, &mut issues);
    validate_releases(snapshot, releases, &mut issues);
    validate_provenance(snapshot, releases, &mut issues);
    validate_receipts(snapshot, releases, &mut issues);
    validate_lifecycle(snapshot, &mut issues);
    issues
}

pub fn admit_catalog_snapshot<'a>(
    envelope: &'a CatalogSnapshotEnvelope,
    releases: &[ModuleRelease],
    trust_policy: &CatalogAttestationTrustPolicy,
    cryptographic_attestation_verified: bool,
) -> Result<VerifiedCatalogSnapshot<'a>, Vec<CatalogContractIssue>> {
    let mut issues = validate_catalog_snapshot(envelope, releases);
    if !cryptographic_attestation_verified {
        push_issue(
            &mut issues,
            "$.attestation",
            "snapshot attestation has not been cryptographically verified",
        );
    }
    if !trust_policy
        .trusted_issuers
        .contains(&envelope.attestation.issuer)
    {
        push_issue(
            &mut issues,
            "$.attestation.issuer",
            "snapshot attestation issuer is not trusted",
        );
    }
    if !trust_policy
        .trusted_signers
        .contains(&envelope.attestation.signer)
    {
        push_issue(
            &mut issues,
            "$.attestation.signer",
            "snapshot attestation signer is not trusted",
        );
    }
    if issues.is_empty() {
        Ok(VerifiedCatalogSnapshot {
            snapshot: &envelope.snapshot,
            snapshot_digest: &envelope.snapshot_digest,
        })
    } else {
        Err(issues)
    }
}

pub fn verification_evaluation(
    snapshot: &CatalogSnapshot,
    requested: &ModuleVerificationCell,
) -> VerificationEvaluation {
    let revoked = snapshot
        .verification_revocations
        .iter()
        .map(|record| record.receipt_digest.as_str())
        .collect::<BTreeSet<_>>();
    let mut passed = Vec::new();
    let mut failed = Vec::new();
    let mut obsolete = false;
    for accepted in &snapshot.verification_receipts {
        if accepted.receipt.cell != *requested || revoked.contains(accepted.receipt_digest.as_str())
        {
            continue;
        }
        if accepted.receipt.verification_profile_digest != snapshot.verification_profile_digest {
            obsolete = true;
            continue;
        }
        match accepted.receipt.outcome {
            VerificationOutcome::Passed => passed.push(accepted.receipt_digest.clone()),
            VerificationOutcome::Failed => failed.push(accepted.receipt_digest.clone()),
        }
    }
    passed.sort();
    failed.sort();
    if !passed.is_empty() && !failed.is_empty() {
        let mut receipts = passed;
        receipts.extend(failed);
        receipts.sort();
        return VerificationEvaluation {
            state: VerificationState::Unknown,
            reason_code: "receipt_conflict".to_owned(),
            receipt_digests: receipts,
        };
    }
    if !passed.is_empty() {
        return VerificationEvaluation {
            state: VerificationState::Verified,
            reason_code: "exact_cell_passed".to_owned(),
            receipt_digests: passed,
        };
    }
    if !failed.is_empty() {
        return VerificationEvaluation {
            state: VerificationState::Failed,
            reason_code: "exact_cell_failed".to_owned(),
            receipt_digests: failed,
        };
    }
    VerificationEvaluation {
        state: VerificationState::Unknown,
        reason_code: if obsolete {
            "obsolete_profile"
        } else {
            "missing_receipt"
        }
        .to_owned(),
        receipt_digests: Vec::new(),
    }
}

pub fn lifecycle_state(snapshot: &CatalogSnapshot, release_digest: &str) -> ModuleLifecycleState {
    let mut records = snapshot
        .lifecycle
        .iter()
        .filter(|record| record.release_digest == release_digest)
        .collect::<Vec<_>>();
    records.sort_by_key(|record| (record.effective_at, record.sequence));
    let mut state = ModuleLifecycleState::default();
    for record in records {
        let active = record.change == ModuleLifecycleChange::Set;
        match record.facet {
            ModuleLifecycleFacet::Deprecated => state.deprecated = active,
            ModuleLifecycleFacet::Yanked => state.yanked = active,
            ModuleLifecycleFacet::SecurityBlocked => state.security_blocked = active,
        }
    }
    state
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate_module_eligibility(
    verified_snapshot: &VerifiedCatalogSnapshot<'_>,
    release_digest: &str,
    requested_cell: &ModuleVerificationCell,
    declared_compatibility: DeclaredCompatibilityState,
    action: CatalogAction,
    policy: &ModuleTrustPolicy,
    now: DateTime<Utc>,
    offline: bool,
) -> ModuleEligibility {
    let snapshot = verified_snapshot.snapshot();
    let verification = verification_evaluation(snapshot, requested_cell);
    let lifecycle = lifecycle_state(snapshot, release_digest);
    let age = u64::try_from(
        now.signed_duration_since(snapshot.generated_at)
            .num_seconds(),
    )
    .unwrap_or(0);
    let mutation = !matches!(action, CatalogAction::Discover | CatalogAction::Continue);
    let fresh = age <= policy.maximum_mutation_age_seconds;
    let stale_allowed = offline && policy.stale_snapshot == StaleSnapshotPolicy::AllowOffline;
    let mut state = ModuleEligibilityState::Eligible;
    let mut reasons = Vec::new();

    apply_lifecycle_eligibility(&lifecycle, action, policy, &mut state, &mut reasons);

    if mutation && !fresh && !stale_allowed {
        state = ModuleEligibilityState::Blocked;
        reasons.push("snapshot_stale".to_owned());
    } else if mutation && !fresh {
        warn(&mut state);
        reasons.push("snapshot_stale_offline_override".to_owned());
    }

    if mutation {
        match declared_compatibility {
            DeclaredCompatibilityState::Compatible => {}
            DeclaredCompatibilityState::Incompatible
                if policy.compatibility == CompatibilityPolicy::AllowOverride =>
            {
                warn(&mut state);
                reasons.push("compatibility_override".to_owned());
            }
            DeclaredCompatibilityState::Incompatible => {
                state = ModuleEligibilityState::Blocked;
                reasons.push("declared_incompatible".to_owned());
            }
            DeclaredCompatibilityState::Unknown
                if policy.verification == VerificationRequirement::RequireVerified =>
            {
                state = ModuleEligibilityState::Blocked;
                reasons.push("declared_compatibility_unknown".to_owned());
            }
            DeclaredCompatibilityState::Unknown => {
                warn(&mut state);
                reasons.push("declared_compatibility_unknown".to_owned());
            }
        }
        match verification.state {
            VerificationState::Verified => {}
            VerificationState::Failed
                if policy.compatibility == CompatibilityPolicy::AllowOverride =>
            {
                warn(&mut state);
                reasons.push("verification_override".to_owned());
            }
            VerificationState::Failed => {
                state = ModuleEligibilityState::Blocked;
                reasons.push("verification_failed".to_owned());
            }
            VerificationState::Unknown
                if policy.verification == VerificationRequirement::RequireVerified =>
            {
                state = ModuleEligibilityState::Blocked;
                reasons.push(verification.reason_code.clone());
            }
            VerificationState::Unknown => {
                warn(&mut state);
                reasons.push(verification.reason_code.clone());
            }
        }
    }
    reasons.sort();
    reasons.dedup();
    ModuleEligibility {
        action,
        state,
        declared_compatibility,
        verification,
        lifecycle,
        snapshot_age_seconds: age,
        snapshot_fresh: fresh,
        reason_codes: reasons,
    }
}

fn apply_lifecycle_eligibility(
    lifecycle: &ModuleLifecycleState,
    action: CatalogAction,
    policy: &ModuleTrustPolicy,
    state: &mut ModuleEligibilityState,
    reasons: &mut Vec<String>,
) {
    if lifecycle.security_blocked {
        reasons.push("security_blocked".to_owned());
        *state = match action {
            CatalogAction::Restore
                if policy.security_restore == SecurityRestorePolicy::BreakGlass =>
            {
                ModuleEligibilityState::EligibleWithWarning
            }
            CatalogAction::Restore => ModuleEligibilityState::BreakGlassOnly,
            CatalogAction::Install | CatalogAction::Update => ModuleEligibilityState::Blocked,
            CatalogAction::Discover | CatalogAction::Continue => {
                ModuleEligibilityState::EligibleWithWarning
            }
        };
        return;
    }
    if lifecycle.yanked {
        reasons.push("yanked".to_owned());
        match action {
            CatalogAction::Discover | CatalogAction::Install | CatalogAction::Update => {
                *state = ModuleEligibilityState::Blocked;
            }
            CatalogAction::Restore | CatalogAction::Continue => warn(state),
        }
    }
    if lifecycle.deprecated {
        reasons.push("deprecated".to_owned());
        warn(state);
    }
}

fn warn(state: &mut ModuleEligibilityState) {
    if *state == ModuleEligibilityState::Eligible {
        *state = ModuleEligibilityState::EligibleWithWarning;
    }
}

fn validate_publishers(snapshot: &CatalogSnapshot, issues: &mut Vec<CatalogContractIssue>) {
    let mut publisher_ids = BTreeSet::new();
    let mut namespaces = BTreeSet::new();
    if !snapshot
        .publishers
        .windows(2)
        .all(|pair| pair[0].publisher_id < pair[1].publisher_id)
    {
        push_issue(
            issues,
            "$.snapshot.publishers",
            "Publishers must be sorted by Publisher identity",
        );
    }
    for (index, publisher) in snapshot.publishers.iter().enumerate() {
        if !publisher_ids.insert(publisher.publisher_id.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.publishers[{index}].publisher_id"),
                "duplicate Publisher identity",
            );
        }
        if publisher.namespaces.is_empty() || !sorted_unique(&publisher.namespaces) {
            push_issue(
                issues,
                format!("$.snapshot.publishers[{index}].namespaces"),
                "Publisher namespaces must be sorted and unique",
            );
        }
        for namespace in &publisher.namespaces {
            if !valid_namespace(namespace) || !namespaces.insert(namespace.as_str()) {
                push_issue(
                    issues,
                    format!("$.snapshot.publishers[{index}].namespaces"),
                    "Publisher namespace must be valid and uniquely owned",
                );
            }
        }
        let mut actors = BTreeSet::new();
        actors.insert(publisher.owner.user_id);
        for maintainer in &publisher.maintainers {
            if !actors.insert(maintainer.user_id) {
                push_issue(
                    issues,
                    format!("$.snapshot.publishers[{index}].maintainers"),
                    "Publisher actors must have unique numeric GitHub identities",
                );
            }
        }
        if publisher.publisher_id.trim().is_empty()
            || publisher.repository.trim().is_empty()
            || publisher.publishing_workflow.trim().is_empty()
            || publisher.security_contact.trim().is_empty()
        {
            push_issue(
                issues,
                format!("$.snapshot.publishers[{index}]"),
                "Publisher trust fields must be non-empty",
            );
        }
    }
}

fn validate_metadata(snapshot: &CatalogSnapshot, issues: &mut Vec<CatalogContractIssue>) {
    let release_module_ids = snapshot
        .releases
        .iter()
        .map(|release| release.module_id.as_str())
        .collect::<BTreeSet<_>>();
    if !snapshot
        .metadata
        .windows(2)
        .all(|pair| pair[0].module_id < pair[1].module_id)
    {
        push_issue(
            issues,
            "$.snapshot.metadata",
            "Module metadata must be sorted by Module identity",
        );
    }
    let mut module_ids = BTreeSet::new();
    for (index, metadata) in snapshot.metadata.iter().enumerate() {
        if !module_ids.insert(metadata.module_id.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.metadata[{index}].module_id"),
                "duplicate Module metadata",
            );
        }
        if !release_module_ids.contains(metadata.module_id.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.metadata[{index}].module_id"),
                "Module metadata references no indexed release",
            );
        }
        if metadata.description.trim().is_empty() || metadata.source_revision.trim().is_empty() {
            push_issue(
                issues,
                format!("$.snapshot.metadata[{index}]"),
                "Module metadata description and source revision must be non-empty",
            );
        }
        if !sorted_unique(&metadata.categories) || !sorted_unique(&metadata.documentation) {
            push_issue(
                issues,
                format!("$.snapshot.metadata[{index}]"),
                "Module metadata lists must be sorted and unique",
            );
        }
    }
}

fn validate_releases(
    snapshot: &CatalogSnapshot,
    releases: &[ModuleRelease],
    issues: &mut Vec<CatalogContractIssue>,
) {
    let release_by_digest = releases
        .iter()
        .filter_map(|release| digest_json(release).ok().map(|digest| (digest, release)))
        .collect::<BTreeMap<_, _>>();
    let publishers = snapshot
        .publishers
        .iter()
        .map(|publisher| (publisher.publisher_id.as_str(), publisher))
        .collect::<BTreeMap<_, _>>();
    let mut identities = BTreeSet::new();
    let mut digests = BTreeSet::new();
    if !snapshot
        .releases
        .windows(2)
        .all(|pair| (&pair[0].module_id, &pair[0].version) < (&pair[1].module_id, &pair[1].version))
    {
        push_issue(
            issues,
            "$.snapshot.releases",
            "Catalog releases must be sorted by Module identity and version",
        );
    }
    for (index, record) in snapshot.releases.iter().enumerate() {
        validate_digest(
            &record.release_digest,
            &format!("$.snapshot.releases[{index}].release_digest"),
            issues,
        );
        if record.release.digest != record.release_digest {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}].release.digest"),
                "release locator must bind the indexed digest",
            );
        }
        if !identities.insert((record.module_id.as_str(), record.version.as_str())) {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}]"),
                "Module release identity is immutable and unique",
            );
        }
        if !digests.insert(record.release_digest.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}].release_digest"),
                "duplicate release digest",
            );
        }
        let Some(publisher) = publishers.get(record.publisher_id.as_str()) else {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}].publisher_id"),
                "unknown Publisher",
            );
            continue;
        };
        let namespace = record
            .module_id
            .split_once('/')
            .map(|(namespace, _)| namespace);
        if !namespace
            .is_some_and(|namespace| publisher.namespaces.iter().any(|owned| owned == namespace))
        {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}].module_id"),
                "Module namespace is not owned by the Publisher",
            );
        }
        let Some(release) = release_by_digest.get(&record.release_digest) else {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}].release_digest"),
                "referenced Module Release bytes are unavailable",
            );
            continue;
        };
        if release.module_id != record.module_id
            || release.version != record.version
            || ModuleDeliveryKind::from(&release.delivery) != record.delivery_kind
        {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}]"),
                "Catalog projection does not match referenced Module Release bytes",
            );
        }
    }
}

#[allow(clippy::too_many_lines)]
fn validate_provenance(
    snapshot: &CatalogSnapshot,
    releases: &[ModuleRelease],
    issues: &mut Vec<CatalogContractIssue>,
) {
    let release_by_digest = releases
        .iter()
        .filter_map(|release| digest_json(release).ok().map(|digest| (digest, release)))
        .collect::<BTreeMap<_, _>>();
    let catalog_releases = snapshot
        .releases
        .iter()
        .map(|release| (release.release_digest.as_str(), release))
        .collect::<BTreeMap<_, _>>();
    let publishers = snapshot
        .publishers
        .iter()
        .map(|publisher| (publisher.publisher_id.as_str(), publisher))
        .collect::<BTreeMap<_, _>>();
    let receipt_digests = snapshot
        .linked_provenance
        .iter()
        .map(digest_json)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    let receipt_by_digest = receipt_digests
        .iter()
        .zip(&snapshot.linked_provenance)
        .map(|(digest, receipt)| (digest.as_str(), receipt))
        .collect::<BTreeMap<_, _>>();
    let superseded_receipt_digests = snapshot
        .linked_provenance
        .iter()
        .filter_map(|receipt| receipt.supersedes_receipt_digest.as_deref())
        .collect::<BTreeSet<_>>();
    let mut receipt_ids = BTreeSet::new();
    let mut unique_receipt_digests = BTreeSet::new();
    if !snapshot.linked_provenance.windows(2).all(|pair| {
        (
            &pair[0].module_release_digest,
            pair[0].issued_at,
            &pair[0].receipt_id,
        ) < (
            &pair[1].module_release_digest,
            pair[1].issued_at,
            &pair[1].receipt_id,
        )
    }) {
        push_issue(
            issues,
            "$.snapshot.linked_provenance",
            "Linked provenance receipts must be sorted by release, issue time, and receipt identity",
        );
    }
    for (index, receipt) in snapshot.linked_provenance.iter().enumerate() {
        if receipt.protocol != LINKED_PROVENANCE_RECEIPT_PROTOCOL {
            push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}].protocol"),
                "unsupported linked provenance protocol",
            );
        }
        validate_digest(
            &receipt.module_release_digest,
            &format!("$.snapshot.linked_provenance[{index}].module_release_digest"),
            issues,
        );
        if !receipt_ids.insert(receipt.receipt_id.as_str())
            || receipt_digests
                .get(index)
                .is_some_and(|digest| !unique_receipt_digests.insert(digest.as_str()))
        {
            push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}].receipt_id"),
                "duplicate Linked provenance receipt identity or bytes",
            );
        }
        if let Some(superseded_digest) = &receipt.supersedes_receipt_digest {
            validate_digest(
                superseded_digest,
                &format!("$.snapshot.linked_provenance[{index}].supersedes_receipt_digest"),
                issues,
            );
            match receipt_by_digest.get(superseded_digest.as_str()) {
                Some(superseded)
                    if superseded.module_release_digest == receipt.module_release_digest
                        && superseded.issued_at < receipt.issued_at => {}
                Some(_) => push_issue(
                    issues,
                    format!("$.snapshot.linked_provenance[{index}].supersedes_receipt_digest"),
                    "superseded provenance must be an earlier receipt for the same release",
                ),
                None => push_issue(
                    issues,
                    format!("$.snapshot.linked_provenance[{index}].supersedes_receipt_digest"),
                    "superseded provenance receipt is absent from the snapshot",
                ),
            }
        }
        let active = receipt_digests
            .get(index)
            .is_some_and(|digest| !superseded_receipt_digests.contains(digest.as_str()));
        if !active {
            continue;
        }
        if let Some(catalog_release) = catalog_releases.get(receipt.module_release_digest.as_str())
        {
            if catalog_release.publisher_id != receipt.publisher_id {
                push_issue(
                    issues,
                    format!("$.snapshot.linked_provenance[{index}].publisher_id"),
                    "provenance Publisher does not own the indexed release",
                );
            }
            if let Some(publisher) = publishers.get(receipt.publisher_id.as_str())
                && (receipt.trusted_publishing.repository != publisher.repository
                    || receipt.trusted_publishing.repository_id != publisher.github_repository_id
                    || receipt.trusted_publishing.workflow != publisher.publishing_workflow)
            {
                push_issue(
                    issues,
                    format!("$.snapshot.linked_provenance[{index}].trusted_publishing"),
                    "trusted publishing identity does not match the pinned Publisher repository",
                );
            }
        }
        validate_digest(
            &receipt.archive_checksum,
            &format!("$.snapshot.linked_provenance[{index}].archive_checksum"),
            issues,
        );
        let Some(release) = release_by_digest.get(&receipt.module_release_digest) else {
            continue;
        };
        match &release.delivery {
            ModuleDelivery::Linked(linked)
                if linked.package == receipt.package
                    && linked.crate_version == receipt.crate_version
                    && linked.archive_checksum == receipt.archive_checksum => {}
            _ => push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}]"),
                "provenance does not match the exact Linked delivery",
            ),
        }
        if receipt.artifact_attestation.attestation.digest != receipt.archive_checksum {
            push_issue(
                issues,
                format!(
                    "$.snapshot.linked_provenance[{index}].artifact_attestation.attestation.digest"
                ),
                "attestation must bind the exact crates.io archive checksum",
            );
        }
        if receipt.artifact_attestation.repository != receipt.trusted_publishing.repository
            || receipt.artifact_attestation.repository_id
                != receipt.trusted_publishing.repository_id
            || receipt.artifact_attestation.workflow != receipt.trusted_publishing.workflow
            || receipt.artifact_attestation.run_id != receipt.trusted_publishing.run_id
            || receipt.artifact_attestation.commit_sha != receipt.trusted_publishing.commit_sha
            || receipt.artifact_attestation.oidc_issuer != receipt.trusted_publishing.oidc_issuer
            || receipt.artifact_attestation.runner_environment
                != receipt.trusted_publishing.runner_environment
            || receipt.artifact_attestation.attestation.issuer
                != receipt.artifact_attestation.oidc_issuer
        {
            push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}].artifact_attestation"),
                "artifact attestation identity must match trusted publishing identity",
            );
        }
        if receipt.trusted_publishing.repository_id == 0
            || receipt.trusted_publishing.run_id == 0
            || receipt.artifact_attestation.repository_id == 0
            || receipt.artifact_attestation.run_id == 0
            || receipt.archive_size == 0
        {
            push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}]"),
                "provenance numeric identities and archive size must be non-zero",
            );
        }
        if receipt.trusted_publishing.runner_environment != "github-hosted" {
            push_issue(
                issues,
                format!(
                    "$.snapshot.linked_provenance[{index}].trusted_publishing.runner_environment"
                ),
                "Linked trusted publishing must use a GitHub-hosted runner",
            );
        }
        for (field, digest) in [
            (
                "trusted_publishing.runner_image_digest",
                receipt.trusted_publishing.runner_image_digest.as_str(),
            ),
            (
                "clean_build.starter_digest",
                receipt.clean_build.starter_digest.as_str(),
            ),
            (
                "clean_build.application_lock_digest",
                receipt.clean_build.application_lock_digest.as_str(),
            ),
            (
                "clean_build.runner_image_digest",
                receipt.clean_build.runner_image_digest.as_str(),
            ),
        ] {
            validate_digest(
                digest,
                &format!("$.snapshot.linked_provenance[{index}].{field}"),
                issues,
            );
        }
        if receipt.clean_build.commands.is_empty()
            || receipt.clean_build.checks.is_empty()
            || receipt
                .clean_build
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
        {
            push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}].clean_build"),
                "clean build provenance requires exact non-empty commands and checks",
            );
        }
        if !receipt.clean_build.checks.iter().all(|check| {
            matches!(
                check.outcome,
                VerificationCheckOutcome::Passed | VerificationCheckOutcome::NotApplicable
            )
        }) {
            push_issue(
                issues,
                format!("$.snapshot.linked_provenance[{index}].clean_build.checks"),
                "clean build provenance contains a failed check",
            );
        }
    }
    for (index, release) in snapshot.releases.iter().enumerate() {
        if release.delivery_kind != ModuleDeliveryKind::Linked {
            continue;
        }
        let active_receipts = receipt_digests
            .iter()
            .zip(&snapshot.linked_provenance)
            .filter(|(digest, receipt)| {
                receipt.module_release_digest == release.release_digest
                    && !superseded_receipt_digests.contains(digest.as_str())
            })
            .count();
        if active_receipts != 1 {
            push_issue(
                issues,
                format!("$.snapshot.releases[{index}].release_digest"),
                "Linked release requires exactly one active provenance receipt",
            );
        }
    }
}

#[allow(clippy::too_many_lines)]
fn validate_receipts(
    snapshot: &CatalogSnapshot,
    releases: &[ModuleRelease],
    issues: &mut Vec<CatalogContractIssue>,
) {
    let profile = &snapshot.verification_profile;
    let release_digests = snapshot
        .releases
        .iter()
        .map(|record| record.release_digest.as_str())
        .collect::<BTreeSet<_>>();
    let release_by_digest = releases
        .iter()
        .filter_map(|release| digest_json(release).ok().map(|digest| (digest, release)))
        .collect::<BTreeMap<_, _>>();
    let catalog_release_by_digest = snapshot
        .releases
        .iter()
        .map(|release| (release.release_digest.as_str(), release))
        .collect::<BTreeMap<_, _>>();
    let mut receipt_digests = BTreeSet::new();
    if !snapshot
        .verification_receipts
        .windows(2)
        .all(|pair| pair[0].receipt_digest < pair[1].receipt_digest)
    {
        push_issue(
            issues,
            "$.snapshot.verification_receipts",
            "verification receipts must be sorted by receipt digest",
        );
    }
    for (index, accepted) in snapshot.verification_receipts.iter().enumerate() {
        let receipt = &accepted.receipt;
        if receipt.protocol != VERIFICATION_RECEIPT_PROTOCOL {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.protocol"),
                "unsupported verification receipt protocol",
            );
        }
        match digest_json(receipt) {
            Ok(digest) if digest == accepted.receipt_digest => {}
            _ => push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt_digest"),
                "verification receipt digest mismatch",
            ),
        }
        validate_digest(
            &accepted.receipt_digest,
            &format!("$.snapshot.verification_receipts[{index}].receipt_digest"),
            issues,
        );
        if !receipt_digests.insert(accepted.receipt_digest.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt_digest"),
                "duplicate verification receipt",
            );
        }
        if accepted.attestation.digest != accepted.receipt_digest {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].attestation.digest"),
                "receipt attestation must bind exact receipt bytes",
            );
        }
        if !release_digests.contains(receipt.cell.module_release_digest.as_str()) {
            push_issue(
                issues,
                format!(
                    "$.snapshot.verification_receipts[{index}].receipt.cell.module_release_digest"
                ),
                "receipt references an unknown Module Release",
            );
        }
        if let Some(release) = release_by_digest.get(&receipt.cell.module_release_digest)
            && release.manifest_digest != receipt.manifest_digest
        {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.manifest_digest"),
                "receipt manifest digest does not match the exact Module Release",
            );
        }
        if let Some(catalog_release) =
            catalog_release_by_digest.get(receipt.cell.module_release_digest.as_str())
            && catalog_release.publisher_id != receipt.publisher_id
        {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.publisher_id"),
                "receipt Publisher does not own the verified release",
            );
        }
        if receipt.completed_at < receipt.started_at {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.completed_at"),
                "receipt completion precedes start",
            );
        }
        for (field, digest) in [
            ("manifest_digest", receipt.manifest_digest.as_str()),
            (
                "catalog_snapshot_digest",
                receipt.catalog_snapshot_digest.as_str(),
            ),
            (
                "verification_profile_digest",
                receipt.verification_profile_digest.as_str(),
            ),
            (
                "toolchain_evidence.application_lock_digest",
                receipt.toolchain_evidence.application_lock_digest.as_str(),
            ),
            (
                "toolchain_evidence.cargo_lock_digest",
                receipt.toolchain_evidence.cargo_lock_digest.as_str(),
            ),
            (
                "toolchain_evidence.config_input_digest",
                receipt.toolchain_evidence.config_input_digest.as_str(),
            ),
            (
                "toolchain_evidence.migration_history_digest",
                receipt.toolchain_evidence.migration_history_digest.as_str(),
            ),
        ] {
            validate_digest(
                digest,
                &format!("$.snapshot.verification_receipts[{index}].receipt.{field}"),
                issues,
            );
        }
        if receipt.commands.is_empty()
            || receipt.checks.is_empty()
            || receipt
                .commands
                .iter()
                .any(|command| command.trim().is_empty())
        {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt"),
                "verification receipt requires exact non-empty commands and checks",
            );
        }
        let required = profile
            .required_checks
            .get(&receipt.cell.operation)
            .cloned()
            .unwrap_or_default();
        let checks = receipt
            .checks
            .iter()
            .map(|check| check.check_id.as_str())
            .collect::<BTreeSet<_>>();
        if required
            .iter()
            .any(|check| !checks.contains(check.as_str()))
        {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.checks"),
                "receipt does not cover every required profile check",
            );
        }
        let duplicate_check_ids = receipt
            .checks
            .iter()
            .map(|check| check.check_id.as_str())
            .collect::<Vec<_>>();
        if !sorted_unique(&duplicate_check_ids) {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.checks"),
                "receipt checks must be sorted and unique",
            );
        }
        if !profile
            .accepted_verifier_repository_ids
            .contains(&receipt.verifier.repository_id)
            || !profile
                .accepted_verifier_workflows
                .contains(&receipt.verifier.workflow)
        {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.verifier"),
                "receipt verifier is not accepted by the profile",
            );
        }
        if receipt.outcome == VerificationOutcome::Passed
            && receipt.checks.iter().any(|check| {
                check.outcome == VerificationCheckOutcome::Failed
                    || (required.contains(&check.check_id)
                        && check.outcome != VerificationCheckOutcome::Passed)
            })
        {
            push_issue(
                issues,
                format!("$.snapshot.verification_receipts[{index}].receipt.outcome"),
                "passed receipt contains a failed check",
            );
        }
        validate_cell(
            &receipt.cell,
            &format!("$.snapshot.verification_receipts[{index}].receipt.cell"),
            issues,
        );
    }
    for (index, revocation) in snapshot.verification_revocations.iter().enumerate() {
        validate_digest(
            &revocation.receipt_digest,
            &format!("$.snapshot.verification_revocations[{index}].receipt_digest"),
            issues,
        );
        if !receipt_digests.contains(revocation.receipt_digest.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.verification_revocations[{index}].receipt_digest"),
                "revocation references an unknown receipt",
            );
        }
    }
}

fn validate_lifecycle(snapshot: &CatalogSnapshot, issues: &mut Vec<CatalogContractIssue>) {
    let releases = snapshot
        .releases
        .iter()
        .map(|record| record.release_digest.as_str())
        .collect::<BTreeSet<_>>();
    let mut identities = BTreeSet::new();
    if !snapshot.lifecycle.windows(2).all(|pair| {
        (
            &pair[0].release_digest,
            pair[0].effective_at,
            pair[0].sequence,
        ) < (
            &pair[1].release_digest,
            pair[1].effective_at,
            pair[1].sequence,
        )
    }) {
        push_issue(
            issues,
            "$.snapshot.lifecycle",
            "lifecycle records must be sorted by release, time, and sequence",
        );
    }
    for (index, record) in snapshot.lifecycle.iter().enumerate() {
        if !releases.contains(record.release_digest.as_str()) {
            push_issue(
                issues,
                format!("$.snapshot.lifecycle[{index}].release_digest"),
                "lifecycle record references an unknown release",
            );
        }
        if record.reason_code.trim().is_empty()
            || record.evidence_reference.trim().is_empty()
            || record.source_revision.trim().is_empty()
        {
            push_issue(
                issues,
                format!("$.snapshot.lifecycle[{index}]"),
                "lifecycle evidence fields must be non-empty",
            );
        }
        if !identities.insert((
            record.release_digest.as_str(),
            record.facet,
            record.sequence,
        )) {
            push_issue(
                issues,
                format!("$.snapshot.lifecycle[{index}].sequence"),
                "lifecycle sequence must be unique per release and facet",
            );
        }
    }
}

fn validate_cell(
    cell: &ModuleVerificationCell,
    path: &str,
    issues: &mut Vec<CatalogContractIssue>,
) {
    for (field, digest) in [
        ("module_release_digest", cell.module_release_digest.as_str()),
        ("starter_digest", cell.starter_digest.as_str()),
        ("delivery_digest", cell.delivery_digest.as_str()),
        ("runner_image_digest", cell.runner_image_digest.as_str()),
    ] {
        validate_digest(digest, &format!("{path}.{field}"), issues);
    }
    for (field, digest) in [
        (
            "source_release_digest",
            cell.source_release_digest.as_deref(),
        ),
        (
            "console_artifact_digest",
            cell.console_artifact_digest.as_deref(),
        ),
        ("console_lock_digest", cell.console_lock_digest.as_deref()),
    ] {
        if let Some(digest) = digest {
            validate_digest(digest, &format!("{path}.{field}"), issues);
        }
    }
    for (index, digest) in cell.protocol_digests.iter().enumerate() {
        validate_digest(digest, &format!("{path}.protocol_digests[{index}]"), issues);
    }
    if cell.operation == VerificationOperation::Upgrade && cell.source_release_digest.is_none() {
        push_issue(
            issues,
            format!("{path}.source_release_digest"),
            "upgrade verification requires an exact source release",
        );
    }
    if cell.operation != VerificationOperation::Upgrade && cell.source_release_digest.is_some() {
        push_issue(
            issues,
            format!("{path}.source_release_digest"),
            "only upgrade verification accepts a source release",
        );
    }
    if !sorted_unique(&cell.features) || !sorted_unique(&cell.protocol_digests) {
        push_issue(
            issues,
            path,
            "verification cell set dimensions must be sorted and unique",
        );
    }
}

fn validate_verification_profile(
    profile: &VerificationProfile,
    issues: &mut Vec<CatalogContractIssue>,
) {
    if profile.profile_id.trim().is_empty() || profile.policy_revision.trim().is_empty() {
        push_issue(
            issues,
            "$.snapshot.verification_profile",
            "verification profile identity fields must be non-empty",
        );
    }
    if !sorted_unique(&profile.accepted_verifier_repository_ids)
        || !sorted_unique(&profile.accepted_verifier_workflows)
        || profile.accepted_verifier_repository_ids.is_empty()
        || profile.accepted_verifier_workflows.is_empty()
    {
        push_issue(
            issues,
            "$.snapshot.verification_profile",
            "accepted verifier identities must be non-empty, sorted, and unique",
        );
    }
    for (operation, checks) in &profile.required_checks {
        if checks.is_empty() || !sorted_unique(checks) {
            push_issue(
                issues,
                format!("$.snapshot.verification_profile.required_checks.{operation:?}"),
                "required checks must be non-empty, sorted, and unique",
            );
        }
    }
}

fn valid_namespace(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn validate_digest(value: &str, path: &str, issues: &mut Vec<CatalogContractIssue>) {
    let valid = value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    });
    if !valid {
        push_issue(
            issues,
            path,
            "digest must be sha256 followed by 64 lowercase hexadecimal characters",
        );
    }
}

fn push_issue(
    issues: &mut Vec<CatalogContractIssue>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(CatalogContractIssue {
        path: path.into(),
        message: message.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LinkedModuleDelivery, ModuleManifest};
    use chrono::TimeZone as _;

    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    fn identity(login: &str, user_id: u64) -> GitHubIdentity {
        GitHubIdentity {
            login: login.to_owned(),
            user_id,
        }
    }

    fn publisher() -> PublisherRecord {
        PublisherRecord {
            publisher_id: "publisher-acme".to_owned(),
            namespaces: vec!["acme".to_owned()],
            owner: identity("owner", 1),
            maintainers: vec![identity("maintainer", 2)],
            github_owner_id: 10,
            github_repository_id: 11,
            repository: "acme/modules".to_owned(),
            publishing_workflow: ".github/workflows/publish.yml".to_owned(),
            security_contact: "security@acme.test".to_owned(),
            status: PublisherStatus::Active,
            source_revision: "old".to_owned(),
        }
    }

    fn approval(login: &str, user_id: u64, revision: &str) -> CatalogApproval {
        CatalogApproval {
            actor: identity(login, user_id),
            source_revision: revision.to_owned(),
            approved_at: Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
        }
    }

    fn profile() -> VerificationProfile {
        VerificationProfile {
            protocol: VERIFICATION_PROFILE_PROTOCOL.to_owned(),
            profile_id: "default".to_owned(),
            policy_revision: "policy-1".to_owned(),
            required_checks: BTreeMap::from([(
                VerificationOperation::FreshInstall,
                vec!["build".to_owned()],
            )]),
            accepted_verifier_repository_ids: vec![100],
            accepted_verifier_workflows: vec!["verify.yml".to_owned()],
        }
    }

    fn cell() -> ModuleVerificationCell {
        ModuleVerificationCell {
            module_release_digest: digest('a'),
            operation: VerificationOperation::FreshInstall,
            source_release_digest: None,
            lenso_version: "0.3.16".to_owned(),
            host_version: "1.0.0".to_owned(),
            cli_version: "0.2.13".to_owned(),
            starter_digest: digest('b'),
            management_engine_version: "1.0.0".to_owned(),
            delivery_digest: digest('c'),
            features: vec!["postgres".to_owned()],
            target: "x86_64-unknown-linux-gnu".to_owned(),
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            runner_image_digest: digest('d'),
            rust_version: "1.88.0".to_owned(),
            cargo_version: "1.88.0".to_owned(),
            store_engine: "postgres".to_owned(),
            store_version: "17".to_owned(),
            protocol_digests: vec![digest('e')],
            console_artifact_digest: None,
            console_host_api_version: None,
            node_version: None,
            package_manager_version: None,
            console_lock_digest: None,
        }
    }

    fn snapshot() -> CatalogSnapshot {
        let profile = profile();
        CatalogSnapshot {
            protocol: CATALOG_SNAPSHOT_PROTOCOL.to_owned(),
            source_revision: "catalog-commit".to_owned(),
            generated_at: Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
            previous_snapshot_digest: None,
            verification_profile_digest: digest_json(&profile).unwrap(),
            verification_profile: profile,
            publishers: vec![publisher()],
            metadata: Vec::new(),
            releases: Vec::new(),
            lifecycle: Vec::new(),
            linked_provenance: Vec::new(),
            verification_receipts: Vec::new(),
            verification_revocations: Vec::new(),
        }
    }

    fn verified(snapshot: &CatalogSnapshot) -> VerifiedCatalogSnapshot<'_> {
        VerifiedCatalogSnapshot {
            snapshot,
            snapshot_digest: "test-only",
        }
    }

    fn linked_release() -> ModuleRelease {
        ModuleRelease::new(
            "acme/support-ticket",
            "1.2.3",
            ModuleManifest::builder("acme/support-ticket").build(),
            ModuleDelivery::Linked(LinkedModuleDelivery {
                package: "acme-support-ticket".to_owned(),
                crate_version: "1.2.3".to_owned(),
                archive_checksum: digest('a'),
                default_features: false,
                features: vec!["postgres".to_owned()],
                binding: "support_ticket".to_owned(),
                attestations: Vec::new(),
                migrations: Vec::new(),
            }),
        )
        .unwrap()
    }

    fn linked_provenance(module_release_digest: String) -> LinkedProvenanceReceipt {
        let builder = VerifierIdentity {
            repository: "lenso/catalog".to_owned(),
            repository_id: 100,
            workflow: "verify.yml".to_owned(),
            run_id: 201,
            commit_sha: "builder-commit".to_owned(),
            oidc_issuer: "https://token.actions.githubusercontent.com".to_owned(),
            signer: "catalog-verifier".to_owned(),
        };
        let attestation = AttestationReference {
            locator: "oci://attestations/crate".to_owned(),
            digest: digest('a'),
            issuer: "https://token.actions.githubusercontent.com".to_owned(),
            signer: "acme-publisher".to_owned(),
        };
        LinkedProvenanceReceipt {
            protocol: LINKED_PROVENANCE_RECEIPT_PROTOCOL.to_owned(),
            receipt_id: "linked-provenance-1".to_owned(),
            issued_at: Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
            supersedes_receipt_digest: None,
            publisher_id: "publisher-acme".to_owned(),
            module_release_digest,
            package: "acme-support-ticket".to_owned(),
            crate_version: "1.2.3".to_owned(),
            archive_size: 42,
            archive_checksum: digest('a'),
            trusted_publishing: TrustedPublishingEvidence {
                repository: "acme/modules".to_owned(),
                repository_id: 11,
                workflow: ".github/workflows/publish.yml".to_owned(),
                run_id: 300,
                commit_sha: "release-commit".to_owned(),
                oidc_issuer: "https://token.actions.githubusercontent.com".to_owned(),
                runner_environment: "github-hosted".to_owned(),
                runner_image_digest: digest('b'),
            },
            artifact_attestation: ArtifactAttestationEvidence {
                attestation,
                repository: "acme/modules".to_owned(),
                repository_id: 11,
                workflow: ".github/workflows/publish.yml".to_owned(),
                run_id: 300,
                commit_sha: "release-commit".to_owned(),
                oidc_issuer: "https://token.actions.githubusercontent.com".to_owned(),
                runner_environment: "github-hosted".to_owned(),
            },
            clean_build: CleanBuildEvidence {
                lenso_version: "0.3.16".to_owned(),
                cli_version: "0.2.13".to_owned(),
                starter_digest: digest('c'),
                toolchain: "rust-1.88.0".to_owned(),
                application_lock_digest: digest('d'),
                runner_image_digest: digest('e'),
                builder,
                commands: vec!["cargo build --locked".to_owned()],
                checks: vec![VerificationCheck {
                    check_id: "build".to_owned(),
                    outcome: VerificationCheckOutcome::Passed,
                    duration_ms: 1,
                    evidence: Vec::new(),
                }],
            },
        }
    }

    fn receipt(
        outcome: VerificationOutcome,
        receipt_digest: String,
    ) -> AttestedVerificationReceipt {
        let snapshot = snapshot();
        let timestamp = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        AttestedVerificationReceipt {
            receipt: ModuleVerificationReceipt {
                protocol: VERIFICATION_RECEIPT_PROTOCOL.to_owned(),
                receipt_id: format!("receipt-{outcome:?}"),
                publisher_id: "publisher-acme".to_owned(),
                manifest_digest: digest('f'),
                catalog_snapshot_digest: digest('9'),
                verification_profile_digest: snapshot.verification_profile_digest,
                cell: cell(),
                outcome,
                started_at: timestamp,
                completed_at: timestamp,
                issues: Vec::new(),
                toolchain_evidence: VerificationToolchainEvidence {
                    application_lock_digest: digest('1'),
                    cargo_lock_digest: digest('2'),
                    console_lock_digest: None,
                    config_input_digest: digest('3'),
                    migration_history_digest: digest('4'),
                },
                commands: vec!["cargo test --locked".to_owned()],
                checks: vec![VerificationCheck {
                    check_id: "build".to_owned(),
                    outcome: if outcome == VerificationOutcome::Passed {
                        VerificationCheckOutcome::Passed
                    } else {
                        VerificationCheckOutcome::Failed
                    },
                    duration_ms: 1,
                    evidence: Vec::new(),
                }],
                verifier: VerifierIdentity {
                    repository: "lenso/catalog".to_owned(),
                    repository_id: 100,
                    workflow: "verify.yml".to_owned(),
                    run_id: 200,
                    commit_sha: "abc".to_owned(),
                    oidc_issuer: "https://token.actions.githubusercontent.com".to_owned(),
                    signer: "catalog-verifier".to_owned(),
                },
            },
            receipt_digest: receipt_digest.clone(),
            attestation: AttestationReference {
                locator: "oci://receipts/example".to_owned(),
                digest: receipt_digest,
                issuer: "https://token.actions.githubusercontent.com".to_owned(),
                signer: "catalog-verifier".to_owned(),
            },
        }
    }

    #[test]
    fn publisher_changes_require_independent_approval_and_forbid_self_approval() {
        let maintainers = BTreeSet::from([90]);
        let decision = evaluate_publisher_governance(
            &publisher(),
            90,
            &maintainers,
            "new",
            &PublisherGovernanceAction::OrdinaryChange,
            &[approval("catalog", 90, "new")],
            Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap(),
        );

        assert!(!decision.approved);
        assert!(
            decision
                .reason_codes
                .contains(&"self_approval_forbidden".to_owned())
        );
    }

    #[test]
    fn publisher_transfer_requires_both_owners_and_independent_catalog_approval() {
        let maintainers = BTreeSet::from([90]);
        let decision = evaluate_publisher_governance(
            &publisher(),
            1,
            &maintainers,
            "new",
            &PublisherGovernanceAction::NormalTransfer {
                receiving_owner: identity("receiver", 3),
            },
            &[
                approval("owner", 1, "new"),
                approval("receiver", 3, "new"),
                approval("catalog", 90, "new"),
            ],
            Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap(),
        );

        assert!(decision.approved);
    }

    #[test]
    fn recovery_requires_two_catalog_maintainers_and_fourteen_days() {
        let maintainers = BTreeSet::from([90, 91]);
        let action = PublisherGovernanceAction::RecoveryTransfer {
            receiving_owner: identity("receiver", 3),
            waiting_started_at: Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
        };
        let approvals = [
            approval("catalog-a", 90, "new"),
            approval("catalog-b", 91, "new"),
        ];
        let early = evaluate_publisher_governance(
            &publisher(),
            90,
            &maintainers,
            "new",
            &action,
            &approvals,
            Utc.with_ymd_and_hms(2026, 7, 14, 23, 59, 59).unwrap(),
        );
        assert!(!early.approved);
        assert!(
            early
                .reason_codes
                .contains(&"recovery_waiting_period_incomplete".to_owned())
        );

        let approved = evaluate_publisher_governance(
            &publisher(),
            92,
            &maintainers,
            "new",
            &action,
            &approvals,
            Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap(),
        );
        assert!(approved.approved);
    }

    #[test]
    fn verification_requires_an_exact_cell_and_conflicts_are_unknown() {
        let mut catalog = snapshot();
        catalog.verification_receipts = vec![
            receipt(VerificationOutcome::Passed, digest('6')),
            receipt(VerificationOutcome::Failed, digest('7')),
        ];
        let conflict = verification_evaluation(&catalog, &cell());
        assert_eq!(conflict.state, VerificationState::Unknown);
        assert_eq!(conflict.reason_code, "receipt_conflict");

        let mut different = cell();
        different.store_version = "16".to_owned();
        let missing = verification_evaluation(&catalog, &different);
        assert_eq!(missing.state, VerificationState::Unknown);
        assert_eq!(missing.reason_code, "missing_receipt");
    }

    #[test]
    fn mutation_snapshot_admission_requires_cryptographic_and_identity_trust() {
        let snapshot = snapshot();
        let snapshot_digest = digest_json(&snapshot).unwrap();
        let envelope = CatalogSnapshotEnvelope {
            snapshot,
            snapshot_digest: snapshot_digest.clone(),
            attestation: AttestationReference {
                locator: "oci://catalog/snapshot".to_owned(),
                digest: snapshot_digest,
                issuer: "https://token.actions.githubusercontent.com".to_owned(),
                signer: "catalog-publisher".to_owned(),
            },
        };
        let policy = CatalogAttestationTrustPolicy {
            trusted_issuers: vec!["https://token.actions.githubusercontent.com".to_owned()],
            trusted_signers: vec!["catalog-publisher".to_owned()],
        };

        assert!(admit_catalog_snapshot(&envelope, &[], &policy, false).is_err());
        let admitted = admit_catalog_snapshot(&envelope, &[], &policy, true).unwrap();
        assert_eq!(admitted.snapshot_digest(), envelope.snapshot_digest);
    }

    #[test]
    fn linked_provenance_binds_crate_and_publisher_numeric_identity() {
        let release = linked_release();
        let release_digest = digest_json(&release).unwrap();
        let mut snapshot = snapshot();
        snapshot.releases.push(CatalogReleaseRecord {
            publisher_id: "publisher-acme".to_owned(),
            module_id: "acme/support-ticket".to_owned(),
            version: "1.2.3".to_owned(),
            release_digest: release_digest.clone(),
            release: ArtifactReference {
                locator: "oci://catalog/module-releases/acme-support-ticket-1.2.3.json".to_owned(),
                digest: release_digest.clone(),
            },
            delivery_kind: ModuleDeliveryKind::Linked,
        });
        snapshot
            .linked_provenance
            .push(linked_provenance(release_digest));
        let snapshot_digest = digest_json(&snapshot).unwrap();
        let mut envelope = CatalogSnapshotEnvelope {
            snapshot,
            snapshot_digest: snapshot_digest.clone(),
            attestation: AttestationReference {
                locator: "oci://catalog/snapshot".to_owned(),
                digest: snapshot_digest,
                issuer: "https://token.actions.githubusercontent.com".to_owned(),
                signer: "catalog-publisher".to_owned(),
            },
        };

        assert!(validate_catalog_snapshot(&envelope, std::slice::from_ref(&release)).is_empty());
        envelope.snapshot.linked_provenance[0]
            .trusted_publishing
            .repository_id = 999;
        assert!(
            validate_catalog_snapshot(&envelope, &[release])
                .iter()
                .any(|issue| issue.path.ends_with(".trusted_publishing"))
        );
    }

    #[test]
    fn lifecycle_and_snapshot_policy_are_action_specific() {
        let mut catalog = snapshot();
        catalog.lifecycle.push(ModuleLifecycleRecord {
            release_digest: digest('a'),
            facet: ModuleLifecycleFacet::SecurityBlocked,
            change: ModuleLifecycleChange::Set,
            reason_code: "cve".to_owned(),
            evidence_reference: "https://advisories.example/cve".to_owned(),
            actor: identity("security", 99),
            source_revision: "security-change".to_owned(),
            sequence: 1,
            effective_at: Utc.with_ymd_and_hms(2026, 7, 2, 0, 0, 0).unwrap(),
            replacement_module_id: None,
            guidance: None,
            recovery_conditions: Some("operator acknowledgement".to_owned()),
        });
        let mut policy = ModuleTrustPolicy {
            verification: VerificationRequirement::AllowUnknown,
            maximum_mutation_age_seconds: 86_400,
            stale_snapshot: StaleSnapshotPolicy::Reject,
            compatibility: CompatibilityPolicy::Strict,
            security_restore: SecurityRestorePolicy::Block,
        };
        let restore = evaluate_module_eligibility(
            &verified(&catalog),
            &digest('a'),
            &cell(),
            DeclaredCompatibilityState::Compatible,
            CatalogAction::Restore,
            &policy,
            Utc.with_ymd_and_hms(2026, 7, 2, 0, 0, 0).unwrap(),
            false,
        );
        assert_eq!(restore.state, ModuleEligibilityState::BreakGlassOnly);

        policy.security_restore = SecurityRestorePolicy::BreakGlass;
        let restore = evaluate_module_eligibility(
            &verified(&catalog),
            &digest('a'),
            &cell(),
            DeclaredCompatibilityState::Compatible,
            CatalogAction::Restore,
            &policy,
            Utc.with_ymd_and_hms(2026, 7, 2, 0, 0, 0).unwrap(),
            false,
        );
        assert_eq!(restore.state, ModuleEligibilityState::EligibleWithWarning);

        let install = evaluate_module_eligibility(
            &verified(&catalog),
            &digest('a'),
            &cell(),
            DeclaredCompatibilityState::Compatible,
            CatalogAction::Install,
            &policy,
            Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap(),
            false,
        );
        assert_eq!(install.state, ModuleEligibilityState::Blocked);
        assert!(install.reason_codes.contains(&"snapshot_stale".to_owned()));
        assert!(
            install
                .reason_codes
                .contains(&"security_blocked".to_owned())
        );

        let stale_restore = evaluate_module_eligibility(
            &verified(&catalog),
            &digest('a'),
            &cell(),
            DeclaredCompatibilityState::Compatible,
            CatalogAction::Restore,
            &policy,
            Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap(),
            false,
        );
        assert_eq!(stale_restore.state, ModuleEligibilityState::Blocked);
        assert!(
            stale_restore
                .reason_codes
                .contains(&"snapshot_stale".to_owned())
        );
    }

    #[test]
    fn yanked_and_deprecated_facets_follow_the_action_matrix() {
        let mut catalog = snapshot();
        let timestamp = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        for (sequence, facet) in [
            (1, ModuleLifecycleFacet::Deprecated),
            (2, ModuleLifecycleFacet::Yanked),
        ] {
            catalog.lifecycle.push(ModuleLifecycleRecord {
                release_digest: digest('a'),
                facet,
                change: ModuleLifecycleChange::Set,
                reason_code: format!("{facet:?}"),
                evidence_reference: "https://catalog.example/evidence".to_owned(),
                actor: identity("catalog", 90),
                source_revision: "lifecycle-change".to_owned(),
                sequence,
                effective_at: timestamp,
                replacement_module_id: None,
                guidance: None,
                recovery_conditions: None,
            });
        }
        let policy = ModuleTrustPolicy {
            verification: VerificationRequirement::AllowUnknown,
            maximum_mutation_age_seconds: 86_400,
            stale_snapshot: StaleSnapshotPolicy::Reject,
            compatibility: CompatibilityPolicy::Strict,
            security_restore: SecurityRestorePolicy::Block,
        };
        for action in [
            CatalogAction::Discover,
            CatalogAction::Install,
            CatalogAction::Update,
        ] {
            let result = evaluate_module_eligibility(
                &verified(&catalog),
                &digest('a'),
                &cell(),
                DeclaredCompatibilityState::Compatible,
                action,
                &policy,
                timestamp,
                false,
            );
            assert_eq!(result.state, ModuleEligibilityState::Blocked);
        }
        for action in [CatalogAction::Restore, CatalogAction::Continue] {
            let result = evaluate_module_eligibility(
                &verified(&catalog),
                &digest('a'),
                &cell(),
                DeclaredCompatibilityState::Compatible,
                action,
                &policy,
                timestamp,
                false,
            );
            assert_eq!(result.state, ModuleEligibilityState::EligibleWithWarning);
        }
    }
}
