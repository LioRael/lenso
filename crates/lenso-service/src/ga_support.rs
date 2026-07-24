use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;

pub const GA_SUPPORT_MANIFEST_PROTOCOL: &str = "lenso.ga-support-manifest.v1";
pub const GA_SUPPORT_EVALUATION_PROTOCOL: &str = "lenso.ga-support-evaluation.v1";
pub const MANIFEST_MIGRATION_PLAN_PROTOCOL: &str = "lenso.manifest-migration-plan.v1";
pub const MANIFEST_MIGRATION_RECEIPT_PROTOCOL: &str = "lenso.manifest-migration-receipt.v1";
pub const SERVICE_UPGRADE_PLAN_PROTOCOL: &str = "lenso.service-upgrade-plan.v1";
pub const CONTRACT_RETIREMENT_PLAN_PROTOCOL: &str = "lenso.contract-retirement-plan.v1";
pub const CONTRACT_RETIREMENT_RECEIPT_PROTOCOL: &str = "lenso.contract-retirement-receipt.v2";
pub const FAILURE_SCENARIO_EVIDENCE_PROTOCOL: &str = "lenso.failure-scenario-evidence.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SupportDecision {
    Supported,
    Unsupported,
    Unknown,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    Candidate,
    GeneralAvailability,
    Deprecated,
    Unsupported,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    Cli,
    Runtime,
    Contracts,
    Provider,
    Operator,
    RuntimeConsole,
    FirstPartyModule,
    Skill,
}

impl ComponentKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Runtime => "runtime",
            Self::Contracts => "contracts",
            Self::Provider => "provider",
            Self::Operator => "operator",
            Self::RuntimeConsole => "runtime_console",
            Self::FirstPartyModule => "first_party_module",
            Self::Skill => "skill",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ManifestKind {
    Provider,
    Service,
    System,
    Module,
    Backup,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GaComponent {
    pub kind: ComponentKind,
    pub component_id: String,
    pub version: String,
    pub digest: String,
}

impl GaComponent {
    #[must_use]
    pub fn reference(&self) -> String {
        format!(
            "{}:{}@{}",
            self.kind.as_str(),
            self.component_id,
            self.version
        )
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFormat {
    pub kind: ManifestKind,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentationIdentity {
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupportCombinationInput {
    pub combination_id: String,
    pub component_references: Vec<String>,
    pub state_version: String,
    pub status: SupportStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeEdgeInput {
    pub edge_id: String,
    pub source_format: String,
    pub target_format: String,
    pub mixed_version_references: Vec<String>,
    pub rollback_safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GaSupportManifestInput {
    pub status: SupportStatus,
    pub components: Vec<GaComponent>,
    pub manifest_formats: Vec<ManifestFormat>,
    pub state_versions: Vec<String>,
    pub adapter_versions: BTreeMap<String, String>,
    pub documentation: DocumentationIdentity,
    pub combinations: Vec<SupportCombinationInput>,
    pub upgrade_edges: Vec<UpgradeEdgeInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReceiptTrust {
    pub authorities: BTreeMap<String, String>,
    pub public_keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GaSupportManifest {
    pub protocol: String,
    pub manifest_id: String,
    pub manifest_digest: String,
    pub status: SupportStatus,
    pub components: Vec<GaComponent>,
    pub manifest_formats: Vec<ManifestFormat>,
    pub state_versions: Vec<String>,
    pub adapter_versions: BTreeMap<String, String>,
    pub documentation: DocumentationIdentity,
    pub combinations: Vec<SupportCombinationInput>,
    pub upgrade_edges: Vec<UpgradeEdgeInput>,
    #[serde(default)]
    pub evidence_receipt_authorities: BTreeMap<String, String>,
    #[serde(default)]
    pub receipt_authority_public_keys: BTreeMap<String, String>,
}

impl GaSupportManifest {
    #[must_use]
    pub fn into_input(self) -> GaSupportManifestInput {
        GaSupportManifestInput {
            status: self.status,
            components: self.components,
            manifest_formats: self.manifest_formats,
            state_versions: self.state_versions,
            adapter_versions: self.adapter_versions,
            documentation: self.documentation,
            combinations: self.combinations,
            upgrade_edges: self.upgrade_edges,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GaIssueCode {
    ManifestInvalid,
    CombinationUnknown,
    CombinationUnsupported,
    ManifestSourceStale,
    ManifestFormatUnsupported,
    ManifestTargetCollision,
    ManifestIdentityChanged,
    PlanIntegrityInvalid,
    UpgradeUnsupported,
    RetirementActiveConsumer,
    RetirementEvidenceStale,
    RetirementDeprecationIncomplete,
    RetirementReplacementMissing,
    RetirementApprovalInvalid,
    RetirementInputStale,
    FailureUnexpectedOutcome,
    FailureCleanupIncomplete,
}

impl GaIssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestInvalid => "ga_manifest_invalid",
            Self::CombinationUnknown => "ga_combination_unknown",
            Self::CombinationUnsupported => "ga_combination_unsupported",
            Self::ManifestSourceStale => "manifest_source_stale",
            Self::ManifestFormatUnsupported => "manifest_format_unsupported",
            Self::ManifestTargetCollision => "manifest_target_collision",
            Self::ManifestIdentityChanged => "manifest_identity_changed",
            Self::PlanIntegrityInvalid => "ga_plan_integrity_invalid",
            Self::UpgradeUnsupported => "service_upgrade_unsupported",
            Self::RetirementActiveConsumer => "retirement_active_consumer",
            Self::RetirementEvidenceStale => "retirement_evidence_stale",
            Self::RetirementDeprecationIncomplete => "retirement_deprecation_incomplete",
            Self::RetirementReplacementMissing => "retirement_replacement_missing",
            Self::RetirementApprovalInvalid => "retirement_approval_invalid",
            Self::RetirementInputStale => "retirement_input_stale",
            Self::FailureUnexpectedOutcome => "failure_unexpected_outcome",
            Self::FailureCleanupIncomplete => "failure_cleanup_incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GaIssue {
    pub code: GaIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GaSupportEvaluation {
    pub protocol: String,
    pub manifest_id: String,
    pub manifest_digest: String,
    pub decision: SupportDecision,
    pub combination_id: Option<String>,
    pub issues: Vec<GaIssue>,
    pub next_actions: Vec<String>,
}

pub fn assemble_ga_support_manifest(
    input: GaSupportManifestInput,
) -> Result<GaSupportManifest, Vec<GaIssue>> {
    assemble_ga_support_manifest_with_trust(input, EvidenceReceiptTrust::default())
}

pub fn assemble_ga_support_manifest_with_trust(
    mut input: GaSupportManifestInput,
    trust: EvidenceReceiptTrust,
) -> Result<GaSupportManifest, Vec<GaIssue>> {
    input.components.sort();
    input.manifest_formats.sort();
    input.state_versions.sort();
    input.state_versions.dedup();
    input
        .combinations
        .sort_by(|left, right| left.combination_id.cmp(&right.combination_id));
    for combination in &mut input.combinations {
        combination.component_references.sort();
        combination.component_references.dedup();
    }
    input
        .upgrade_edges
        .sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
    for edge in &mut input.upgrade_edges {
        edge.mixed_version_references.sort();
        edge.mixed_version_references.dedup();
    }

    let component_references = input
        .components
        .iter()
        .map(GaComponent::reference)
        .collect::<BTreeSet<_>>();
    let invalid = input.components.is_empty()
        || input.components.iter().any(|component| {
            component.component_id.trim().is_empty()
                || component.version.trim().is_empty()
                || !valid_digest(&component.digest)
        })
        || !valid_digest(&input.documentation.digest)
        || trust
            .authorities
            .values()
            .any(|authority| !trust.public_keys.contains_key(authority))
        || trust
            .public_keys
            .values()
            .any(|key| !key.starts_with("-----BEGIN PUBLIC KEY-----"))
        || input.combinations.iter().any(|combination| {
            combination.component_references.is_empty()
                || combination
                    .component_references
                    .iter()
                    .any(|reference| !component_references.contains(reference))
                || !input.state_versions.contains(&combination.state_version)
        });
    if invalid {
        return Err(vec![issue(
            GaIssueCode::ManifestInvalid,
            "The GA Support Manifest contains an invalid or unbound subject.",
            "Bind every combination to exact components, state formats, and immutable digests.",
            "Correct the manifest source and regenerate its contracts.",
        )]);
    }

    let mut manifest = GaSupportManifest {
        protocol: GA_SUPPORT_MANIFEST_PROTOCOL.into(),
        manifest_id: String::new(),
        manifest_digest: String::new(),
        status: input.status,
        components: input.components,
        manifest_formats: input.manifest_formats,
        state_versions: input.state_versions,
        adapter_versions: input.adapter_versions,
        documentation: input.documentation,
        combinations: input.combinations,
        upgrade_edges: input.upgrade_edges,
        evidence_receipt_authorities: trust.authorities,
        receipt_authority_public_keys: trust.public_keys,
    };
    manifest.manifest_digest = ga_support_manifest_digest(&manifest);
    manifest.manifest_id = format!("ga-support:{}", &manifest.manifest_digest[7..23]);
    Ok(manifest)
}

#[must_use]
pub fn evaluate_ga_support(
    manifest: &GaSupportManifest,
    component_references: &[&str],
    state_version: &str,
) -> GaSupportEvaluation {
    if !ga_support_manifest_integrity_valid(manifest) {
        return GaSupportEvaluation {
            protocol: GA_SUPPORT_EVALUATION_PROTOCOL.into(),
            manifest_id: manifest.manifest_id.clone(),
            manifest_digest: manifest.manifest_digest.clone(),
            decision: SupportDecision::Blocked,
            combination_id: None,
            issues: vec![issue(
                GaIssueCode::ManifestInvalid,
                "The GA Support Manifest content does not match its canonical identity.",
                "Reject modified or unverified support manifests.",
                "Regenerate the manifest from its reviewed source and verify its digest.",
            )],
            next_actions: vec!["Load an integrity-valid GA Support Manifest.".into()],
        };
    }
    let requested = component_references
        .iter()
        .map(|reference| (*reference).to_owned())
        .collect::<BTreeSet<_>>();
    let combination = manifest.combinations.iter().find(|candidate| {
        candidate
            .component_references
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            == requested
            && candidate.state_version == state_version
    });
    match combination {
        Some(combination) if combination.status == SupportStatus::GeneralAvailability => {
            GaSupportEvaluation {
                protocol: GA_SUPPORT_EVALUATION_PROTOCOL.into(),
                manifest_id: manifest.manifest_id.clone(),
                manifest_digest: manifest.manifest_digest.clone(),
                decision: SupportDecision::Supported,
                combination_id: Some(combination.combination_id.clone()),
                issues: Vec::new(),
                next_actions: vec!["Proceed using the exact supported component set.".into()],
            }
        }
        Some(combination) => GaSupportEvaluation {
            protocol: GA_SUPPORT_EVALUATION_PROTOCOL.into(),
            manifest_id: manifest.manifest_id.clone(),
            manifest_digest: manifest.manifest_digest.clone(),
            decision: SupportDecision::Unsupported,
            combination_id: Some(combination.combination_id.clone()),
            issues: vec![issue(
                GaIssueCode::CombinationUnsupported,
                "The exact combination is declared but is not supported for GA.",
                "Select a General Availability combination from the manifest.",
                "Inspect the declared support status and migration guidance.",
            )],
            next_actions: vec!["Select a GA combination from the support manifest.".into()],
        },
        None => GaSupportEvaluation {
            protocol: GA_SUPPORT_EVALUATION_PROTOCOL.into(),
            manifest_id: manifest.manifest_id.clone(),
            manifest_digest: manifest.manifest_digest.clone(),
            decision: SupportDecision::Unknown,
            combination_id: None,
            issues: vec![issue(
                GaIssueCode::CombinationUnknown,
                "The exact combination is absent from the GA Support Manifest.",
                "Do not infer compatibility from adjacent semantic versions.",
                "Choose an exact manifest combination or request compatibility evidence.",
            )],
            next_actions: vec!["Choose an exact combination from the support manifest.".into()],
        },
    }
}

pub fn ga_support_manifest_integrity_valid(manifest: &GaSupportManifest) -> bool {
    if manifest.protocol != GA_SUPPORT_MANIFEST_PROTOCOL {
        return false;
    }
    let digest = ga_support_manifest_digest(manifest);
    manifest.manifest_digest == digest
        && manifest.manifest_id == format!("ga-support:{}", &digest[7..23])
}

fn ga_support_manifest_digest(manifest: &GaSupportManifest) -> String {
    if manifest.evidence_receipt_authorities.is_empty()
        && manifest.receipt_authority_public_keys.is_empty()
    {
        return digest_json(&manifest.clone().into_input());
    }
    let mut canonical = manifest.clone();
    canonical.protocol.clear();
    canonical.manifest_id.clear();
    canonical.manifest_digest.clear();
    digest_json(&canonical)
}

#[must_use]
pub fn contract_retirement_plan_integrity_is_valid(plan: &ContractRetirementPlan) -> bool {
    valid_digest(&plan.plan_digest)
        && plan.plan_digest
            == plan_digest(plan, |value| {
                value.plan_id.clear();
                value.plan_digest.clear();
            })
        && plan.plan_id == format!("contract-retirement:{}", &plan.plan_digest[7..23])
}

#[must_use]
pub fn contract_retirement_receipt_integrity_is_valid(receipt: &ContractRetirementReceipt) -> bool {
    let mut canonical = receipt.clone();
    canonical.receipt_id.clear();
    canonical.receipt_digest.clear();
    let digest = digest_json(&canonical);
    receipt.protocol == CONTRACT_RETIREMENT_RECEIPT_PROTOCOL
        && valid_digest(&receipt.plan_digest)
        && receipt.receipt_digest == digest
        && receipt.receipt_id == format!("contract-retirement-receipt:{}", &digest[7..23])
        && !receipt.contract_id.trim().is_empty()
        && !receipt.retired_version.trim().is_empty()
        && !receipt.replacement_version.trim().is_empty()
        && !receipt.approver.trim().is_empty()
        && !receipt.approval_reason.trim().is_empty()
        && receipt.retired
}

#[must_use]
pub fn render_ga_support_manifest(manifest: &GaSupportManifest) -> String {
    let mut output = format!(
        "# Lenso GA Support Manifest\n\n- Protocol: `{}`\n- Manifest ID: `{}`\n- Manifest digest: `{}`\n- Status: `{:?}`\n- Documentation: `{}` (`{}`)\n\n## Components\n\n",
        manifest.protocol,
        manifest.manifest_id,
        manifest.manifest_digest,
        manifest.status,
        manifest.documentation.version,
        manifest.documentation.digest,
    );
    for component in &manifest.components {
        output.push_str(&format!(
            "- `{}` — `{}`\n",
            component.reference(),
            component.digest
        ));
    }
    output.push_str("\n## Manifest and state formats\n\n");
    for format in &manifest.manifest_formats {
        output.push_str(&format!("- `{:?}`: `{}`\n", format.kind, format.version));
    }
    for state_version in &manifest.state_versions {
        output.push_str(&format!("- State: `{state_version}`\n"));
    }
    output.push_str("\n## Supported combinations\n\n");
    for combination in &manifest.combinations {
        output.push_str(&format!(
            "- `{}`: `{:?}`, state `{}`, components `{}`\n",
            combination.combination_id,
            combination.status,
            combination.state_version,
            combination.component_references.join("`, `")
        ));
    }
    output.push_str("\n## Upgrade and skew edges\n\n");
    for edge in &manifest.upgrade_edges {
        output.push_str(&format!(
            "- `{}`: `{}` -> `{}`; rollback safe `{}`; mixed versions `{}`\n",
            edge.edge_id,
            edge.source_format,
            edge.target_format,
            edge.rollback_safe,
            edge.mixed_version_references.join("`, `"),
        ));
    }
    output.push_str(
        "\nUnknown combinations are not inferred compatible from semantic-version proximity.\n",
    );
    output
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMigrationInput {
    pub kind: ManifestKind,
    pub source_format: String,
    pub target_format: String,
    pub source: Value,
    pub identity_pointers: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMigrationEffects {
    pub mutates_source: bool,
    pub creates_target: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMigrationPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub kind: ManifestKind,
    pub source_format: String,
    pub target_format: String,
    pub source_digest: String,
    pub migrated_digest: String,
    pub migrated: Value,
    pub identity_pointers: Vec<String>,
    pub effects: ManifestMigrationEffects,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMigrationReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub plan_digest: String,
    pub source_digest: String,
    pub migrated_digest: String,
    pub migrated: Value,
}

pub fn plan_manifest_migration(
    input: &ManifestMigrationInput,
    manifest: &GaSupportManifest,
) -> Result<ManifestMigrationPlan, GaIssue> {
    if !ga_support_manifest_integrity_valid(manifest) {
        return Err(issue(
            GaIssueCode::ManifestInvalid,
            "The GA Support Manifest content does not match its canonical identity.",
            "Reject modified or unverified support manifests before planning migration.",
            "Regenerate and verify the manifest before retrying dry-run.",
        ));
    }
    let formats = manifest
        .manifest_formats
        .iter()
        .map(|format| (format.kind, format.version.as_str()))
        .collect::<BTreeSet<_>>();
    if !formats.contains(&(input.kind, input.source_format.as_str()))
        || !formats.contains(&(input.kind, input.target_format.as_str()))
    {
        return Err(issue(
            GaIssueCode::ManifestFormatUnsupported,
            "The requested manifest migration edge is not supported.",
            "Select source and target formats declared by the GA Support Manifest.",
            "Inspect the manifest format matrix before applying any change.",
        ));
    }
    let mut migrated = input.source.clone();
    let Some(object) = migrated.as_object_mut() else {
        return Err(issue(
            GaIssueCode::ManifestInvalid,
            "The manifest must be a JSON object.",
            "Provide a valid public manifest artifact.",
            "Correct the source manifest and retry dry-run.",
        ));
    };
    object.insert(
        "protocol".into(),
        Value::String(input.target_format.clone()),
    );
    for pointer in &input.identity_pointers {
        if input.source.pointer(pointer) != migrated.pointer(pointer) {
            return Err(issue(
                GaIssueCode::ManifestIdentityChanged,
                "Manifest migration changed a protected identity.",
                "Preserve every declared Service, Module, Workload, Store, Contract, and authority identity.",
                "Correct the migration adapter before apply.",
            ));
        }
    }
    let source_digest = digest_json(&input.source);
    let migrated_digest = digest_json(&migrated);
    let mut plan = ManifestMigrationPlan {
        protocol: MANIFEST_MIGRATION_PLAN_PROTOCOL.into(),
        plan_id: String::new(),
        plan_digest: String::new(),
        kind: input.kind,
        source_format: input.source_format.clone(),
        target_format: input.target_format.clone(),
        source_digest,
        migrated_digest,
        migrated,
        identity_pointers: input.identity_pointers.clone(),
        effects: ManifestMigrationEffects::default(),
    };
    plan.plan_digest = plan_digest(&plan, |value| {
        value.plan_id.clear();
        value.plan_digest.clear();
    });
    plan.plan_id = format!("manifest-migration:{}", &plan.plan_digest[7..23]);
    Ok(plan)
}

pub fn apply_manifest_migration(
    plan: &ManifestMigrationPlan,
    current_source: &Value,
    target_exists: bool,
) -> Result<ManifestMigrationReceipt, GaIssue> {
    if plan_digest(plan, |value| {
        value.plan_id.clear();
        value.plan_digest.clear();
    }) != plan.plan_digest
    {
        return Err(plan_integrity_issue());
    }
    if target_exists {
        return Err(issue(
            GaIssueCode::ManifestTargetCollision,
            "The migration target already exists.",
            "Choose an empty target or return the previously committed receipt.",
            "Inspect the target and receipt store before retrying.",
        ));
    }
    if digest_json(current_source) != plan.source_digest {
        return Err(issue(
            GaIssueCode::ManifestSourceStale,
            "The source manifest changed after the plan was created.",
            "Generate a new plan bound to the current source digest.",
            "Repeat dry-run with the current manifest.",
        ));
    }
    for pointer in &plan.identity_pointers {
        if current_source.pointer(pointer) != plan.migrated.pointer(pointer) {
            return Err(issue(
                GaIssueCode::ManifestIdentityChanged,
                "The planned migration does not preserve a protected identity.",
                "Reject migration adapters that reinterpret business or authority identity.",
                "Regenerate the plan with an identity-preserving adapter.",
            ));
        }
    }
    let receipt_id = format!("manifest-migration-receipt:{}", &plan.plan_digest[7..23]);
    Ok(ManifestMigrationReceipt {
        protocol: MANIFEST_MIGRATION_RECEIPT_PROTOCOL.into(),
        receipt_id,
        plan_digest: plan.plan_digest.clone(),
        source_digest: plan.source_digest.clone(),
        migrated_digest: plan.migrated_digest.clone(),
        migrated: plan.migrated.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpgradeInput {
    pub service_id: String,
    pub from_release_id: String,
    pub from_release_digest: String,
    pub to_release_id: String,
    pub to_release_digest: String,
    pub config_revision_id: String,
    pub config_revision_digest: String,
    pub source_state_version: String,
    pub target_state_version: String,
    pub workflow_artifact_digests: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeWorkload {
    Migration,
    Api,
    Worker,
}

impl UpgradeWorkload {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Migration => "migration",
            Self::Api => "api",
            Self::Worker => "worker",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpgradeStep {
    pub sequence: u32,
    pub workload: UpgradeWorkload,
    pub precondition: String,
    pub expected_state_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeRollbackConstraint {
    pub automatic_allowed: bool,
    pub reason: String,
    pub approval_boundary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpgradePlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub manifest_id: String,
    pub manifest_digest: String,
    pub input: ServiceUpgradeInput,
    pub steps: Vec<ServiceUpgradeStep>,
    pub mixed_version_references: Vec<String>,
    pub preserved_identities: Vec<String>,
    pub rollback: UpgradeRollbackConstraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpgradeRuntimeObservation {
    pub workload: UpgradeWorkload,
    pub current_release_id: String,
    pub current_state_version: String,
    pub migration_completed: bool,
    pub workflow_artifact_digests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpgradeAdmission {
    pub protocol: String,
    pub plan_digest: String,
    pub workload: UpgradeWorkload,
    pub decision: SupportDecision,
    pub claims_work: bool,
    pub mutates_state: bool,
    pub issues: Vec<GaIssue>,
    pub next_actions: Vec<String>,
}

pub fn plan_service_upgrade(
    manifest: &GaSupportManifest,
    input: ServiceUpgradeInput,
) -> Result<ServiceUpgradePlan, GaIssue> {
    if !ga_support_manifest_integrity_valid(manifest) {
        return Err(issue(
            GaIssueCode::ManifestInvalid,
            "The GA Support Manifest content does not match its canonical identity.",
            "Reject modified or unverified support manifests before planning an upgrade.",
            "Regenerate and verify the manifest before retrying.",
        ));
    }
    let Some(edge) = manifest.upgrade_edges.iter().find(|edge| {
        edge.source_format == input.source_state_version
            && edge.target_format == input.target_state_version
    }) else {
        return Err(issue(
            GaIssueCode::UpgradeUnsupported,
            "The state upgrade edge is absent from the GA Support Manifest.",
            "Use a declared edge; do not infer reader or writer compatibility.",
            "Select a supported target or add reviewed compatibility evidence.",
        ));
    };
    let rollback = if edge.rollback_safe {
        UpgradeRollbackConstraint {
            automatic_allowed: true,
            reason: "The support edge declares state and workload rollback compatible.".into(),
            approval_boundary: None,
        }
    } else {
        UpgradeRollbackConstraint {
            automatic_allowed: false,
            reason: "The state edge is irreversible or not proven downgrade compatible.".into(),
            approval_boundary: Some("service_state_upgrade_intervention".into()),
        }
    };
    let mut plan = ServiceUpgradePlan {
        protocol: SERVICE_UPGRADE_PLAN_PROTOCOL.into(),
        plan_id: String::new(),
        plan_digest: String::new(),
        manifest_id: manifest.manifest_id.clone(),
        manifest_digest: manifest.manifest_digest.clone(),
        steps: vec![
            ServiceUpgradeStep {
                sequence: 1,
                workload: UpgradeWorkload::Migration,
                precondition: "exact source state and release digest remain current".into(),
                expected_state_version: input.target_state_version.clone(),
            },
            ServiceUpgradeStep {
                sequence: 2,
                workload: UpgradeWorkload::Api,
                precondition: "migration receipt is complete and target reader is compatible"
                    .into(),
                expected_state_version: input.target_state_version.clone(),
            },
            ServiceUpgradeStep {
                sequence: 3,
                workload: UpgradeWorkload::Worker,
                precondition:
                    "migration receipt is complete and pinned workflows are structurally compatible"
                        .into(),
                expected_state_version: input.target_state_version.clone(),
            },
        ],
        mixed_version_references: edge.mixed_version_references.clone(),
        preserved_identities: vec![
            "service".into(),
            "workflow_instance".into(),
            "workflow_definition_artifact".into(),
            "inbox".into(),
            "outbox".into(),
            "timer".into(),
            "attempt".into(),
            "compensation".into(),
            "story_segment".into(),
            "config_revision".into(),
            "deployment_observation".into(),
        ],
        rollback,
        input,
    };
    plan.plan_digest = plan_digest(&plan, |value| {
        value.plan_id.clear();
        value.plan_digest.clear();
    });
    plan.plan_id = format!("service-upgrade:{}", &plan.plan_digest[7..23]);
    Ok(plan)
}

#[must_use]
pub fn evaluate_service_upgrade_admission(
    plan: &ServiceUpgradePlan,
    observation: &ServiceUpgradeRuntimeObservation,
) -> ServiceUpgradeAdmission {
    let mut issues = Vec::new();
    let integrity_valid = plan_digest(plan, |value| {
        value.plan_id.clear();
        value.plan_digest.clear();
    }) == plan.plan_digest;
    if !integrity_valid {
        issues.push(plan_integrity_issue());
    }
    let expected_workflows = plan
        .input
        .workflow_artifact_digests
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let observed_workflows = observation
        .workflow_artifact_digests
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let admitted = integrity_valid
        && match observation.workload {
            UpgradeWorkload::Migration => {
                observation.current_release_id == plan.input.from_release_id
                    && observation.current_state_version == plan.input.source_state_version
            }
            UpgradeWorkload::Api => {
                observation.migration_completed
                    && observation.current_release_id == plan.input.to_release_id
                    && observation.current_state_version == plan.input.target_state_version
            }
            UpgradeWorkload::Worker => {
                observation.migration_completed
                    && observation.current_release_id == plan.input.to_release_id
                    && observation.current_state_version == plan.input.target_state_version
                    && expected_workflows == observed_workflows
            }
        };
    if !admitted {
        issues.push(issue(
            GaIssueCode::UpgradeUnsupported,
            "The Workload is not compatible with the current durable state and pinned artifacts.",
            "Do not claim work or mutate state until the exact migration and compatibility preconditions pass.",
            "Resume the migration-first plan or restore the last compatible release.",
        ));
    }
    ServiceUpgradeAdmission {
        protocol: "lenso.service-upgrade-admission.v1".into(),
        plan_digest: plan.plan_digest.clone(),
        workload: observation.workload,
        decision: if admitted {
            SupportDecision::Supported
        } else {
            SupportDecision::Blocked
        },
        claims_work: admitted && observation.workload == UpgradeWorkload::Worker,
        mutates_state: admitted && observation.workload == UpgradeWorkload::Migration,
        next_actions: if admitted {
            vec!["Execute only the admitted Workload step.".into()]
        } else {
            vec!["Restore exact state, release, migration, and pinned Workflow evidence.".into()]
        },
        issues,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractConsumerEvidence {
    pub consumer_id: String,
    pub active_version: Option<String>,
    pub replacement_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetirementInput {
    pub system_graph_digest: String,
    pub environment_evidence_digest: String,
    pub evidence_fresh: bool,
    pub contract_id: String,
    pub retiring_version: String,
    pub replacement_version: String,
    pub deprecation_window_complete: bool,
    pub consumers: Vec<ContractConsumerEvidence>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetirementEffects {
    pub retires_contract: bool,
    pub mutates_consumers: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetirementPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub input_digest: String,
    pub decision: SupportDecision,
    pub contract_id: String,
    pub retiring_version: String,
    pub replacement_version: String,
    pub affected_consumers: Vec<String>,
    pub irreversible_effects: Vec<String>,
    pub issues: Vec<GaIssue>,
    pub effects: ContractRetirementEffects,
    pub approval_boundary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetirementApproval {
    pub plan_digest: String,
    pub approver: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContractRetirementReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub plan_digest: String,
    pub contract_id: String,
    pub retired_version: String,
    pub replacement_version: String,
    pub approver: String,
    pub approval_reason: String,
    pub retired: bool,
}

#[must_use]
pub fn plan_contract_retirement(input: &ContractRetirementInput) -> ContractRetirementPlan {
    let mut issues = Vec::new();
    if !input.evidence_fresh {
        issues.push(issue(
            GaIssueCode::RetirementEvidenceStale,
            "Consumer or Environment Verification evidence is stale.",
            "Refresh the System graph and Environment Verification.",
            "Regenerate the Retirement plan from fresh evidence.",
        ));
    }
    if !input.deprecation_window_complete {
        issues.push(issue(
            GaIssueCode::RetirementDeprecationIncomplete,
            "The declared deprecation window has not elapsed.",
            "Continue serving the old Contract Version.",
            "Retry after the declared window completes.",
        ));
    }
    let affected_consumers = input
        .consumers
        .iter()
        .filter(|consumer| consumer.active_version.as_deref() == Some(&input.retiring_version))
        .map(|consumer| consumer.consumer_id.clone())
        .collect::<Vec<_>>();
    if !affected_consumers.is_empty() {
        issues.push(issue(
            GaIssueCode::RetirementActiveConsumer,
            "At least one active Consumer still uses the retiring Contract Version.",
            "Move every Consumer to the compatible replacement before Retirement.",
            "Inspect affected Consumers and replacement verification.",
        ));
    }
    if input
        .consumers
        .iter()
        .any(|consumer| consumer.active_version.is_none() || !consumer.replacement_verified)
    {
        issues.push(issue(
            GaIssueCode::RetirementReplacementMissing,
            "Consumer inventory is unknown or replacement coverage is incomplete.",
            "Verify every active Consumer against the replacement Contract Version.",
            "Refresh Consumer compatibility evidence.",
        ));
    }
    let input_digest = digest_json(input);
    let mut plan = ContractRetirementPlan {
        protocol: CONTRACT_RETIREMENT_PLAN_PROTOCOL.into(),
        plan_id: String::new(),
        plan_digest: String::new(),
        input_digest,
        decision: if issues.is_empty() {
            SupportDecision::Supported
        } else {
            SupportDecision::Unsupported
        },
        contract_id: input.contract_id.clone(),
        retiring_version: input.retiring_version.clone(),
        replacement_version: input.replacement_version.clone(),
        affected_consumers,
        irreversible_effects: vec!["stop serving the retired Contract Version".into()],
        issues,
        effects: ContractRetirementEffects::default(),
        approval_boundary: "contract_retirement".into(),
    };
    plan.plan_digest = plan_digest(&plan, |value| {
        value.plan_id.clear();
        value.plan_digest.clear();
    });
    plan.plan_id = format!("contract-retirement:{}", &plan.plan_digest[7..23]);
    plan
}

pub fn apply_contract_retirement(
    plan: &ContractRetirementPlan,
    current: &ContractRetirementInput,
    approval: &RetirementApproval,
) -> Result<ContractRetirementReceipt, GaIssue> {
    if plan_digest(plan, |value| {
        value.plan_id.clear();
        value.plan_digest.clear();
    }) != plan.plan_digest
    {
        return Err(plan_integrity_issue());
    }
    if digest_json(current) != plan.input_digest {
        return Err(issue(
            GaIssueCode::RetirementInputStale,
            "Retirement inputs changed after dry-run.",
            "Rebuild the plan from the current graph and Environment Verification.",
            "Repeat dry-run and request approval for the new digest.",
        ));
    }
    if plan.decision != SupportDecision::Supported
        || approval.plan_digest != plan.plan_digest
        || approval.approver.trim().is_empty()
        || approval.reason.trim().is_empty()
    {
        return Err(issue(
            GaIssueCode::RetirementApprovalInvalid,
            "Contract Retirement lacks exact human approval for this plan digest.",
            "Obtain approval bound to the current stale-safe plan.",
            "Stop before mutation and request the Contract Retirement Approval Boundary.",
        ));
    }
    let mut receipt = ContractRetirementReceipt {
        protocol: CONTRACT_RETIREMENT_RECEIPT_PROTOCOL.into(),
        receipt_id: String::new(),
        receipt_digest: String::new(),
        plan_digest: plan.plan_digest.clone(),
        contract_id: plan.contract_id.clone(),
        retired_version: plan.retiring_version.clone(),
        replacement_version: plan.replacement_version.clone(),
        approver: approval.approver.clone(),
        approval_reason: approval.reason.clone(),
        retired: true,
    };
    let digest = digest_json(&receipt);
    receipt.receipt_digest = digest.clone();
    receipt.receipt_id = format!("contract-retirement-receipt:{}", &digest[7..23]);
    Ok(receipt)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FailureCondition {
    ApiCrash,
    WorkerCrash,
    NetworkSlow,
    NetworkPartitioned,
    PostgresStoreUnavailable,
    NatsDisconnected,
    NatsAcknowledgementLost,
    NatsRedelivery,
    NatsPoisonEvent,
    SpiffeWorkloadApiUnavailable,
    SpiffeCredentialExpired,
    SpiffeCredentialRotated,
    TelemetryUnavailable,
    StoryAggregationUnavailable,
    RuntimeConsoleUnavailable,
    SystemPlaneUnavailable,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FailureOutcome {
    Continue,
    Degrade,
    PauseCoordinatedMutation,
    RejectWork,
    FailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailureObservation {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<FailureOutcome>,
    pub outcome: FailureOutcome,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailureScenarioInput {
    pub scenario_id: String,
    pub condition: FailureCondition,
    pub expected: FailureOutcome,
    pub observations: Vec<FailureObservation>,
    pub effects: Vec<String>,
    pub cleanup_complete: bool,
    pub adapter_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_time_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailureScenarioEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub scenario_id: String,
    pub condition: FailureCondition,
    pub expected: FailureOutcome,
    pub observations: Vec<FailureObservation>,
    pub effects: Vec<String>,
    pub cleanup_complete: bool,
    pub adapter_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_time_unix_ms: Option<u64>,
    pub decision: SupportDecision,
    pub issues: Vec<GaIssue>,
    pub remediation: Vec<String>,
}

#[must_use]
pub fn evaluate_failure_scenario(input: FailureScenarioInput) -> FailureScenarioEvidence {
    let mut issues = Vec::new();
    if input.observations.is_empty()
        || input.observations.iter().any(|observation| {
            observation.outcome != observation.expected.unwrap_or(input.expected)
        })
    {
        issues.push(issue(
            GaIssueCode::FailureUnexpectedOutcome,
            "Observed Service behavior differs from the declared Failure Scenario outcome.",
            "Fail the scenario and preserve authoritative Service evidence for diagnosis.",
            "Inspect business effects, Inbox/Outbox, Workflow, and Story evidence.",
        ));
    }
    if !input.cleanup_complete {
        issues.push(issue(
            GaIssueCode::FailureCleanupIncomplete,
            "Failure Scenario cleanup is incomplete.",
            "Remove or isolate every disposable process, Store, stream, socket, and trust artifact.",
            "Finish cleanup before accepting the scenario evidence.",
        ));
    }
    let mut evidence = FailureScenarioEvidence {
        protocol: FAILURE_SCENARIO_EVIDENCE_PROTOCOL.into(),
        evidence_id: String::new(),
        evidence_digest: String::new(),
        scenario_id: input.scenario_id,
        condition: input.condition,
        expected: input.expected,
        observations: input.observations,
        effects: input.effects,
        cleanup_complete: input.cleanup_complete,
        adapter_version: input.adapter_version,
        controlled_time_unix_ms: input.controlled_time_unix_ms,
        decision: if issues.is_empty() {
            SupportDecision::Supported
        } else {
            SupportDecision::Unsupported
        },
        remediation: issues
            .iter()
            .map(|issue| issue.remediation.clone())
            .collect(),
        issues,
    };
    evidence.evidence_digest = digest_without(&evidence, |value| value.evidence_digest.clear());
    evidence.evidence_id = format!("failure-evidence:{}", &evidence.evidence_digest[7..23]);
    evidence
}

fn issue(
    code: GaIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> GaIssue {
    GaIssue {
        code,
        message: message.into(),
        remediation: remediation.into(),
        next_actions: vec![next_action.into()],
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("GA support values serialize"))
}

fn digest_without<T: Clone + Serialize>(value: &T, clear: impl FnOnce(&mut T)) -> String {
    let mut canonical = value.clone();
    clear(&mut canonical);
    digest_json(&canonical)
}

fn plan_digest<T: Clone + Serialize>(value: &T, clear_identity: impl FnOnce(&mut T)) -> String {
    digest_without(value, clear_identity)
}

fn plan_integrity_issue() -> GaIssue {
    issue(
        GaIssueCode::PlanIntegrityInvalid,
        "The plan content does not match its immutable digest.",
        "Reject modified plans before any Workload claim, state mutation, or approval.",
        "Regenerate the plan from authoritative current inputs.",
    )
}

pub fn ga_support_manifest_schema() -> Value {
    schema::<GaSupportManifest>(GA_SUPPORT_MANIFEST_PROTOCOL)
}

pub fn manifest_migration_plan_schema() -> Value {
    schema::<ManifestMigrationPlan>(MANIFEST_MIGRATION_PLAN_PROTOCOL)
}

pub fn service_upgrade_plan_schema() -> Value {
    schema::<ServiceUpgradePlan>(SERVICE_UPGRADE_PLAN_PROTOCOL)
}

pub fn contract_retirement_plan_schema() -> Value {
    schema::<ContractRetirementPlan>(CONTRACT_RETIREMENT_PLAN_PROTOCOL)
}

pub fn failure_scenario_evidence_schema() -> Value {
    schema::<FailureScenarioEvidence>(FAILURE_SCENARIO_EVIDENCE_PROTOCOL)
}

fn schema<T: JsonSchema>(protocol: &str) -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T)).expect("schema serializes");
    let name = protocol.strip_prefix("lenso.").unwrap_or(protocol);
    schema["$id"] = Value::String(format!(
        "https://contracts.lenso.local/ga/lenso.{name}.schema.json"
    ));
    schema
}
