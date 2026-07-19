use crate::{
    CommonContextRequirement, CompatibilityCategory, EXTRACTION_READINESS_REPORT_PROTOCOL,
    ExtractionContractDirection, ExtractionContractKind, ExtractionCursorEvidence,
    ExtractionDataEvidenceSource, ExtractionReadinessIssueCode, ExtractionReadinessReport,
    ExtractionReadinessSurfaceSummary, ModuleManifest, ServiceTenancyMode, system_v2_graph,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

pub const EXTRACTION_PLAN_PROTOCOL: &str = "lenso.extraction-plan.v1";
pub const EXTRACTION_PLAN_GENERATOR_VERSION: &str = "lenso.extraction-plan-generator.v1";
const EXTRACTION_PLAN_SCHEMA_ID: &str =
    "https://contracts.lenso.local/extraction/lenso.extraction-plan.v1.schema.json";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionAuthorityKind {
    LinkedHost,
    AutonomousService,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionExpectedAuthority {
    pub kind: ExtractionAuthorityKind,
    pub owner_id: String,
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanContractVersion {
    pub contract_id: String,
    pub version: String,
    pub kind: ExtractionContractKind,
    pub direction: ExtractionContractDirection,
    pub artifact_reference: String,
    pub artifact_digest: String,
    pub artifact_format: ExtractionContractArtifactFormat,
    pub tenancy_mode: ServiceTenancyMode,
    #[serde(default)]
    pub required_context: Vec<CommonContextRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_id: Option<String>,
    #[serde(default)]
    pub consumer_ids: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionContractArtifactFormat {
    Openapi,
    Protobuf,
    JsonSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionEvidenceDigest {
    pub reference: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionPlanInputs {
    pub readiness_report: ExtractionReadinessReport,
    pub module: ModuleManifest,
    pub system: Value,
    pub contract_versions: Vec<ExtractionPlanContractVersion>,
    pub expected_authority: ExtractionExpectedAuthority,
    pub evidence_digests: Vec<ExtractionEvidenceDigest>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionInputPinKind {
    ReadinessEvidence,
    ModuleDeclaration,
    ContractVersion,
    SystemGraph,
    AnalyzerVersion,
    DataMapping,
    AuthorityRevision,
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionInputPin {
    pub kind: ExtractionInputPinKind,
    pub subject: String,
    pub digest: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionCopyMode {
    OnlineCheckpointed,
    BoundedWritePause,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionTableMapping {
    pub source_table: String,
    pub destination_table: String,
    pub destination_store: String,
    pub owner_module: String,
    pub copy_mode: ExtractionCopyMode,
    #[serde(default)]
    pub evidence_sources: Vec<ExtractionDataEvidenceSource>,
    #[serde(default)]
    pub cursors: Vec<ExtractionCursorEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionMigrationMapping {
    pub source_migration: String,
    pub source_reference: String,
    pub source_digest: String,
    pub destination_store: String,
    pub owner_module: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDataMapping {
    pub store_engine: String,
    pub destination_store: String,
    #[serde(default)]
    pub tables: Vec<ExtractionTableMapping>,
    #[serde(default)]
    pub migrations: Vec<ExtractionMigrationMapping>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionWorkloadRole {
    Api,
    Worker,
    Migration,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionWorkloadPlan {
    pub workload_id: String,
    pub role: ExtractionWorkloadRole,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionStorePlan {
    pub store_id: String,
    pub engine: String,
    pub isolated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionServiceReferencePlan {
    pub reference_id: String,
    pub contract_id: String,
    pub version: String,
    pub direction: ExtractionContractDirection,
    pub target_service_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionGeneratedClientPlan {
    pub client_id: String,
    pub owner_id: String,
    pub contract_id: String,
    pub version: String,
    pub artifact_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionServicePlan {
    pub service_id: String,
    pub module_id: String,
    pub workloads: Vec<ExtractionWorkloadPlan>,
    pub store: ExtractionStorePlan,
    pub contract_versions: Vec<ExtractionPlanContractVersion>,
    pub service_references: Vec<ExtractionServiceReferencePlan>,
    pub generated_clients: Vec<ExtractionGeneratedClientPlan>,
    pub preserved_capabilities: Vec<String>,
    pub preserved_surfaces: ExtractionReadinessSurfaceSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanDiffEntry {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanDiff {
    pub entries: Vec<ExtractionPlanDiffEntry>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionPlanIssueCode {
    PlanIntegrityInvalid,
    ReadinessEvidenceChanged,
    ModuleDeclarationChanged,
    ContractVersionChanged,
    SystemGraphChanged,
    AnalyzerVersionChanged,
    DataMappingChanged,
    AuthorityRevisionChanged,
    InputEvidenceChanged,
    ScaffoldConflict,
    DestinationExpansionFailed,
    BackfillCheckpointStale,
    ReconciliationMismatch,
    DrainIncomplete,
    ProvisionalCutoverFailed,
    VerificationFailed,
    RollbackRequired,
    FinalApprovalRequired,
    TerminalEvidenceIncomplete,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionPlanPhaseKind {
    Analysis,
    Scaffold,
    DestinationExpansion,
    Backfill,
    Reconciliation,
    Drain,
    ProvisionalCutover,
    Verification,
    RollbackOrCommit,
    TerminalEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionApprovalBoundary {
    pub boundary_id: String,
    pub phase_id: String,
    pub action: String,
    pub reason: String,
    pub required_pins: Vec<ExtractionInputPinKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanPhase {
    pub phase_id: String,
    pub order: u16,
    pub kind: ExtractionPlanPhaseKind,
    #[serde(default)]
    pub prerequisite_phase_ids: Vec<String>,
    pub prerequisites: Vec<String>,
    pub intended_mutations: Vec<String>,
    pub expected_evidence: Vec<String>,
    pub rollback_conditions: Vec<String>,
    pub issue_codes: Vec<ExtractionPlanIssueCode>,
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_boundary: Option<ExtractionApprovalBoundary>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanEffects {
    pub writes_repository_files: bool,
    pub starts_workloads: bool,
    pub copies_data: bool,
    pub changes_authority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlan {
    pub protocol: String,
    pub generator_version: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub target_module: String,
    pub source_system_id: String,
    pub readiness_classification: CompatibilityCategory,
    pub readiness_issue_codes: Vec<ExtractionReadinessIssueCode>,
    pub expected_authority: ExtractionExpectedAuthority,
    pub pinned_inputs: Vec<ExtractionInputPin>,
    pub data_mapping: ExtractionDataMapping,
    pub proposed_service: ExtractionServicePlan,
    pub diff: ExtractionPlanDiff,
    pub phases: Vec<ExtractionPlanPhase>,
    pub approval_boundaries: Vec<ExtractionApprovalBoundary>,
    pub effects: ExtractionPlanEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionPlanGenerationIssueCode {
    ReadinessNotReady,
    ReadinessTargetMismatch,
    SystemEvidenceInvalid,
    AuthorityMismatch,
    ContractVersionsMissing,
    InputInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanGenerationError {
    pub code: ExtractionPlanGenerationIssueCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionPlanGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionPlanGenerationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionStaleInput {
    pub kind: ExtractionInputPinKind,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPlanRejection {
    pub plan_id: String,
    pub message: String,
    pub issue_codes: Vec<ExtractionPlanIssueCode>,
    pub stale_inputs: Vec<ExtractionStaleInput>,
    pub next_actions: Vec<String>,
    pub effects: ExtractionPlanEffects,
}

impl fmt::Display for ExtractionPlanRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionPlanRejection {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractionPlanContent<'a> {
    protocol: &'a str,
    generator_version: &'a str,
    target_module: &'a str,
    source_system_id: &'a str,
    readiness_classification: CompatibilityCategory,
    readiness_issue_codes: &'a [ExtractionReadinessIssueCode],
    expected_authority: &'a ExtractionExpectedAuthority,
    pinned_inputs: &'a [ExtractionInputPin],
    data_mapping: &'a ExtractionDataMapping,
    proposed_service: &'a ExtractionServicePlan,
    diff: &'a ExtractionPlanDiff,
    phases: &'a [ExtractionPlanPhase],
    approval_boundaries: &'a [ExtractionApprovalBoundary],
    effects: ExtractionPlanEffects,
}

#[must_use]
pub fn extraction_input_digest(bytes: impl AsRef<[u8]>) -> String {
    let digest = Sha256::digest(bytes.as_ref());
    let mut rendered = String::with_capacity(7 + digest.len() * 2);
    rendered.push_str("sha256:");
    for byte in digest {
        write!(&mut rendered, "{byte:02x}").expect("writing to String cannot fail");
    }
    rendered
}

pub fn generate_extraction_plan(
    inputs: &ExtractionPlanInputs,
) -> Result<ExtractionPlan, ExtractionPlanGenerationError> {
    validate_generation_inputs(inputs)?;
    let graph = system_v2_graph(&inputs.system).map_err(|issues| {
        generation_error(
            ExtractionPlanGenerationIssueCode::SystemEvidenceInvalid,
            format!(
                "System evidence is invalid: {}",
                issues
                    .iter()
                    .map(|issue| issue.code.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Correct the lenso.system.v2 graph and regenerate the Extraction Plan.",
        )
    })?;
    let target_owner = graph
        .nodes
        .iter()
        .find(|node| node.kind == "module" && node.id == inputs.module.name)
        .and_then(|node| node.owner.as_deref());
    if target_owner != Some(inputs.expected_authority.owner_id.as_str()) {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::AuthorityMismatch,
            "The current System graph does not assign the target Module to the expected linked Host authority.",
            "Refresh readiness and authority evidence from the current System graph.",
        ));
    }
    let source_system_id = graph.system_id;
    if inputs.readiness_report.system_id.as_deref() != Some(source_system_id.as_str()) {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::SystemEvidenceInvalid,
            "The readiness report and current System graph identify different Systems.",
            "Regenerate readiness evidence from the current lenso.system.v2 artifact.",
        ));
    }
    let proposed_service = proposed_service(inputs);
    let data_mapping = data_mapping(inputs, &proposed_service.store.store_id)?;
    let pinned_inputs = pinned_inputs(inputs, &data_mapping)?;
    let diff = plan_diff(inputs, &proposed_service);
    let phases = plan_phases(&proposed_service, &data_mapping);
    let approval_boundaries = phases
        .iter()
        .filter_map(|phase| phase.approval_boundary.clone())
        .collect::<Vec<_>>();
    let effects = ExtractionPlanEffects::default();
    let content = ExtractionPlanContent {
        protocol: EXTRACTION_PLAN_PROTOCOL,
        generator_version: EXTRACTION_PLAN_GENERATOR_VERSION,
        target_module: &inputs.module.name,
        source_system_id: &source_system_id,
        readiness_classification: inputs.readiness_report.classification,
        readiness_issue_codes: &inputs.readiness_report.issue_codes,
        expected_authority: &inputs.expected_authority,
        pinned_inputs: &pinned_inputs,
        data_mapping: &data_mapping,
        proposed_service: &proposed_service,
        diff: &diff,
        phases: &phases,
        approval_boundaries: &approval_boundaries,
        effects,
    };
    let plan_digest = digest_serializable(&content)?;
    Ok(ExtractionPlan {
        protocol: EXTRACTION_PLAN_PROTOCOL.to_owned(),
        generator_version: EXTRACTION_PLAN_GENERATOR_VERSION.to_owned(),
        plan_id: format!("extraction-plan:{plan_digest}"),
        plan_digest,
        target_module: inputs.module.name.clone(),
        source_system_id,
        readiness_classification: inputs.readiness_report.classification,
        readiness_issue_codes: inputs.readiness_report.issue_codes.clone(),
        expected_authority: inputs.expected_authority.clone(),
        pinned_inputs,
        data_mapping,
        proposed_service,
        diff,
        phases,
        approval_boundaries,
        effects,
    })
}

pub fn dry_run_extraction_plan(
    inputs: &ExtractionPlanInputs,
) -> Result<ExtractionPlan, ExtractionPlanGenerationError> {
    generate_extraction_plan(inputs)
}

#[must_use]
pub fn extraction_plan_integrity_is_valid(plan: &ExtractionPlan) -> bool {
    if plan.protocol != EXTRACTION_PLAN_PROTOCOL
        || plan.generator_version != EXTRACTION_PLAN_GENERATOR_VERSION
        || plan.plan_id != format!("extraction-plan:{}", plan.plan_digest)
    {
        return false;
    }
    let content = ExtractionPlanContent {
        protocol: &plan.protocol,
        generator_version: &plan.generator_version,
        target_module: &plan.target_module,
        source_system_id: &plan.source_system_id,
        readiness_classification: plan.readiness_classification,
        readiness_issue_codes: &plan.readiness_issue_codes,
        expected_authority: &plan.expected_authority,
        pinned_inputs: &plan.pinned_inputs,
        data_mapping: &plan.data_mapping,
        proposed_service: &plan.proposed_service,
        diff: &plan.diff,
        phases: &plan.phases,
        approval_boundaries: &plan.approval_boundaries,
        effects: plan.effects,
    };
    digest_serializable(&content).is_ok_and(|digest| digest == plan.plan_digest)
}

#[allow(clippy::result_large_err)]
pub fn ensure_extraction_plan_fresh(
    plan: &ExtractionPlan,
    current_inputs: &ExtractionPlanInputs,
) -> Result<(), ExtractionPlanRejection> {
    if !extraction_plan_integrity_is_valid(plan) {
        return Err(ExtractionPlanRejection {
            plan_id: plan.plan_id.clone(),
            message: "Extraction Plan integrity validation failed before mutation.".to_owned(),
            issue_codes: vec![ExtractionPlanIssueCode::PlanIntegrityInvalid],
            stale_inputs: Vec::new(),
            next_actions: vec![
                "Discard the modified plan and generate a new content-addressed Extraction Plan."
                    .to_owned(),
            ],
            effects: ExtractionPlanEffects::default(),
        });
    }

    let destination_store = format!("{}-service-store", current_inputs.module.name);
    let current_mapping = data_mapping(current_inputs, &destination_store).map_err(|error| {
        ExtractionPlanRejection {
            plan_id: plan.plan_id.clone(),
            message:
                "Current migration evidence could not be pinned; the plan is stale before mutation."
                    .to_owned(),
            issue_codes: vec![ExtractionPlanIssueCode::DataMappingChanged],
            stale_inputs: Vec::new(),
            next_actions: error.next_actions,
            effects: ExtractionPlanEffects::default(),
        }
    })?;
    let current_pins = pinned_inputs(current_inputs, &current_mapping).map_err(|error| {
        ExtractionPlanRejection {
            plan_id: plan.plan_id.clone(),
            message:
                "Current extraction inputs could not be pinned; the plan is stale before mutation."
                    .to_owned(),
            issue_codes: vec![ExtractionPlanIssueCode::InputEvidenceChanged],
            stale_inputs: Vec::new(),
            next_actions: error.next_actions,
            effects: ExtractionPlanEffects::default(),
        }
    })?;
    let planned = plan
        .pinned_inputs
        .iter()
        .map(|pin| ((pin.kind, pin.subject.as_str()), pin.digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let current = current_pins
        .iter()
        .map(|pin| ((pin.kind, pin.subject.as_str()), pin.digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let keys = planned
        .keys()
        .chain(current.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut stale_inputs = keys
        .into_iter()
        .filter_map(|(kind, subject)| {
            let planned_digest = planned.get(&(kind, subject)).copied();
            let current_digest = current.get(&(kind, subject)).copied();
            (planned_digest != current_digest).then(|| ExtractionStaleInput {
                kind,
                subject: subject.to_owned(),
                planned_digest: planned_digest.map(str::to_owned),
                current_digest: current_digest.map(str::to_owned),
            })
        })
        .collect::<Vec<_>>();
    stale_inputs
        .sort_by(|left, right| (&left.kind, &left.subject).cmp(&(&right.kind, &right.subject)));
    if stale_inputs.is_empty() {
        return Ok(());
    }
    let mut issue_codes = stale_inputs
        .iter()
        .map(|input| stale_issue_code(input.kind))
        .collect::<Vec<_>>();
    issue_codes.sort();
    issue_codes.dedup();
    Err(ExtractionPlanRejection {
        plan_id: plan.plan_id.clone(),
        message: "Pinned extraction inputs changed; the stale plan was rejected before mutation."
            .to_owned(),
        issue_codes,
        stale_inputs,
        next_actions: vec![
            "Rerun readiness analysis and generate a new Extraction Plan from the current inputs."
                .to_owned(),
        ],
        effects: ExtractionPlanEffects::default(),
    })
}

fn validate_generation_inputs(
    inputs: &ExtractionPlanInputs,
) -> Result<(), ExtractionPlanGenerationError> {
    if inputs.readiness_report.protocol != EXTRACTION_READINESS_REPORT_PROTOCOL
        || inputs.readiness_report.analyzer_version.trim().is_empty()
    {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::InputInvalid,
            "The Extraction Readiness Report protocol or analyzer version is invalid.",
            "Regenerate readiness evidence with a supported public analyzer.",
        ));
    }
    if !inputs.readiness_report.ready
        || matches!(
            inputs.readiness_report.classification,
            CompatibilityCategory::Breaking | CompatibilityCategory::Blocked
        )
    {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::ReadinessNotReady,
            "Only a ready linked Module can produce an Extraction Plan.",
            "Resolve the readiness findings and rerun the public readiness command.",
        ));
    }
    if inputs.readiness_report.target_module != inputs.module.name {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::ReadinessTargetMismatch,
            "The readiness report does not describe the requested Module declaration.",
            "Regenerate readiness evidence for exactly the target Module.",
        ));
    }
    if inputs.expected_authority.kind != ExtractionAuthorityKind::LinkedHost
        || inputs.expected_authority.owner_id.trim().is_empty()
        || inputs.expected_authority.revision.trim().is_empty()
        || inputs.readiness_report.target_owner.as_deref()
            != Some(inputs.expected_authority.owner_id.as_str())
    {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::AuthorityMismatch,
            "Expected authority must pin the linked Host owner and a non-empty revision.",
            "Read the current linked authority and regenerate the plan with its exact revision.",
        ));
    }
    if inputs.contract_versions.is_empty() {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::ContractVersionsMissing,
            "Extraction Plan inputs must include the relevant authoritative Contract Versions.",
            "Resolve every provided or consumed Contract artifact and supply its version and digest.",
        ));
    }
    validate_contracts(&inputs.contract_versions)?;
    validate_relevant_contracts(inputs)?;
    validate_evidence_digests(&inputs.evidence_digests)?;
    Ok(())
}

fn validate_relevant_contracts(
    inputs: &ExtractionPlanInputs,
) -> Result<(), ExtractionPlanGenerationError> {
    let planned = inputs.contract_versions.iter().fold(
        BTreeMap::<&str, Vec<&ExtractionPlanContractVersion>>::new(),
        |mut map, contract| {
            map.entry(contract.contract_id.as_str())
                .or_default()
                .push(contract);
            map
        },
    );
    for evidence in &inputs.readiness_report.contract_evidence {
        if evidence.status != crate::ExtractionEvidenceStatus::Present {
            continue;
        }
        let Some(contract_id) = evidence.contract_id.as_deref() else {
            continue;
        };
        let matches = planned
            .get(contract_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if !matches.iter().any(|contract| {
            contract.kind == evidence.kind && contract.direction == evidence.direction
        }) {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::ContractVersionsMissing,
                format!(
                    "Readiness evidence requires Contract `{contract_id}` with kind {:?} and direction {:?}, but the plan input does not pin it.",
                    evidence.kind, evidence.direction
                ),
                "Supply the exact authoritative Contract Version and artifact digest used by readiness.",
            ));
        }
    }
    Ok(())
}

fn validate_contracts(
    contracts: &[ExtractionPlanContractVersion],
) -> Result<(), ExtractionPlanGenerationError> {
    let mut identities = BTreeSet::new();
    for contract in contracts {
        if contract.contract_id.trim().is_empty()
            || contract.version.trim().is_empty()
            || contract.artifact_reference.trim().is_empty()
            || !valid_sha256_digest(&contract.artifact_digest)
        {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "Contract Version inputs require stable identities, artifact references, and SHA-256 digests.",
                "Correct the Contract Version input and regenerate the Extraction Plan.",
            ));
        }
        if contract.kind == ExtractionContractKind::Service
            && contract.direction == ExtractionContractDirection::Consumes
            && contract
                .producer_id
                .as_deref()
                .is_none_or(|producer| producer.trim().is_empty())
        {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "A consumed Service Contract must identify its producing Service.",
                "Resolve the producer from the current System graph before planning a generated client.",
            ));
        }
        if !matches!(
            (contract.kind, contract.artifact_format),
            (
                ExtractionContractKind::Service,
                ExtractionContractArtifactFormat::Openapi
                    | ExtractionContractArtifactFormat::Protobuf
            ) | (
                ExtractionContractKind::Event,
                ExtractionContractArtifactFormat::JsonSchema
                    | ExtractionContractArtifactFormat::Protobuf
            )
        ) {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "A Contract Version uses an artifact format that does not match its Service or Event kind.",
                "Use OpenAPI or Protobuf for Service Contracts and JSON Schema or Protobuf for Event Contracts.",
            ));
        }
        if contract.tenancy_mode == ServiceTenancyMode::Required
            && !contract
                .required_context
                .contains(&CommonContextRequirement::Tenant)
        {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "A tenant-required Contract Version must require tenant context.",
                "Preserve the authoritative Contract context requirements before generating the plan.",
            ));
        }
        let mut required_context = contract.required_context.clone();
        required_context.sort();
        required_context.dedup();
        if required_context != contract.required_context {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "Contract context requirements must be unique and deterministically ordered.",
                "Sort and deduplicate the authoritative Contract context requirements.",
            ));
        }
        let identity = (contract.contract_id.as_str(), contract.version.as_str());
        if !identities.insert(identity) {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "A Contract Version input is duplicated.",
                "Supply one authoritative entry per Contract Version, kind, and direction.",
            ));
        }
    }
    Ok(())
}

fn validate_evidence_digests(
    evidence: &[ExtractionEvidenceDigest],
) -> Result<(), ExtractionPlanGenerationError> {
    if evidence.is_empty() {
        return Err(generation_error(
            ExtractionPlanGenerationIssueCode::InputInvalid,
            "Extraction Plan inputs must include digests for the readiness evidence bundle.",
            "Digest the analyzer and Store evidence consumed by readiness before planning.",
        ));
    }
    let mut references = BTreeMap::new();
    for item in evidence {
        if item.reference.trim().is_empty() || !valid_sha256_digest(&item.digest) {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "Evidence inputs require a stable reference and SHA-256 digest.",
                "Digest every analyzer, topology, Contract, and Store evidence input before planning.",
            ));
        }
        if references
            .insert(item.reference.as_str(), item.digest.as_str())
            .is_some()
        {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                "An evidence input reference is duplicated.",
                "Supply exactly one digest for every evidence input reference.",
            ));
        }
    }
    Ok(())
}

fn proposed_service(inputs: &ExtractionPlanInputs) -> ExtractionServicePlan {
    let service_id = format!("{}-service", inputs.module.name);
    let store_id = format!("{service_id}-store");
    let mut contracts = inputs.contract_versions.clone();
    for contract in &mut contracts {
        normalize_strings(&mut contract.consumer_ids);
        contract.required_context.sort();
        contract.required_context.dedup();
    }
    contracts.sort();
    let mut service_references = contracts
        .iter()
        .filter(|contract| contract.kind == ExtractionContractKind::Service)
        .map(|contract| ExtractionServiceReferencePlan {
            reference_id: format!(
                "{}-{}-{}-reference",
                service_id,
                stable_slug(&contract.contract_id),
                stable_slug(&contract.version)
            ),
            contract_id: contract.contract_id.clone(),
            version: contract.version.clone(),
            direction: contract.direction,
            target_service_id: if contract.direction == ExtractionContractDirection::Provides {
                service_id.clone()
            } else {
                contract
                    .producer_id
                    .clone()
                    .expect("consumed Service Contracts require a producer")
            },
        })
        .collect::<Vec<_>>();
    service_references.sort();
    service_references.dedup();

    let mut generated_clients = Vec::new();
    for contract in contracts
        .iter()
        .filter(|contract| contract.kind == ExtractionContractKind::Service)
    {
        if contract.direction == ExtractionContractDirection::Consumes {
            generated_clients.push(ExtractionGeneratedClientPlan {
                client_id: format!(
                    "{}-{}-client",
                    service_id,
                    stable_slug(&contract.contract_id)
                ),
                owner_id: service_id.clone(),
                contract_id: contract.contract_id.clone(),
                version: contract.version.clone(),
                artifact_reference: contract.artifact_reference.clone(),
            });
        } else {
            generated_clients.extend(contract.consumer_ids.iter().map(|consumer_id| {
                ExtractionGeneratedClientPlan {
                    client_id: format!(
                        "{}-{}-client",
                        stable_slug(consumer_id),
                        stable_slug(&contract.contract_id)
                    ),
                    owner_id: consumer_id.clone(),
                    contract_id: contract.contract_id.clone(),
                    version: contract.version.clone(),
                    artifact_reference: contract.artifact_reference.clone(),
                }
            }));
        }
    }
    generated_clients.sort();
    generated_clients.dedup();
    let mut capabilities = inputs.module.capabilities.clone();
    normalize_strings(&mut capabilities);
    ExtractionServicePlan {
        service_id: service_id.clone(),
        module_id: inputs.module.name.clone(),
        workloads: vec![
            ExtractionWorkloadPlan {
                workload_id: format!("{service_id}-api"),
                role: ExtractionWorkloadRole::Api,
            },
            ExtractionWorkloadPlan {
                workload_id: format!("{service_id}-worker"),
                role: ExtractionWorkloadRole::Worker,
            },
            ExtractionWorkloadPlan {
                workload_id: format!("{service_id}-migration"),
                role: ExtractionWorkloadRole::Migration,
            },
        ],
        store: ExtractionStorePlan {
            store_id,
            engine: "postgres".to_owned(),
            isolated: true,
        },
        contract_versions: contracts,
        service_references,
        generated_clients,
        preserved_capabilities: capabilities,
        preserved_surfaces: inputs.readiness_report.surfaces.clone(),
    }
}

fn data_mapping(
    inputs: &ExtractionPlanInputs,
    destination_store: &str,
) -> Result<ExtractionDataMapping, ExtractionPlanGenerationError> {
    #[derive(Default)]
    struct TableEvidence {
        sources: BTreeSet<ExtractionDataEvidenceSource>,
        cursors: BTreeSet<ExtractionCursorEvidence>,
    }
    let mut tables = BTreeMap::<(String, String), TableEvidence>::new();
    for table in &inputs.readiness_report.service_data.tables {
        let owner = table
            .owner_module
            .clone()
            .unwrap_or_else(|| "unresolved".to_owned());
        let item = tables.entry((table.table.clone(), owner)).or_default();
        item.sources.insert(table.source.clone());
        if let Some(cursor) = &table.cursor {
            item.cursors.insert(cursor.clone());
        }
    }
    let tables = tables
        .into_iter()
        .map(|((table, owner), evidence)| {
            let cursors = evidence.cursors.into_iter().collect::<Vec<_>>();
            ExtractionTableMapping {
                source_table: table.clone(),
                destination_table: table,
                destination_store: destination_store.to_owned(),
                owner_module: owner,
                copy_mode: if cursors.iter().any(|cursor| cursor.trustworthy) {
                    ExtractionCopyMode::OnlineCheckpointed
                } else {
                    ExtractionCopyMode::BoundedWritePause
                },
                evidence_sources: evidence.sources.into_iter().collect(),
                cursors,
            }
        })
        .collect::<Vec<_>>();
    let evidence_digests = inputs
        .evidence_digests
        .iter()
        .map(|evidence| (evidence.reference.as_str(), evidence.digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut migrations = Vec::new();
    for migration in &inputs.readiness_report.service_data.migrations {
        let references = migration
            .evidence_references
            .iter()
            .filter_map(|reference| {
                evidence_digests
                    .get(reference.as_str())
                    .map(|digest| (reference, *digest))
            })
            .collect::<Vec<_>>();
        let [(source_reference, source_digest)] = references.as_slice() else {
            return Err(generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                format!(
                    "Migration `{}` must resolve to exactly one digest-pinned source artifact.",
                    migration.migration
                ),
                "Record the authoritative migration path and content digest as Extraction Plan evidence.",
            ));
        };
        migrations.push(ExtractionMigrationMapping {
            source_migration: migration.migration.clone(),
            source_reference: (*source_reference).clone(),
            source_digest: (*source_digest).to_owned(),
            destination_store: destination_store.to_owned(),
            owner_module: migration
                .owner_module
                .clone()
                .unwrap_or_else(|| "unresolved".to_owned()),
        });
    }
    migrations.sort();
    migrations.dedup();
    Ok(ExtractionDataMapping {
        store_engine: "postgres".to_owned(),
        destination_store: destination_store.to_owned(),
        tables,
        migrations,
    })
}

fn pinned_inputs(
    inputs: &ExtractionPlanInputs,
    data_mapping: &ExtractionDataMapping,
) -> Result<Vec<ExtractionInputPin>, ExtractionPlanGenerationError> {
    let source_system_id = inputs
        .system
        .get("systemId")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut pins = vec![
        serializable_pin(
            ExtractionInputPinKind::ReadinessEvidence,
            &inputs.module.name,
            &inputs.readiness_report,
        )?,
        serializable_pin(
            ExtractionInputPinKind::ModuleDeclaration,
            &inputs.module.name,
            &inputs.module,
        )?,
        serializable_pin(
            ExtractionInputPinKind::SystemGraph,
            source_system_id,
            &inputs.system,
        )?,
        serializable_pin(
            ExtractionInputPinKind::AnalyzerVersion,
            &inputs.readiness_report.analyzer_version,
            &inputs.readiness_report.analyzer_version,
        )?,
        serializable_pin(
            ExtractionInputPinKind::DataMapping,
            &inputs.module.name,
            data_mapping,
        )?,
        serializable_pin(
            ExtractionInputPinKind::AuthorityRevision,
            &inputs.expected_authority.owner_id,
            &inputs.expected_authority,
        )?,
    ];
    let mut contracts = inputs.contract_versions.clone();
    for contract in &mut contracts {
        normalize_strings(&mut contract.consumer_ids);
        contract.required_context.sort();
        contract.required_context.dedup();
    }
    contracts.sort();
    pins.extend(
        contracts
            .iter()
            .map(|contract| {
                serializable_pin(
                    ExtractionInputPinKind::ContractVersion,
                    &format!("{}@{}", contract.contract_id, contract.version),
                    contract,
                )
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    pins.extend(
        inputs
            .evidence_digests
            .iter()
            .map(|evidence| ExtractionInputPin {
                kind: ExtractionInputPinKind::Evidence,
                subject: evidence.reference.clone(),
                digest: evidence.digest.clone(),
            }),
    );
    pins.sort();
    pins.dedup();
    Ok(pins)
}

fn serializable_pin<T: Serialize>(
    kind: ExtractionInputPinKind,
    subject: &str,
    value: &T,
) -> Result<ExtractionInputPin, ExtractionPlanGenerationError> {
    Ok(ExtractionInputPin {
        kind,
        subject: subject.to_owned(),
        digest: digest_serializable(value)?,
    })
}

fn plan_diff(inputs: &ExtractionPlanInputs, service: &ExtractionServicePlan) -> ExtractionPlanDiff {
    let mut entries = vec![
        ExtractionPlanDiffEntry {
            subject: format!("system.host.modules.{}", inputs.module.name),
            before: Some(inputs.expected_authority.owner_id.clone()),
            after: None,
        },
        ExtractionPlanDiffEntry {
            subject: format!("system.autonomousServices.{}", service.service_id),
            before: None,
            after: Some(format!("module={}", inputs.module.name)),
        },
        ExtractionPlanDiffEntry {
            subject: format!("authority.module.{}", inputs.module.name),
            before: Some(format!(
                "linked_host:{}@{}",
                inputs.expected_authority.owner_id, inputs.expected_authority.revision
            )),
            after: Some(format!("autonomous_service:{}", service.service_id)),
        },
        ExtractionPlanDiffEntry {
            subject: format!("store.{}", service.store.store_id),
            before: None,
            after: Some("postgres:isolated".to_owned()),
        },
    ];
    entries.extend(
        service
            .workloads
            .iter()
            .map(|workload| ExtractionPlanDiffEntry {
                subject: format!("workload.{}", workload.workload_id),
                before: None,
                after: Some(format!("{}:{:?}", service.service_id, workload.role).to_lowercase()),
            }),
    );
    entries.extend(
        service
            .service_references
            .iter()
            .map(|reference| ExtractionPlanDiffEntry {
                subject: format!("serviceReference.{}", reference.reference_id),
                before: None,
                after: Some(format!(
                    "{}@{}:{}",
                    reference.contract_id, reference.version, reference.target_service_id
                )),
            }),
    );
    entries.extend(
        service
            .generated_clients
            .iter()
            .map(|client| ExtractionPlanDiffEntry {
                subject: format!("generatedClient.{}", client.client_id),
                before: None,
                after: Some(format!("{}@{}", client.contract_id, client.version)),
            }),
    );
    entries.extend(
        service
            .contract_versions
            .iter()
            .filter(|contract| contract.direction == ExtractionContractDirection::Provides)
            .map(|contract| ExtractionPlanDiffEntry {
                subject: format!(
                    "contractProducer.{}@{}",
                    contract.contract_id, contract.version
                ),
                before: Some(format!("host:{}", inputs.expected_authority.owner_id)),
                after: Some(format!("autonomous_service:{}", service.service_id)),
            }),
    );
    entries.sort();
    entries.dedup();
    ExtractionPlanDiff { entries }
}

#[allow(clippy::too_many_lines)]
fn plan_phases(
    service: &ExtractionServicePlan,
    data_mapping: &ExtractionDataMapping,
) -> Vec<ExtractionPlanPhase> {
    let full_pause_tables = data_mapping
        .tables
        .iter()
        .filter(|table| table.copy_mode == ExtractionCopyMode::BoundedWritePause)
        .map(|table| table.source_table.as_str())
        .collect::<Vec<_>>();
    let backfill_action = if full_pause_tables.is_empty() {
        "Copy ordered idempotent batches through pinned trustworthy cursors and durable checkpoints.".to_owned()
    } else {
        format!(
            "Keep full-copy tables blocked until the bounded write pause: {}.",
            full_pause_tables.join(", ")
        )
    };
    let approval = ExtractionApprovalBoundary {
        boundary_id: "commit-extraction-authority".to_owned(),
        phase_id: "09-rollback-or-commit".to_owned(),
        action: "commit_authority_to_autonomous_service".to_owned(),
        reason: "Final ownership transfer and write reopening are irreversible without a separately reviewed reverse-migration plan.".to_owned(),
        required_pins: vec![
            ExtractionInputPinKind::ReadinessEvidence,
            ExtractionInputPinKind::ContractVersion,
            ExtractionInputPinKind::SystemGraph,
            ExtractionInputPinKind::DataMapping,
            ExtractionInputPinKind::AuthorityRevision,
            ExtractionInputPinKind::Evidence,
        ],
    };
    let provisional_cutover_action = format!(
        "Route declared verification traffic to candidate Service `{}` without admitting authoritative mutations.",
        service.service_id
    );
    vec![
        phase(
            1,
            ExtractionPlanPhaseKind::Analysis,
            Vec::new(),
            vec!["The target Module is linked, ready, and owned by the pinned Host authority."],
            Vec::new(),
            vec!["Fresh input digests and a content-addressed Extraction Plan."],
            vec!["No mutation occurs; regenerate the plan if any pinned input changes."],
            vec![
                ExtractionPlanIssueCode::ReadinessEvidenceChanged,
                ExtractionPlanIssueCode::ModuleDeclarationChanged,
                ExtractionPlanIssueCode::ContractVersionChanged,
                ExtractionPlanIssueCode::SystemGraphChanged,
                ExtractionPlanIssueCode::AnalyzerVersionChanged,
                ExtractionPlanIssueCode::DataMappingChanged,
                ExtractionPlanIssueCode::AuthorityRevisionChanged,
                ExtractionPlanIssueCode::InputEvidenceChanged,
            ],
            vec!["Review the exact plan, diff, risks, and Approval Boundary."],
            None,
        ),
        phase(
            2,
            ExtractionPlanPhaseKind::Scaffold,
            vec!["01-analysis"],
            vec!["The exact plan is fresh and the deterministic scaffold patch has been reviewed."],
            vec![
                "Write the candidate API, Worker, and Migration Workload scaffold plus generated bindings and clients.",
            ],
            vec![
                "A deterministic patch, file digests, compile evidence, and identity-preservation evidence.",
            ],
            vec!["Remove only plan-owned generated files; refuse changed or unrecognized files."],
            vec![ExtractionPlanIssueCode::ScaffoldConflict],
            vec!["Apply the scaffold without changing linked authority."],
            None,
        ),
        phase(
            3,
            ExtractionPlanPhaseKind::DestinationExpansion,
            vec!["02-scaffold"],
            vec![
                "The candidate Migration Workload is valid and the destination Store is isolated.",
            ],
            vec![
                "Create the isolated Service Store and apply expand-first destination schema changes.",
            ],
            vec!["Idempotent migration receipts and candidate health evidence."],
            vec!["Discard the candidate Store; never contract or delete source schema."],
            vec![ExtractionPlanIssueCode::DestinationExpansionFailed],
            vec!["Verify destination schema compatibility before copying Service Data."],
            None,
        ),
        phase(
            4,
            ExtractionPlanPhaseKind::Backfill,
            vec!["03-destination-expansion"],
            vec![
                "Destination expansion succeeded and every online table has a pinned trustworthy cursor.",
            ],
            vec![backfill_action.as_str()],
            vec![
                "Durable source high-water marks, destination checkpoints, counts, and batch digests.",
            ],
            vec![
                "Stop copying and retain the linked implementation as the sole authoritative writer.",
            ],
            vec![ExtractionPlanIssueCode::BackfillCheckpointStale],
            vec!["Resume only from validated checkpoints, then reconcile the copied state."],
            None,
        ),
        phase(
            5,
            ExtractionPlanPhaseKind::Reconciliation,
            vec!["04-backfill"],
            vec!["Backfill checkpoint and source high-water mark are stable."],
            Vec::new(),
            vec![
                "Matching identities, counts, field digests, relationships, and declared business invariants.",
            ],
            vec![
                "Keep linked authority and remediate or repeat backfill when reconciliation differs.",
            ],
            vec![ExtractionPlanIssueCode::ReconciliationMismatch],
            vec!["Record reconciliation evidence bound to the exact plan and checkpoint."],
            None,
        ),
        phase(
            6,
            ExtractionPlanPhaseKind::Drain,
            vec!["05-reconciliation"],
            vec!["Candidate readiness and pre-pause reconciliation passed."],
            vec![
                "Pause new Module mutations, drain requests, Inbox, Outbox, schedules, and Workflows, then copy the final delta.",
            ],
            vec![
                "Source quiescence, drained-work evidence, final checkpoint, and final reconciliation.",
            ],
            vec![
                "Reopen linked writes before provisional routing if drain or final reconciliation fails.",
            ],
            vec![ExtractionPlanIssueCode::DrainIncomplete],
            vec!["Proceed only while external authoritative mutations remain paused."],
            None,
        ),
        phase(
            7,
            ExtractionPlanPhaseKind::ProvisionalCutover,
            vec!["06-drain"],
            vec!["The source is quiescent, work is drained, and the final delta reconciles."],
            vec![provisional_cutover_action.as_str()],
            vec!["Candidate health, routing, compatibility, and provisional authority evidence."],
            vec!["Restore linked routing and authority without reverse data movement."],
            vec![ExtractionPlanIssueCode::ProvisionalCutoverFailed],
            vec!["Run behavior, Contract, policy, health, and Runtime Story verification."],
            None,
        ),
        phase(
            8,
            ExtractionPlanPhaseKind::Verification,
            vec!["07-provisional-cutover"],
            vec![
                "Provisional routing is active and all external authoritative mutations remain paused.",
            ],
            Vec::new(),
            vec![
                "Compatibility, policy, business scenario, durable state, Event, Workflow, and Runtime Story comparison evidence.",
            ],
            vec!["Rollback provisional routing on any mismatch or stale input."],
            vec![ExtractionPlanIssueCode::VerificationFailed],
            vec!["Choose rollback on failure or request exact final commit approval on success."],
            None,
        ),
        phase(
            9,
            ExtractionPlanPhaseKind::RollbackOrCommit,
            vec!["08-verification"],
            vec![
                "Verification is terminal, the plan is fresh, and the authority revision still matches.",
            ],
            vec![
                "Either restore linked routing and writes, or compare-and-set authority and topology to the candidate before reopening writes.",
            ],
            vec![
                "Rollback evidence, or verified approval plus one-owner authority and topology evidence.",
            ],
            vec![
                "Before commit, restore linked authority; after new Autonomous writes, block fast rollback without a reviewed reverse plan.",
            ],
            vec![
                ExtractionPlanIssueCode::RollbackRequired,
                ExtractionPlanIssueCode::FinalApprovalRequired,
            ],
            vec!["Stop at the human Approval Boundary before final ownership transfer."],
            Some(approval),
        ),
        phase(
            10,
            ExtractionPlanPhaseKind::TerminalEvidence,
            vec!["09-rollback-or-commit"],
            vec!["Rollback or commit reached one unambiguous authoritative owner."],
            Vec::new(),
            vec![
                "Terminal plan, phase receipts, evidence, authority, topology, rollback constraints, and next actions.",
            ],
            vec!["Do not erase source data, linked recovery state, or audit evidence."],
            vec![ExtractionPlanIssueCode::TerminalEvidenceIncomplete],
            vec![
                "Publish the versioned terminal extraction evidence for operators and automation.",
            ],
            None,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn phase(
    order: u16,
    kind: ExtractionPlanPhaseKind,
    prerequisite_phase_ids: Vec<&str>,
    prerequisites: Vec<&str>,
    intended_mutations: Vec<&str>,
    expected_evidence: Vec<&str>,
    rollback_conditions: Vec<&str>,
    issue_codes: Vec<ExtractionPlanIssueCode>,
    next_actions: Vec<&str>,
    approval_boundary: Option<ExtractionApprovalBoundary>,
) -> ExtractionPlanPhase {
    let label = match kind {
        ExtractionPlanPhaseKind::Analysis => "analysis",
        ExtractionPlanPhaseKind::Scaffold => "scaffold",
        ExtractionPlanPhaseKind::DestinationExpansion => "destination-expansion",
        ExtractionPlanPhaseKind::Backfill => "backfill",
        ExtractionPlanPhaseKind::Reconciliation => "reconciliation",
        ExtractionPlanPhaseKind::Drain => "drain",
        ExtractionPlanPhaseKind::ProvisionalCutover => "provisional-cutover",
        ExtractionPlanPhaseKind::Verification => "verification",
        ExtractionPlanPhaseKind::RollbackOrCommit => "rollback-or-commit",
        ExtractionPlanPhaseKind::TerminalEvidence => "terminal-evidence",
    };
    ExtractionPlanPhase {
        phase_id: format!("{order:02}-{label}"),
        order,
        kind,
        prerequisite_phase_ids: owned(prerequisite_phase_ids),
        prerequisites: owned(prerequisites),
        intended_mutations: owned(intended_mutations),
        expected_evidence: owned(expected_evidence),
        rollback_conditions: owned(rollback_conditions),
        issue_codes,
        next_actions: owned(next_actions),
        approval_boundary,
    }
}

fn owned(values: Vec<&str>) -> Vec<String> {
    values.into_iter().map(str::to_owned).collect()
}

fn stale_issue_code(kind: ExtractionInputPinKind) -> ExtractionPlanIssueCode {
    match kind {
        ExtractionInputPinKind::ReadinessEvidence => {
            ExtractionPlanIssueCode::ReadinessEvidenceChanged
        }
        ExtractionInputPinKind::ModuleDeclaration => {
            ExtractionPlanIssueCode::ModuleDeclarationChanged
        }
        ExtractionInputPinKind::ContractVersion => ExtractionPlanIssueCode::ContractVersionChanged,
        ExtractionInputPinKind::SystemGraph => ExtractionPlanIssueCode::SystemGraphChanged,
        ExtractionInputPinKind::AnalyzerVersion => ExtractionPlanIssueCode::AnalyzerVersionChanged,
        ExtractionInputPinKind::DataMapping => ExtractionPlanIssueCode::DataMappingChanged,
        ExtractionInputPinKind::AuthorityRevision => {
            ExtractionPlanIssueCode::AuthorityRevisionChanged
        }
        ExtractionInputPinKind::Evidence => ExtractionPlanIssueCode::InputEvidenceChanged,
    }
}

fn digest_serializable<T: Serialize>(value: &T) -> Result<String, ExtractionPlanGenerationError> {
    serde_json::to_vec(value)
        .map(extraction_input_digest)
        .map_err(|error| {
            generation_error(
                ExtractionPlanGenerationIssueCode::InputInvalid,
                format!("Extraction input could not be serialized deterministically: {error}"),
                "Correct the structured input and regenerate the Extraction Plan.",
            )
        })
}

fn generation_error(
    code: ExtractionPlanGenerationIssueCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ExtractionPlanGenerationError {
    ExtractionPlanGenerationError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
    }
}

fn valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn stable_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    slug
}

fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
}

#[must_use]
pub fn render_extraction_plan(plan: &ExtractionPlan) -> String {
    let mut output = vec![
        format!("Extraction plan: {}", plan.target_module),
        format!("Plan ID: {}", plan.plan_id),
        format!("System: {}", plan.source_system_id),
        format!(
            "Expected authority: {}:{}@{}",
            serialized_label(plan.expected_authority.kind),
            plan.expected_authority.owner_id,
            plan.expected_authority.revision
        ),
        format!("Candidate Service: {}", plan.proposed_service.service_id),
        format!(
            "Workloads: {}",
            plan.proposed_service
                .workloads
                .iter()
                .map(|workload| {
                    format!(
                        "{} ({})",
                        workload.workload_id,
                        serialized_label(workload.role)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        format!(
            "Store: {} (postgres, isolated)",
            plan.proposed_service.store.store_id
        ),
        "Effects: dry-run; writesRepositoryFiles=false; startsWorkloads=false; copiesData=false; changesAuthority=false".to_owned(),
        "Pinned inputs:".to_owned(),
    ];
    output.extend(plan.pinned_inputs.iter().map(|pin| {
        format!(
            "- {} {}: {}",
            serialized_label(pin.kind),
            pin.subject,
            pin.digest
        )
    }));
    output.push("Diff:".to_owned());
    output.extend(plan.diff.entries.iter().map(|entry| {
        format!(
            "- {}: {} -> {}",
            entry.subject,
            entry.before.as_deref().unwrap_or("absent"),
            entry.after.as_deref().unwrap_or("absent")
        )
    }));
    output.push("Phases:".to_owned());
    for phase in &plan.phases {
        output.push(format!(
            "- {} {}",
            phase.phase_id,
            serialized_label(phase.kind)
        ));
        for prerequisite in &phase.prerequisites {
            output.push(format!("  prerequisite: {prerequisite}"));
        }
        if phase.intended_mutations.is_empty() {
            output.push("  mutation: none".to_owned());
        } else {
            for mutation in &phase.intended_mutations {
                output.push(format!("  mutation: {mutation}"));
            }
        }
        for evidence in &phase.expected_evidence {
            output.push(format!("  evidence: {evidence}"));
        }
        for condition in &phase.rollback_conditions {
            output.push(format!("  rollback: {condition}"));
        }
        output.push(format!(
            "  issueCodes: {}",
            phase
                .issue_codes
                .iter()
                .map(|code| serialized_label(*code))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for action in &phase.next_actions {
            output.push(format!("  next: {action}"));
        }
        if let Some(boundary) = &phase.approval_boundary {
            output.push(format!("  approvalBoundary: {}", boundary.boundary_id));
        }
    }
    output.push(String::new());
    output.join("\n")
}

fn serialized_label<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

pub fn extraction_plan_json(plan: &ExtractionPlan) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(plan).map(|rendered| format!("{rendered}\n"))
}

#[must_use]
pub fn extraction_plan_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ExtractionPlan))
        .expect("Extraction Plan schema must serialize");
    let object = schema
        .as_object_mut()
        .expect("Extraction Plan schema must be an object");
    object.insert(
        "$id".to_owned(),
        Value::String(EXTRACTION_PLAN_SCHEMA_ID.to_owned()),
    );
    object.insert(
        "title".to_owned(),
        Value::String("Lenso Extraction Plan v1".to_owned()),
    );
    schema["properties"]["protocol"] = json!({
        "type": "string",
        "const": EXTRACTION_PLAN_PROTOCOL
    });
    schema["properties"]["generatorVersion"] = json!({
        "type": "string",
        "const": EXTRACTION_PLAN_GENERATOR_VERSION
    });
    schema["properties"]["planId"] = json!({
        "type": "string",
        "pattern": "^extraction-plan:sha256:[0-9a-f]{64}$"
    });
    schema["properties"]["planDigest"] = json!({
        "type": "string",
        "pattern": "^sha256:[0-9a-f]{64}$"
    });
    for field in [
        "writesRepositoryFiles",
        "startsWorkloads",
        "copiesData",
        "changesAuthority",
    ] {
        schema["$defs"]["ExtractionPlanEffects"]["properties"][field] = json!({
            "type": "boolean",
            "const": false
        });
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EXTRACTION_READINESS_ANALYZER_VERSION, EXTRACTION_READINESS_REPORT_PROTOCOL,
        ExtractionContractEvidence, ExtractionDataTableEvidence, ExtractionEvidenceStatus,
        ExtractionReadinessEffects, ExtractionServiceDataEvidence,
    };
    use lenso_contracts::ModuleManifest;

    fn inputs() -> ExtractionPlanInputs {
        let module = ModuleManifest::builder("support-ticket")
            .capabilities(vec!["support.tickets.read".to_owned()])
            .build();
        let report = ExtractionReadinessReport {
            protocol: EXTRACTION_READINESS_REPORT_PROTOCOL.to_owned(),
            analyzer_version: EXTRACTION_READINESS_ANALYZER_VERSION.to_owned(),
            target_module: module.name.clone(),
            system_id: Some("support-system".to_owned()),
            target_owner: Some("support-host".to_owned()),
            classification: CompatibilityCategory::Safe,
            ready: true,
            issue_codes: Vec::new(),
            contract_evidence: Vec::new(),
            active_consumers: Vec::new(),
            surfaces: ExtractionReadinessSurfaceSummary::default(),
            service_data: ExtractionServiceDataEvidence {
                complete: true,
                ..ExtractionServiceDataEvidence::default()
            },
            findings: Vec::new(),
            effects: ExtractionReadinessEffects::default(),
        };
        ExtractionPlanInputs {
            readiness_report: report,
            module,
            system: json!({
                "protocol": "lenso.system.v2",
                "systemId": "support-system",
                "host": { "hostId": "support-host", "modules": ["support-ticket"] },
                "providers": [{
                    "providerId": "notification-provider",
                    "modules": ["notification-gateway"]
                }],
                "autonomousServices": [{
                    "serviceId": "support-sla-service",
                    "modules": ["support-sla"],
                    "workloads": [{ "workloadId": "support-sla-api", "role": "api" }]
                }],
                "contracts": [{
                    "contractId": "support.sla-updated.v1",
                    "version": "v1",
                    "producerKind": "autonomous_service",
                    "producerId": "support-sla-service",
                    "artifact": {
                        "format": "json_schema",
                        "path": "contracts/events/support.sla-updated.v1.schema.json"
                    },
                    "tenancyMode": "required"
                }],
                "consumers": [{
                    "consumerId": "support-ticket-sla-updates",
                    "ownerKind": "host",
                    "ownerId": "support-host",
                    "contractId": "support.sla-updated.v1",
                    "tenancyMode": "required"
                }]
            }),
            contract_versions: vec![ExtractionPlanContractVersion {
                contract_id: "support-ticket-http.v1".to_owned(),
                version: "v1".to_owned(),
                kind: ExtractionContractKind::Service,
                direction: ExtractionContractDirection::Provides,
                artifact_reference: "contracts/openapi/support-ticket.v1.yaml".to_owned(),
                artifact_digest: extraction_input_digest(b"support-ticket-http-v1"),
                artifact_format: ExtractionContractArtifactFormat::Openapi,
                tenancy_mode: ServiceTenancyMode::Required,
                required_context: vec![CommonContextRequirement::Tenant],
                producer_id: None,
                consumer_ids: vec!["support-portal".to_owned()],
            }],
            expected_authority: ExtractionExpectedAuthority {
                kind: ExtractionAuthorityKind::LinkedHost,
                owner_id: "support-host".to_owned(),
                revision: "authority-7".to_owned(),
            },
            evidence_digests: vec![ExtractionEvidenceDigest {
                reference: "analyzer:rust/support-ticket".to_owned(),
                digest: extraction_input_digest(b"boundary-clean"),
            }],
        }
    }

    #[test]
    fn plan_is_content_addressed_and_dry_run_is_exact() {
        let inputs = inputs();
        let plan = generate_extraction_plan(&inputs).expect("plan should generate");
        let dry_run = dry_run_extraction_plan(&inputs).expect("dry run should generate");

        assert_eq!(plan, dry_run);
        assert!(extraction_plan_integrity_is_valid(&plan));
        assert_eq!(plan.effects, ExtractionPlanEffects::default());
        assert_eq!(
            plan.proposed_service
                .workloads
                .iter()
                .map(|workload| workload.role)
                .collect::<Vec<_>>(),
            vec![
                ExtractionWorkloadRole::Api,
                ExtractionWorkloadRole::Worker,
                ExtractionWorkloadRole::Migration,
            ]
        );
        assert!(plan.proposed_service.store.isolated);
    }

    #[test]
    fn plan_has_the_complete_ordered_phase_protocol() {
        let plan = generate_extraction_plan(&inputs()).expect("plan should generate");
        assert_eq!(
            plan.phases
                .iter()
                .map(|phase| phase.kind)
                .collect::<Vec<_>>(),
            vec![
                ExtractionPlanPhaseKind::Analysis,
                ExtractionPlanPhaseKind::Scaffold,
                ExtractionPlanPhaseKind::DestinationExpansion,
                ExtractionPlanPhaseKind::Backfill,
                ExtractionPlanPhaseKind::Reconciliation,
                ExtractionPlanPhaseKind::Drain,
                ExtractionPlanPhaseKind::ProvisionalCutover,
                ExtractionPlanPhaseKind::Verification,
                ExtractionPlanPhaseKind::RollbackOrCommit,
                ExtractionPlanPhaseKind::TerminalEvidence,
            ]
        );
        assert!(plan.phases.iter().all(|phase| {
            !phase.prerequisites.is_empty()
                && !phase.expected_evidence.is_empty()
                && !phase.rollback_conditions.is_empty()
                && !phase.issue_codes.is_empty()
                && !phase.next_actions.is_empty()
        }));
        assert_eq!(plan.approval_boundaries.len(), 1);
        assert_eq!(
            plan.approval_boundaries[0].action,
            "commit_authority_to_autonomous_service"
        );
    }

    #[test]
    fn input_order_does_not_change_plan_identity() {
        let mut left = inputs();
        left.contract_versions.push(ExtractionPlanContractVersion {
            contract_id: "support-sla-grpc.v1".to_owned(),
            version: "v1".to_owned(),
            kind: ExtractionContractKind::Service,
            direction: ExtractionContractDirection::Consumes,
            artifact_reference: "contracts/services/support-sla.v1.proto".to_owned(),
            artifact_digest: extraction_input_digest(b"support-sla-grpc-v1"),
            artifact_format: ExtractionContractArtifactFormat::Protobuf,
            tenancy_mode: ServiceTenancyMode::Required,
            required_context: vec![CommonContextRequirement::Tenant],
            producer_id: Some("support-sla-service".to_owned()),
            consumer_ids: Vec::new(),
        });
        left.evidence_digests.push(ExtractionEvidenceDigest {
            reference: "store:host-postgres".to_owned(),
            digest: extraction_input_digest(b"read-only-observation"),
        });
        let mut right = left.clone();
        right.contract_versions.reverse();
        right.evidence_digests.reverse();

        let left = generate_extraction_plan(&left).expect("left plan");
        let right = generate_extraction_plan(&right).expect("right plan");
        assert_eq!(left.plan_id, right.plan_id);
        assert_eq!(left, right);
    }

    #[test]
    fn authority_or_evidence_drift_rejects_before_mutation() {
        let inputs = inputs();
        let plan = generate_extraction_plan(&inputs).expect("plan should generate");
        let mut changed = inputs.clone();
        changed.expected_authority.revision = "authority-8".to_owned();
        changed.evidence_digests[0].digest = extraction_input_digest(b"changed-evidence");

        let rejection = ensure_extraction_plan_fresh(&plan, &changed)
            .expect_err("changed inputs must reject the stale plan");
        assert_eq!(rejection.effects, ExtractionPlanEffects::default());
        assert_eq!(
            rejection.issue_codes,
            vec![
                ExtractionPlanIssueCode::AuthorityRevisionChanged,
                ExtractionPlanIssueCode::InputEvidenceChanged,
            ]
        );
        assert_eq!(
            rejection
                .stale_inputs
                .iter()
                .map(|input| input.kind)
                .collect::<Vec<_>>(),
            vec![
                ExtractionInputPinKind::AuthorityRevision,
                ExtractionInputPinKind::Evidence,
            ]
        );
    }

    #[test]
    fn modified_plan_fails_integrity_before_freshness() {
        let inputs = inputs();
        let mut plan = generate_extraction_plan(&inputs).expect("plan should generate");
        plan.diff.entries.clear();

        let rejection = ensure_extraction_plan_fresh(&plan, &inputs)
            .expect_err("modified plans must fail integrity");
        assert_eq!(
            rejection.issue_codes,
            vec![ExtractionPlanIssueCode::PlanIntegrityInvalid]
        );
        assert!(rejection.stale_inputs.is_empty());
    }

    #[test]
    fn every_pinned_input_category_rejects_drift() {
        let original = inputs();
        let plan = generate_extraction_plan(&original).expect("plan should generate");

        let mut readiness = original.clone();
        readiness.readiness_report.system_id = Some("changed-evidence-system".to_owned());
        assert_stale_code(
            &plan,
            &readiness,
            ExtractionPlanIssueCode::ReadinessEvidenceChanged,
        );

        let mut module = original.clone();
        module
            .module
            .capabilities
            .push("support.tickets.write".to_owned());
        assert_stale_code(
            &plan,
            &module,
            ExtractionPlanIssueCode::ModuleDeclarationChanged,
        );

        let mut contract = original.clone();
        contract.contract_versions[0].artifact_digest =
            extraction_input_digest(b"changed-contract");
        assert_stale_code(
            &plan,
            &contract,
            ExtractionPlanIssueCode::ContractVersionChanged,
        );

        let mut topology = original.clone();
        topology.system["host"]["modules"] = json!(["auth", "support-ticket"]);
        assert_stale_code(
            &plan,
            &topology,
            ExtractionPlanIssueCode::SystemGraphChanged,
        );

        let mut analyzer = original.clone();
        analyzer.readiness_report.analyzer_version = "lenso.extraction-readiness.v3".to_owned();
        assert_stale_code(
            &plan,
            &analyzer,
            ExtractionPlanIssueCode::AnalyzerVersionChanged,
        );

        let mut data = original.clone();
        data.readiness_report
            .service_data
            .tables
            .push(ExtractionDataTableEvidence {
                table: "support.tickets".to_owned(),
                owner_module: Some("support-ticket".to_owned()),
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                volume: None,
                cursor: None,
                evidence_references: vec!["changed:data-mapping".to_owned()],
            });
        assert_stale_code(&plan, &data, ExtractionPlanIssueCode::DataMappingChanged);

        let mut authority = original.clone();
        authority.expected_authority.revision = "authority-8".to_owned();
        assert_stale_code(
            &plan,
            &authority,
            ExtractionPlanIssueCode::AuthorityRevisionChanged,
        );

        let mut evidence = original;
        evidence.evidence_digests[0].digest = extraction_input_digest(b"changed-evidence");
        assert_stale_code(
            &plan,
            &evidence,
            ExtractionPlanIssueCode::InputEvidenceChanged,
        );
    }

    fn assert_stale_code(
        plan: &ExtractionPlan,
        inputs: &ExtractionPlanInputs,
        expected: ExtractionPlanIssueCode,
    ) {
        let rejection = ensure_extraction_plan_fresh(plan, inputs)
            .expect_err("changed pinned input must reject the plan");
        assert!(rejection.issue_codes.contains(&expected));
        assert_eq!(rejection.effects, ExtractionPlanEffects::default());
    }

    #[test]
    fn plan_schema_accepts_public_json_and_v1_reader_ignores_future_fields() {
        let plan = generate_extraction_plan(&inputs()).expect("plan should generate");
        let value = serde_json::to_value(&plan).expect("plan should serialize");
        let validator = jsonschema::validator_for(&extraction_plan_schema())
            .expect("plan schema should compile");
        assert!(validator.is_valid(&value));

        let mut future = value;
        future["futureField"] = json!(true);
        let decoded: ExtractionPlan =
            serde_json::from_value(future).expect("v1 reader should ignore future fields");
        assert_eq!(decoded.plan_id, plan.plan_id);
    }

    #[test]
    fn blocked_readiness_cannot_generate_a_plan() {
        let mut inputs = inputs();
        inputs.readiness_report.ready = false;
        inputs.readiness_report.classification = CompatibilityCategory::Blocked;

        let error = generate_extraction_plan(&inputs).expect_err("blocked readiness must fail");
        assert_eq!(
            error.code,
            ExtractionPlanGenerationIssueCode::ReadinessNotReady
        );
    }

    #[test]
    fn every_contract_used_by_readiness_must_be_pinned() {
        let mut inputs = inputs();
        inputs
            .readiness_report
            .contract_evidence
            .push(ExtractionContractEvidence {
                subject: "event-handler:apply_sla_update".to_owned(),
                kind: ExtractionContractKind::Event,
                direction: ExtractionContractDirection::Consumes,
                status: ExtractionEvidenceStatus::Present,
                contract_id: Some("support.sla-updated.v1".to_owned()),
                evidence_references: vec![
                    "contracts/events/support.sla-updated.v1.schema.json".to_owned(),
                ],
            });

        let error = generate_extraction_plan(&inputs)
            .expect_err("readiness Contract Versions must be pinned");
        assert_eq!(
            error.code,
            ExtractionPlanGenerationIssueCode::ContractVersionsMissing
        );
    }
}
