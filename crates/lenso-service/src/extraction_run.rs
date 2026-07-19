use crate::{
    ExtractionExpectedAuthority, ExtractionPlan, ExtractionPlanInputs, ExtractionPlanPhaseKind,
    ExtractionScaffold, ExtractionScaffoldApplyResult, ExtractionWorkloadRole,
    ensure_extraction_plan_fresh, extraction_input_digest, extraction_plan_integrity_is_valid,
    extraction_scaffold_integrity_is_valid, validate_extraction_scaffold,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const EXTRACTION_RUN_PROTOCOL: &str = "lenso.extraction-run.v1";
pub const EXTRACTION_OPERATION_RECEIPT_PROTOCOL: &str = "lenso.extraction-operation-receipt.v1";
pub const DESTINATION_EXPANSION_PHASE_ID: &str = "03-destination-expansion";
const EXTRACTION_RUN_SCHEMA_ID: &str =
    "https://contracts.lenso.local/extraction/lenso.extraction-run.v1.schema.json";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRunMode {
    Apply,
    DryRun,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRunStatus {
    Planned,
    InProgress,
    Blocked,
    Succeeded,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionExpansionOperationKind {
    CreateIsolatedStore,
    ApplyExpandMigration,
    VerifyMigrationWorkload,
    VerifyCandidateHealth,
}

impl ExtractionExpansionOperationKind {
    fn is_mutating(self) -> bool {
        matches!(self, Self::CreateIsolatedStore | Self::ApplyExpandMigration)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionMigrationArtifact {
    pub migration_id: String,
    pub source_reference: String,
    pub source_digest: String,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionExpandMigration {
    pub migration_id: String,
    pub source_reference: String,
    pub source_digest: String,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionExpansionOperation {
    pub operation_id: String,
    pub operation_digest: String,
    pub order: u16,
    pub kind: ExtractionExpansionOperationKind,
    pub workload_id: String,
    pub candidate_service_id: String,
    pub destination_store_id: String,
    pub mutating: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<ExtractionExpandMigration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunExpectedState {
    pub plan_id: String,
    pub plan_digest: String,
    pub scaffold_id: String,
    pub scaffold_digest: String,
    pub phase_id: String,
    pub source_authority: ExtractionExpectedAuthority,
    pub linked_authority_remains_authoritative: bool,
    pub source_store_remains_unchanged: bool,
    pub candidate_service_id: String,
    pub destination_store_id: String,
    pub destination_store_engine: String,
    pub destination_store_isolated: bool,
    pub migration_workload_id: String,
    pub api_workload_id: String,
    pub ordered_operations_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunPhase {
    pub phase_id: String,
    pub kind: ExtractionPlanPhaseKind,
    pub status: ExtractionRunStatus,
    #[serde(default)]
    pub completed_operation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_operation_id: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRunEvidenceKind {
    StoreIsolation,
    MigrationApplied,
    MigrationWorkloadHealth,
    CandidateHealth,
    SourceAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunEvidence {
    pub kind: ExtractionRunEvidenceKind,
    pub subject: String,
    pub digest: String,
    pub detail: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionOperationOutcome {
    Created,
    AlreadyExists,
    Applied,
    AlreadyApplied,
    Healthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionOperationReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub run_id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub expected_state_digest: String,
    pub operation_id: String,
    pub operation_digest: String,
    pub operation_kind: ExtractionExpansionOperationKind,
    pub workload_id: String,
    pub candidate_service_id: String,
    pub destination_store_id: String,
    pub outcome: ExtractionOperationOutcome,
    pub source_authority: ExtractionExpectedAuthority,
    pub source_store_unchanged: bool,
    pub linked_authority_remains_authoritative: bool,
    pub candidate_authoritative: bool,
    #[serde(default)]
    pub evidence: Vec<ExtractionRunEvidence>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRunErrorCode {
    PlanStale,
    AuthorityChanged,
    SourceMutationReported,
    ReceiptInvalid,
    StoreProvisioningFailed,
    MigrationFailed,
    MigrationWorkloadUnhealthy,
    CandidateUnhealthy,
    WorkloadUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunError {
    pub sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    pub code: ExtractionRunErrorCode,
    pub message: String,
    #[serde(default)]
    pub evidence: Vec<ExtractionRunEvidence>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    pub resolved: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunEffects {
    pub creates_destination_store: bool,
    pub applies_destination_schema: bool,
    pub invokes_candidate_workload_behavior: bool,
    pub copies_service_data: bool,
    pub mutates_source_store: bool,
    pub mutates_linked_implementation: bool,
    pub changes_authority: bool,
    pub performs_destructive_cleanup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRun {
    pub protocol: String,
    pub run_id: String,
    pub run_digest: String,
    pub revision: u64,
    pub mode: ExtractionRunMode,
    pub plan: ExtractionPlan,
    pub current_phase: ExtractionRunPhase,
    pub expected_state: ExtractionRunExpectedState,
    pub expected_state_digest: String,
    pub ordered_operations: Vec<ExtractionExpansionOperation>,
    #[serde(default)]
    pub receipts: Vec<ExtractionOperationReceipt>,
    #[serde(default)]
    pub evidence: Vec<ExtractionRunEvidence>,
    #[serde(default)]
    pub errors: Vec<ExtractionRunError>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    pub effects: ExtractionRunEffects,
}

#[derive(Debug, Clone)]
pub struct ExtractionRunInputs {
    pub plan: ExtractionPlan,
    pub current_plan_inputs: ExtractionPlanInputs,
    pub scaffold: ExtractionScaffold,
    pub scaffold_apply_result: ExtractionScaffoldApplyResult,
    pub migrations: Vec<ExtractionMigrationArtifact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRunStartErrorCode {
    PlanInvalid,
    PlanStale,
    PhaseInvalid,
    ScaffoldInvalid,
    ScaffoldNotApplied,
    UnsupportedStoreEngine,
    StoreNotIsolated,
    WorkloadMissing,
    MigrationArtifactMissing,
    MigrationArtifactUnexpected,
    MigrationArtifactChanged,
    MigrationNotExpandFirst,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunStartError {
    pub code: ExtractionRunStartErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
    pub effects: ExtractionRunEffects,
}

impl fmt::Display for ExtractionRunStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionRunStartError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionRunAdvanceErrorCode {
    RunInvalid,
    DryRunCannotAdvance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunAdvanceError {
    pub code: ExtractionRunAdvanceErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionRunAdvanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionRunAdvanceError {}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionWorkloadFailureCode {
    Unavailable,
    StoreProvisioningFailed,
    MigrationFailed,
    MigrationWorkloadUnhealthy,
    CandidateUnhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionWorkloadFailure {
    pub code: ExtractionWorkloadFailureCode,
    pub message: String,
    #[serde(default)]
    pub evidence: Vec<ExtractionRunEvidence>,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionWorkloadFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionWorkloadFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionWorkloadRequest {
    pub run_id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub expected_state: ExtractionRunExpectedState,
    pub expected_state_digest: String,
    pub operation: ExtractionExpansionOperation,
}

/// Public behavior used by CLI-owned Postgres orchestration and candidate Workloads.
///
/// Implementations must persist operation receipts beside the destination effect.
/// `inspect_receipt` is always called before `execute`, which lets a restarted
/// caller recover an effect committed before its Extraction Run was saved.
#[async_trait]
pub trait ExtractionExpansionWorkload: fmt::Debug + Send + Sync {
    async fn inspect_receipt(
        &self,
        request: &ExtractionWorkloadRequest,
    ) -> Result<Option<ExtractionOperationReceipt>, ExtractionWorkloadFailure>;

    async fn execute(
        &self,
        request: &ExtractionWorkloadRequest,
    ) -> Result<ExtractionOperationReceipt, ExtractionWorkloadFailure>;
}

pub fn start_destination_expansion(
    inputs: &ExtractionRunInputs,
) -> Result<ExtractionRun, ExtractionRunStartError> {
    build_destination_expansion(inputs, ExtractionRunMode::Apply)
}

pub fn dry_run_destination_expansion(
    inputs: &ExtractionRunInputs,
) -> Result<ExtractionRun, ExtractionRunStartError> {
    build_destination_expansion(inputs, ExtractionRunMode::DryRun)
}

fn build_destination_expansion(
    inputs: &ExtractionRunInputs,
    mode: ExtractionRunMode,
) -> Result<ExtractionRun, ExtractionRunStartError> {
    validate_start_inputs(inputs)?;
    let plan = &inputs.plan;
    let (operations, migration_workload_id, api_workload_id) =
        destination_expansion_operations(inputs)?;

    let ordered_operations_digest = digest_serializable(&operations)?;
    let expected_state = ExtractionRunExpectedState {
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        scaffold_id: inputs.scaffold.scaffold_id.clone(),
        scaffold_digest: inputs.scaffold.scaffold_digest.clone(),
        phase_id: DESTINATION_EXPANSION_PHASE_ID.to_owned(),
        source_authority: plan.expected_authority.clone(),
        linked_authority_remains_authoritative: true,
        source_store_remains_unchanged: true,
        candidate_service_id: plan.proposed_service.service_id.clone(),
        destination_store_id: plan.proposed_service.store.store_id.clone(),
        destination_store_engine: plan.proposed_service.store.engine.clone(),
        destination_store_isolated: true,
        migration_workload_id,
        api_workload_id,
        ordered_operations_digest,
    };
    let expected_state_digest = digest_serializable(&expected_state)?;
    let run_identity_digest = digest_serializable(&(
        plan.plan_id.as_str(),
        DESTINATION_EXPANSION_PHASE_ID,
        expected_state_digest.as_str(),
    ))?;
    let mut run = ExtractionRun {
        protocol: EXTRACTION_RUN_PROTOCOL.to_owned(),
        run_id: format!("extraction-run:{run_identity_digest}"),
        run_digest: String::new(),
        revision: 1,
        mode,
        plan: plan.clone(),
        current_phase: ExtractionRunPhase {
            phase_id: DESTINATION_EXPANSION_PHASE_ID.to_owned(),
            kind: ExtractionPlanPhaseKind::DestinationExpansion,
            status: ExtractionRunStatus::Planned,
            completed_operation_ids: Vec::new(),
            next_operation_id: operations.first().map(|operation| operation.operation_id.clone()),
        },
        expected_state,
        expected_state_digest,
        ordered_operations: operations,
        receipts: Vec::new(),
        evidence: Vec::new(),
        errors: Vec::new(),
        next_actions: match mode {
            ExtractionRunMode::Apply => vec![
                "Persist this Run, then advance exactly one destination operation through the public Workload behavior."
                    .to_owned(),
            ],
            ExtractionRunMode::DryRun => vec![
                "Review these exact ordered operations before starting an apply Run."
                    .to_owned(),
            ],
        },
        effects: ExtractionRunEffects::default(),
    };
    refresh_run_digest(&mut run);
    Ok(run)
}

fn destination_expansion_operations(
    inputs: &ExtractionRunInputs,
) -> Result<(Vec<ExtractionExpansionOperation>, String, String), ExtractionRunStartError> {
    let plan = &inputs.plan;
    let migration_workload_id = workload_id(plan, ExtractionWorkloadRole::Migration)?;
    let api_workload_id = workload_id(plan, ExtractionWorkloadRole::Api)?;
    let mut operations = vec![operation(
        1,
        ExtractionExpansionOperationKind::CreateIsolatedStore,
        &migration_workload_id,
        plan,
        None,
    )?];
    for (index, mapping) in plan.data_mapping.migrations.iter().enumerate() {
        let artifact = inputs
            .migrations
            .iter()
            .find(|artifact| artifact.migration_id == mapping.source_migration)
            .expect("validated migration artifact");
        let order = u16::try_from(index + 2).map_err(|_| too_many_operations_error())?;
        operations.push(operation(
            order,
            ExtractionExpansionOperationKind::ApplyExpandMigration,
            &migration_workload_id,
            plan,
            Some(ExtractionExpandMigration {
                migration_id: artifact.migration_id.clone(),
                source_reference: artifact.source_reference.clone(),
                source_digest: artifact.source_digest.clone(),
                sql: artifact.sql.clone(),
            }),
        )?);
    }
    let next_order =
        u16::try_from(operations.len() + 1).map_err(|_| too_many_operations_error())?;
    let health_order = next_order
        .checked_add(1)
        .ok_or_else(too_many_operations_error)?;
    operations.push(operation(
        next_order,
        ExtractionExpansionOperationKind::VerifyMigrationWorkload,
        &migration_workload_id,
        plan,
        None,
    )?);
    operations.push(operation(
        health_order,
        ExtractionExpansionOperationKind::VerifyCandidateHealth,
        &api_workload_id,
        plan,
        None,
    )?);
    Ok((operations, migration_workload_id, api_workload_id))
}

fn too_many_operations_error() -> ExtractionRunStartError {
    start_error(
        ExtractionRunStartErrorCode::MigrationArtifactUnexpected,
        "Too many destination operations were supplied for one Extraction Run.",
        "Split the Module migration set into a reviewable Extraction Plan.",
    )
}

pub async fn advance_destination_expansion(
    mut run: ExtractionRun,
    current_inputs: &ExtractionPlanInputs,
    workload: &dyn ExtractionExpansionWorkload,
) -> Result<ExtractionRun, ExtractionRunAdvanceError> {
    if !extraction_run_integrity_is_valid(&run) {
        return Err(advance_error(
            ExtractionRunAdvanceErrorCode::RunInvalid,
            "Extraction Run integrity validation failed before Workload behavior was invoked.",
            "Discard the changed Run and resume from the last integrity-valid revision.",
        ));
    }
    if run.mode == ExtractionRunMode::DryRun {
        return Err(advance_error(
            ExtractionRunAdvanceErrorCode::DryRunCannotAdvance,
            "A dry-run Extraction Run cannot execute destination operations.",
            "Start an apply Run from the same fresh inputs after review.",
        ));
    }
    if run.current_phase.status == ExtractionRunStatus::Succeeded {
        return Ok(run);
    }
    if let Err(rejection) = ensure_extraction_plan_fresh(&run.plan, current_inputs) {
        block_run(
            &mut run,
            None,
            ExtractionRunErrorCode::PlanStale,
            rejection.message,
            Vec::new(),
            rejection.next_actions,
        );
        return Ok(run);
    }
    let Some(operation) = next_unreceipted_operation(&run).cloned() else {
        finish_run(&mut run);
        return Ok(run);
    };
    for error in &mut run.errors {
        if error.operation_id.as_deref() == Some(operation.operation_id.as_str()) {
            error.resolved = true;
        }
    }
    let request = ExtractionWorkloadRequest {
        run_id: run.run_id.clone(),
        plan_id: run.plan.plan_id.clone(),
        plan_digest: run.plan.plan_digest.clone(),
        expected_state: run.expected_state.clone(),
        expected_state_digest: run.expected_state_digest.clone(),
        operation: operation.clone(),
    };
    let inspected = match workload.inspect_receipt(&request).await {
        Ok(receipt) => receipt,
        Err(failure) => {
            record_workload_failure(&mut run, &operation, failure);
            return Ok(run);
        }
    };
    let receipt = match inspected {
        Some(receipt) => receipt,
        None => match workload.execute(&request).await {
            Ok(receipt) => receipt,
            Err(failure) => {
                record_workload_failure(&mut run, &operation, failure);
                return Ok(run);
            }
        },
    };
    record_destination_expansion_receipt(run, receipt)
}

pub fn record_destination_expansion_receipt(
    mut run: ExtractionRun,
    mut receipt: ExtractionOperationReceipt,
) -> Result<ExtractionRun, ExtractionRunAdvanceError> {
    if !extraction_run_integrity_is_valid(&run) {
        return Err(advance_error(
            ExtractionRunAdvanceErrorCode::RunInvalid,
            "Extraction Run integrity validation failed before recording a receipt.",
            "Resume from the last integrity-valid Run revision.",
        ));
    }
    let Some(operation) = next_unreceipted_operation(&run).cloned() else {
        return Ok(run);
    };
    receipt.evidence.sort();
    receipt.evidence.dedup();
    if let Err((code, message, next_action)) = validate_receipt(&run, &operation, &receipt) {
        block_run(
            &mut run,
            Some(operation.operation_id),
            code,
            message,
            Vec::new(),
            vec![next_action],
        );
        return Ok(run);
    }
    run.effects.invokes_candidate_workload_behavior = true;
    match (operation.kind, receipt.outcome) {
        (
            ExtractionExpansionOperationKind::CreateIsolatedStore,
            ExtractionOperationOutcome::Created,
        ) => run.effects.creates_destination_store = true,
        (
            ExtractionExpansionOperationKind::ApplyExpandMigration,
            ExtractionOperationOutcome::Applied,
        ) => run.effects.applies_destination_schema = true,
        _ => {}
    }
    run.evidence.extend(receipt.evidence.iter().cloned());
    run.evidence.sort();
    run.evidence.dedup();
    run.current_phase
        .completed_operation_ids
        .push(operation.operation_id.clone());
    run.receipts.push(receipt);
    run.current_phase.status = ExtractionRunStatus::InProgress;
    run.current_phase.next_operation_id =
        next_unreceipted_operation(&run).map(|operation| operation.operation_id.clone());
    run.next_actions = if let Some(next) = &run.current_phase.next_operation_id {
        vec![format!(
            "Persist this Run revision, then advance operation `{next}`."
        )]
    } else {
        vec![
            "Persist the successful destination expansion evidence before starting backfill."
                .to_owned(),
        ]
    };
    run.revision += 1;
    if run.current_phase.next_operation_id.is_none() {
        run.current_phase.status = ExtractionRunStatus::Succeeded;
    }
    refresh_run_digest(&mut run);
    Ok(run)
}

pub fn build_extraction_operation_receipt(
    request: &ExtractionWorkloadRequest,
    outcome: ExtractionOperationOutcome,
    mut evidence: Vec<ExtractionRunEvidence>,
) -> Result<ExtractionOperationReceipt, ExtractionRunStartError> {
    evidence.sort();
    evidence.dedup();
    let mut receipt = ExtractionOperationReceipt {
        protocol: EXTRACTION_OPERATION_RECEIPT_PROTOCOL.to_owned(),
        receipt_id: String::new(),
        receipt_digest: String::new(),
        run_id: request.run_id.clone(),
        plan_id: request.plan_id.clone(),
        plan_digest: request.plan_digest.clone(),
        expected_state_digest: request.expected_state_digest.clone(),
        operation_id: request.operation.operation_id.clone(),
        operation_digest: request.operation.operation_digest.clone(),
        operation_kind: request.operation.kind,
        workload_id: request.operation.workload_id.clone(),
        candidate_service_id: request.expected_state.candidate_service_id.clone(),
        destination_store_id: request.expected_state.destination_store_id.clone(),
        outcome,
        source_authority: request.expected_state.source_authority.clone(),
        source_store_unchanged: true,
        linked_authority_remains_authoritative: true,
        candidate_authoritative: false,
        evidence,
    };
    let digest = receipt_digest(&receipt)?;
    receipt.receipt_id = format!("extraction-operation-receipt:{digest}");
    receipt.receipt_digest = digest;
    Ok(receipt)
}

#[must_use]
pub fn extraction_operation_receipt_integrity_is_valid(
    receipt: &ExtractionOperationReceipt,
) -> bool {
    receipt.protocol == EXTRACTION_OPERATION_RECEIPT_PROTOCOL
        && receipt.receipt_id == format!("extraction-operation-receipt:{}", receipt.receipt_digest)
        && receipt_digest(receipt).is_ok_and(|digest| digest == receipt.receipt_digest)
}

#[must_use]
pub fn extraction_run_integrity_is_valid(run: &ExtractionRun) -> bool {
    if run.protocol != EXTRACTION_RUN_PROTOCOL
        || !extraction_plan_integrity_is_valid(&run.plan)
        || run.current_phase.phase_id != DESTINATION_EXPANSION_PHASE_ID
        || run.current_phase.kind != ExtractionPlanPhaseKind::DestinationExpansion
        || run.expected_state.plan_id != run.plan.plan_id
        || run.expected_state.plan_digest != run.plan.plan_digest
        || run.expected_state.source_authority != run.plan.expected_authority
        || run.expected_state.candidate_service_id != run.plan.proposed_service.service_id
        || run.expected_state.destination_store_id != run.plan.proposed_service.store.store_id
        || run.expected_state.destination_store_engine != run.plan.proposed_service.store.engine
        || !run.expected_state.destination_store_isolated
        || !run.expected_state.linked_authority_remains_authoritative
        || !run.expected_state.source_store_remains_unchanged
        || run.effects.copies_service_data
        || run.effects.mutates_source_store
        || run.effects.mutates_linked_implementation
        || run.effects.changes_authority
        || run.effects.performs_destructive_cleanup
        || !digest_serializable(&run.expected_state)
            .is_ok_and(|digest| digest == run.expected_state_digest)
        || !digest_serializable(&run.ordered_operations)
            .is_ok_and(|digest| digest == run.expected_state.ordered_operations_digest)
    {
        return false;
    }
    let identity = digest_serializable(&(
        run.plan.plan_id.as_str(),
        DESTINATION_EXPANSION_PHASE_ID,
        run.expected_state_digest.as_str(),
    ));
    if !identity.is_ok_and(|digest| run.run_id == format!("extraction-run:{digest}")) {
        return false;
    }
    let operation_ids = run
        .ordered_operations
        .iter()
        .map(|operation| operation.operation_id.as_str())
        .collect::<BTreeSet<_>>();
    if operation_ids.len() != run.ordered_operations.len()
        || run
            .ordered_operations
            .iter()
            .enumerate()
            .any(|(index, operation)| {
                operation.order != u16::try_from(index + 1).unwrap_or(u16::MAX)
                    || operation.mutating != operation.kind.is_mutating()
                    || !operation_digest(operation)
                        .is_ok_and(|digest| digest == operation.operation_digest)
            })
    {
        return false;
    }
    if run.receipts.iter().any(|receipt| {
        let operation = run
            .ordered_operations
            .iter()
            .find(|operation| operation.operation_id == receipt.operation_id);
        operation.is_none_or(|operation| validate_receipt(run, operation, receipt).is_err())
    }) {
        return false;
    }
    let completed = run
        .receipts
        .iter()
        .map(|receipt| receipt.operation_id.as_str())
        .collect::<Vec<_>>();
    let expected_completed = run
        .ordered_operations
        .iter()
        .take(completed.len())
        .map(|operation| operation.operation_id.as_str())
        .collect::<Vec<_>>();
    if completed
        != run
            .current_phase
            .completed_operation_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
        || completed != expected_completed
    {
        return false;
    }
    let expected_next = run
        .ordered_operations
        .get(completed.len())
        .map(|operation| operation.operation_id.as_str());
    if run.current_phase.next_operation_id.as_deref() != expected_next
        || (expected_next.is_none() && run.current_phase.status != ExtractionRunStatus::Succeeded)
        || (expected_next.is_some() && run.current_phase.status == ExtractionRunStatus::Succeeded)
    {
        return false;
    }
    run_digest(run).is_ok_and(|digest| digest == run.run_digest)
}

pub fn extraction_run_json(run: &ExtractionRun) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(run).map(|value| format!("{value}\n"))
}

#[must_use]
pub fn extraction_run_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ExtractionRun))
        .expect("Extraction Run schema must serialize");
    schema["$id"] = Value::String(EXTRACTION_RUN_SCHEMA_ID.to_owned());
    schema["title"] = Value::String("Lenso Extraction Run v1".to_owned());
    schema["properties"]["protocol"] = json!({
        "type": "string",
        "const": EXTRACTION_RUN_PROTOCOL
    });
    schema["properties"]["runId"] = json!({
        "type": "string",
        "pattern": "^extraction-run:sha256:[0-9a-f]{64}$"
    });
    schema["properties"]["runDigest"] = json!({
        "type": "string",
        "pattern": "^sha256:[0-9a-f]{64}$"
    });
    schema
}

#[must_use]
pub fn render_extraction_run(run: &ExtractionRun) -> String {
    let mut output = vec![
        format!("Extraction Run: {}", run.run_id),
        format!("Plan: {}", run.plan.plan_id),
        format!("Mode: {:?}", run.mode).to_lowercase(),
        format!(
            "Phase: {} ({:?})",
            run.current_phase.phase_id, run.current_phase.status
        )
        .to_lowercase(),
        format!(
            "Authority: {:?}:{}@{} (unchanged)",
            run.expected_state.source_authority.kind,
            run.expected_state.source_authority.owner_id,
            run.expected_state.source_authority.revision
        )
        .to_lowercase(),
        format!(
            "Candidate Store: {} ({}, isolated={})",
            run.expected_state.destination_store_id,
            run.expected_state.destination_store_engine,
            run.expected_state.destination_store_isolated
        ),
        String::new(),
        "Ordered operations:".to_owned(),
    ];
    output.extend(run.ordered_operations.iter().map(|operation| {
        let completed = run
            .current_phase
            .completed_operation_ids
            .contains(&operation.operation_id);
        format!(
            "- {:02} [{}] {:?}: {}",
            operation.order,
            if completed { "done" } else { "pending" },
            operation.kind,
            operation.operation_id
        )
        .to_lowercase()
    }));
    if !run.errors.is_empty() {
        output.push(String::new());
        output.push("Errors:".to_owned());
        output.extend(run.errors.iter().map(|error| {
            format!(
                "- {:?}: {}{}",
                error.code,
                error.message,
                if error.resolved { " (resolved)" } else { "" }
            )
            .to_lowercase()
        }));
    }
    output.push(String::new());
    output.push("Next actions:".to_owned());
    output.extend(run.next_actions.iter().map(|action| format!("- {action}")));
    format!("{}\n", output.join("\n"))
}

#[must_use]
pub fn validate_expand_first_postgres_sql(sql: &str) -> bool {
    let mut source = String::new();
    for line in sql.lines() {
        let code = line.split("--").next().unwrap_or_default();
        if code.contains("/*") || code.contains("*/") {
            return false;
        }
        source.push_str(code);
        source.push('\n');
    }
    let statements = source
        .split(';')
        .map(|statement| statement.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|statement| !statement.is_empty())
        .map(|statement| statement.to_ascii_lowercase())
        .collect::<Vec<_>>();
    !statements.is_empty()
        && statements.iter().all(|statement| {
            let forbidden = [
                " drop ",
                " truncate ",
                " delete ",
                " update ",
                " insert ",
                " rename ",
                " alter column ",
                " set schema ",
            ];
            let padded = format!(" {statement} ");
            !forbidden.iter().any(|token| padded.contains(token))
                && (statement.starts_with("create schema ")
                    || statement.starts_with("create table ")
                    || statement.starts_with("create index ")
                    || statement.starts_with("create unique index ")
                    || statement.starts_with("create type ")
                    || statement.starts_with("create extension ")
                    || statement.starts_with("comment on ")
                    || (statement.starts_with("alter table ")
                        && (statement.contains(" add column ")
                            || statement.contains(" add constraint "))))
        })
}

fn validate_start_inputs(inputs: &ExtractionRunInputs) -> Result<(), ExtractionRunStartError> {
    let plan = &inputs.plan;
    if !extraction_plan_integrity_is_valid(plan) {
        return Err(start_error(
            ExtractionRunStartErrorCode::PlanInvalid,
            "Extraction Plan integrity validation failed before destination expansion.",
            "Regenerate the exact content-addressed Extraction Plan.",
        ));
    }
    ensure_extraction_plan_fresh(plan, &inputs.current_plan_inputs).map_err(|rejection| {
        ExtractionRunStartError {
            code: ExtractionRunStartErrorCode::PlanStale,
            message: rejection.message,
            next_actions: rejection.next_actions,
            effects: ExtractionRunEffects::default(),
        }
    })?;
    let phase = plan
        .phases
        .iter()
        .find(|phase| phase.phase_id == DESTINATION_EXPANSION_PHASE_ID);
    if phase.is_none_or(|phase| phase.kind != ExtractionPlanPhaseKind::DestinationExpansion) {
        return Err(start_error(
            ExtractionRunStartErrorCode::PhaseInvalid,
            "The exact Extraction Plan does not contain the destination expansion phase.",
            "Regenerate the plan with the supported ordered phase protocol.",
        ));
    }
    if !extraction_scaffold_integrity_is_valid(&inputs.scaffold)
        || !validate_extraction_scaffold(&inputs.scaffold).is_empty()
        || inputs.scaffold.plan_id != plan.plan_id
        || inputs.scaffold.plan_digest != plan.plan_digest
    {
        return Err(start_error(
            ExtractionRunStartErrorCode::ScaffoldInvalid,
            "The candidate scaffold does not match the exact Extraction Plan.",
            "Regenerate and apply the identity-preserving scaffold from this plan.",
        ));
    }
    let mut applied = inputs
        .scaffold_apply_result
        .created_files
        .iter()
        .chain(&inputs.scaffold_apply_result.unchanged_files)
        .cloned()
        .collect::<Vec<_>>();
    applied.sort();
    applied.dedup();
    let mut expected = inputs
        .scaffold
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    expected.sort();
    if inputs.scaffold_apply_result.protocol != "lenso.extraction-scaffold-apply.v1"
        || inputs.scaffold_apply_result.scaffold_id != inputs.scaffold.scaffold_id
        || inputs.scaffold_apply_result.plan_id != plan.plan_id
        || !inputs
            .scaffold_apply_result
            .linked_authority_remains_authoritative
        || inputs.scaffold_apply_result.effects.starts_workloads
        || inputs.scaffold_apply_result.effects.copies_data
        || inputs.scaffold_apply_result.effects.changes_authority
        || inputs.scaffold_apply_result.effects.changes_provider_path
        || applied != expected
    {
        return Err(start_error(
            ExtractionRunStartErrorCode::ScaffoldNotApplied,
            "The complete candidate scaffold has not been applied idempotently.",
            "Apply every plan-owned scaffold file without changing linked authority.",
        ));
    }
    if plan.proposed_service.store.engine != "postgres"
        || plan.data_mapping.store_engine != "postgres"
    {
        return Err(start_error(
            ExtractionRunStartErrorCode::UnsupportedStoreEngine,
            "Destination expansion currently supports Postgres Service Stores only.",
            "Use Postgres or block the phase until the Store has equivalent safety semantics.",
        ));
    }
    if !plan.proposed_service.store.isolated
        || plan.proposed_service.store.store_id != plan.data_mapping.destination_store
    {
        return Err(start_error(
            ExtractionRunStartErrorCode::StoreNotIsolated,
            "The candidate destination Store is not isolated and plan-owned.",
            "Generate one isolated Store owned only by the candidate Autonomous Service.",
        ));
    }
    validate_migration_artifacts(inputs)
}

fn validate_migration_artifacts(
    inputs: &ExtractionRunInputs,
) -> Result<(), ExtractionRunStartError> {
    let mappings = inputs
        .plan
        .data_mapping
        .migrations
        .iter()
        .map(|mapping| (mapping.source_migration.as_str(), mapping))
        .collect::<BTreeMap<_, _>>();
    let artifacts = inputs
        .migrations
        .iter()
        .map(|artifact| (artifact.migration_id.as_str(), artifact))
        .collect::<BTreeMap<_, _>>();
    if mappings.len() != inputs.plan.data_mapping.migrations.len()
        || artifacts.len() != inputs.migrations.len()
    {
        return Err(start_error(
            ExtractionRunStartErrorCode::MigrationArtifactUnexpected,
            "Migration identities must be unique within one destination expansion phase.",
            "Rename or deduplicate the migrations, then generate a new Extraction Plan.",
        ));
    }
    let missing = mappings
        .keys()
        .filter(|migration| !artifacts.contains_key(**migration))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(start_error(
            ExtractionRunStartErrorCode::MigrationArtifactMissing,
            format!(
                "Plan-owned migration artifacts are missing: {}.",
                missing.join(", ")
            ),
            "Supply the exact digest-pinned source migrations from the target Module.",
        ));
    }
    let unexpected = artifacts
        .keys()
        .filter(|migration| !mappings.contains_key(**migration))
        .copied()
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        return Err(start_error(
            ExtractionRunStartErrorCode::MigrationArtifactUnexpected,
            format!(
                "Unplanned migration artifacts were supplied: {}.",
                unexpected.join(", ")
            ),
            "Remove every migration not pinned by the exact Extraction Plan.",
        ));
    }
    for (migration_id, mapping) in mappings {
        let artifact = artifacts[migration_id];
        if artifact.source_reference != mapping.source_reference
            || artifact.source_digest != mapping.source_digest
            || extraction_input_digest(artifact.sql.as_bytes()) != artifact.source_digest
        {
            return Err(start_error(
                ExtractionRunStartErrorCode::MigrationArtifactChanged,
                format!("Migration `{migration_id}` changed after plan approval."),
                "Regenerate readiness evidence and the Extraction Plan from the current migration content.",
            ));
        }
        if !validate_expand_first_postgres_sql(&artifact.sql) {
            return Err(start_error(
                ExtractionRunStartErrorCode::MigrationNotExpandFirst,
                format!(
                    "Migration `{migration_id}` contains a non-expand or data-mutating Postgres statement."
                ),
                "Split the migration so destination expansion contains only additive schema statements.",
            ));
        }
    }
    Ok(())
}

fn workload_id(
    plan: &ExtractionPlan,
    role: ExtractionWorkloadRole,
) -> Result<String, ExtractionRunStartError> {
    let matches = plan
        .proposed_service
        .workloads
        .iter()
        .filter(|workload| workload.role == role)
        .collect::<Vec<_>>();
    let [workload] = matches.as_slice() else {
        return Err(start_error(
            ExtractionRunStartErrorCode::WorkloadMissing,
            format!("The candidate must declare exactly one {role:?} Workload."),
            "Regenerate the candidate Service with API, Worker, and Migration Workloads.",
        ));
    };
    Ok(workload.workload_id.clone())
}

fn operation(
    order: u16,
    kind: ExtractionExpansionOperationKind,
    workload_id: &str,
    plan: &ExtractionPlan,
    migration: Option<ExtractionExpandMigration>,
) -> Result<ExtractionExpansionOperation, ExtractionRunStartError> {
    let label = match kind {
        ExtractionExpansionOperationKind::CreateIsolatedStore => "create-isolated-store".to_owned(),
        ExtractionExpansionOperationKind::ApplyExpandMigration => format!(
            "apply-expand-migration-{}",
            migration
                .as_ref()
                .map_or("missing", |migration| migration.migration_id.as_str())
        ),
        ExtractionExpansionOperationKind::VerifyMigrationWorkload => {
            "verify-migration-workload".to_owned()
        }
        ExtractionExpansionOperationKind::VerifyCandidateHealth => {
            "verify-candidate-health".to_owned()
        }
    };
    let mut operation = ExtractionExpansionOperation {
        operation_id: format!("{DESTINATION_EXPANSION_PHASE_ID}/{order:02}-{label}"),
        operation_digest: String::new(),
        order,
        kind,
        workload_id: workload_id.to_owned(),
        candidate_service_id: plan.proposed_service.service_id.clone(),
        destination_store_id: plan.proposed_service.store.store_id.clone(),
        mutating: kind.is_mutating(),
        migration,
    };
    operation.operation_digest = operation_digest(&operation)?;
    Ok(operation)
}

fn validate_receipt(
    run: &ExtractionRun,
    operation: &ExtractionExpansionOperation,
    receipt: &ExtractionOperationReceipt,
) -> Result<(), (ExtractionRunErrorCode, String, String)> {
    if !extraction_operation_receipt_integrity_is_valid(receipt)
        || receipt.run_id != run.run_id
        || receipt.plan_id != run.plan.plan_id
        || receipt.plan_digest != run.plan.plan_digest
        || receipt.expected_state_digest != run.expected_state_digest
        || receipt.operation_id != operation.operation_id
        || receipt.operation_digest != operation.operation_digest
        || receipt.operation_kind != operation.kind
        || receipt.workload_id != operation.workload_id
        || receipt.candidate_service_id != run.expected_state.candidate_service_id
        || receipt.destination_store_id != run.expected_state.destination_store_id
        || receipt.evidence.is_empty()
        || !outcome_matches(operation.kind, receipt.outcome)
    {
        return Err((
            ExtractionRunErrorCode::ReceiptInvalid,
            format!(
                "Operation `{}` returned a receipt that is not bound to the exact plan and expected state.",
                operation.operation_id
            ),
            "Inspect the candidate Workload receipt store and retry only with the exact operation identity."
                .to_owned(),
        ));
    }
    if receipt.source_authority != run.expected_state.source_authority {
        return Err((
            ExtractionRunErrorCode::AuthorityChanged,
            "Linked source authority changed during destination expansion.".to_owned(),
            "Stop preparation, refresh authority evidence, and generate a new Extraction Plan."
                .to_owned(),
        ));
    }
    if !receipt.source_store_unchanged
        || !receipt.linked_authority_remains_authoritative
        || receipt.candidate_authoritative
    {
        return Err((
            ExtractionRunErrorCode::SourceMutationReported,
            "A Workload receipt did not preserve the source Store and linked authority invariants."
                .to_owned(),
            "Stop extraction and inspect the source before any further candidate operation."
                .to_owned(),
        ));
    }
    Ok(())
}

fn outcome_matches(
    kind: ExtractionExpansionOperationKind,
    outcome: ExtractionOperationOutcome,
) -> bool {
    matches!(
        (kind, outcome),
        (
            ExtractionExpansionOperationKind::CreateIsolatedStore,
            ExtractionOperationOutcome::Created | ExtractionOperationOutcome::AlreadyExists
        ) | (
            ExtractionExpansionOperationKind::ApplyExpandMigration,
            ExtractionOperationOutcome::Applied | ExtractionOperationOutcome::AlreadyApplied
        ) | (
            ExtractionExpansionOperationKind::VerifyMigrationWorkload
                | ExtractionExpansionOperationKind::VerifyCandidateHealth,
            ExtractionOperationOutcome::Healthy
        )
    )
}

fn record_workload_failure(
    run: &mut ExtractionRun,
    operation: &ExtractionExpansionOperation,
    failure: ExtractionWorkloadFailure,
) {
    let code = match failure.code {
        ExtractionWorkloadFailureCode::Unavailable => ExtractionRunErrorCode::WorkloadUnavailable,
        ExtractionWorkloadFailureCode::StoreProvisioningFailed => {
            ExtractionRunErrorCode::StoreProvisioningFailed
        }
        ExtractionWorkloadFailureCode::MigrationFailed => ExtractionRunErrorCode::MigrationFailed,
        ExtractionWorkloadFailureCode::MigrationWorkloadUnhealthy => {
            ExtractionRunErrorCode::MigrationWorkloadUnhealthy
        }
        ExtractionWorkloadFailureCode::CandidateUnhealthy => {
            ExtractionRunErrorCode::CandidateUnhealthy
        }
    };
    block_run(
        run,
        Some(operation.operation_id.clone()),
        code,
        failure.message,
        failure.evidence,
        failure.next_actions,
    );
}

fn block_run(
    run: &mut ExtractionRun,
    operation_id: Option<String>,
    code: ExtractionRunErrorCode,
    message: impl Into<String>,
    evidence: Vec<ExtractionRunEvidence>,
    next_actions: Vec<String>,
) {
    let sequence = u32::try_from(run.errors.len() + 1).unwrap_or(u32::MAX);
    run.errors.push(ExtractionRunError {
        sequence,
        operation_id,
        code,
        message: message.into(),
        evidence,
        next_actions: next_actions.clone(),
        resolved: false,
    });
    run.current_phase.status = ExtractionRunStatus::Blocked;
    run.next_actions = next_actions;
    run.revision += 1;
    refresh_run_digest(run);
}

fn finish_run(run: &mut ExtractionRun) {
    run.current_phase.status = ExtractionRunStatus::Succeeded;
    run.current_phase.next_operation_id = None;
    run.next_actions = vec![
        "Persist the successful destination expansion evidence before starting backfill."
            .to_owned(),
    ];
    run.revision += 1;
    refresh_run_digest(run);
}

fn next_unreceipted_operation(run: &ExtractionRun) -> Option<&ExtractionExpansionOperation> {
    let completed = run
        .receipts
        .iter()
        .map(|receipt| receipt.operation_id.as_str())
        .collect::<BTreeSet<_>>();
    run.ordered_operations
        .iter()
        .find(|operation| !completed.contains(operation.operation_id.as_str()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationContent<'a> {
    operation_id: &'a str,
    order: u16,
    kind: ExtractionExpansionOperationKind,
    workload_id: &'a str,
    candidate_service_id: &'a str,
    destination_store_id: &'a str,
    mutating: bool,
    migration: &'a Option<ExtractionExpandMigration>,
}

fn operation_digest(
    operation: &ExtractionExpansionOperation,
) -> Result<String, ExtractionRunStartError> {
    digest_serializable(&OperationContent {
        operation_id: &operation.operation_id,
        order: operation.order,
        kind: operation.kind,
        workload_id: &operation.workload_id,
        candidate_service_id: &operation.candidate_service_id,
        destination_store_id: &operation.destination_store_id,
        mutating: operation.mutating,
        migration: &operation.migration,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptContent<'a> {
    protocol: &'a str,
    run_id: &'a str,
    plan_id: &'a str,
    plan_digest: &'a str,
    expected_state_digest: &'a str,
    operation_id: &'a str,
    operation_digest: &'a str,
    operation_kind: ExtractionExpansionOperationKind,
    workload_id: &'a str,
    candidate_service_id: &'a str,
    destination_store_id: &'a str,
    outcome: ExtractionOperationOutcome,
    source_authority: &'a ExtractionExpectedAuthority,
    source_store_unchanged: bool,
    linked_authority_remains_authoritative: bool,
    candidate_authoritative: bool,
    evidence: &'a [ExtractionRunEvidence],
}

fn receipt_digest(receipt: &ExtractionOperationReceipt) -> Result<String, ExtractionRunStartError> {
    digest_serializable(&ReceiptContent {
        protocol: &receipt.protocol,
        run_id: &receipt.run_id,
        plan_id: &receipt.plan_id,
        plan_digest: &receipt.plan_digest,
        expected_state_digest: &receipt.expected_state_digest,
        operation_id: &receipt.operation_id,
        operation_digest: &receipt.operation_digest,
        operation_kind: receipt.operation_kind,
        workload_id: &receipt.workload_id,
        candidate_service_id: &receipt.candidate_service_id,
        destination_store_id: &receipt.destination_store_id,
        outcome: receipt.outcome,
        source_authority: &receipt.source_authority,
        source_store_unchanged: receipt.source_store_unchanged,
        linked_authority_remains_authoritative: receipt.linked_authority_remains_authoritative,
        candidate_authoritative: receipt.candidate_authoritative,
        evidence: &receipt.evidence,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunContent<'a> {
    protocol: &'a str,
    run_id: &'a str,
    revision: u64,
    mode: ExtractionRunMode,
    plan: &'a ExtractionPlan,
    current_phase: &'a ExtractionRunPhase,
    expected_state: &'a ExtractionRunExpectedState,
    expected_state_digest: &'a str,
    ordered_operations: &'a [ExtractionExpansionOperation],
    receipts: &'a [ExtractionOperationReceipt],
    evidence: &'a [ExtractionRunEvidence],
    errors: &'a [ExtractionRunError],
    next_actions: &'a [String],
    effects: ExtractionRunEffects,
}

fn run_digest(run: &ExtractionRun) -> Result<String, ExtractionRunStartError> {
    digest_serializable(&RunContent {
        protocol: &run.protocol,
        run_id: &run.run_id,
        revision: run.revision,
        mode: run.mode,
        plan: &run.plan,
        current_phase: &run.current_phase,
        expected_state: &run.expected_state,
        expected_state_digest: &run.expected_state_digest,
        ordered_operations: &run.ordered_operations,
        receipts: &run.receipts,
        evidence: &run.evidence,
        errors: &run.errors,
        next_actions: &run.next_actions,
        effects: run.effects,
    })
}

fn refresh_run_digest(run: &mut ExtractionRun) {
    run.run_digest = run_digest(run).expect("Extraction Run content must serialize");
}

fn digest_serializable(value: &impl Serialize) -> Result<String, ExtractionRunStartError> {
    serde_json::to_vec(value)
        .map(extraction_input_digest)
        .map_err(|error| {
            start_error(
                ExtractionRunStartErrorCode::PlanInvalid,
                format!("Extraction Run content could not be serialized: {error}"),
                "Correct the public artifact input and retry without mutation.",
            )
        })
}

fn start_error(
    code: ExtractionRunStartErrorCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ExtractionRunStartError {
    ExtractionRunStartError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
        effects: ExtractionRunEffects::default(),
    }
}

fn advance_error(
    code: ExtractionRunAdvanceErrorCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ExtractionRunAdvanceError {
    ExtractionRunAdvanceError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CommonContextRequirement, DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML,
        EXTRACTION_READINESS_ANALYZER_VERSION, EXTRACTION_READINESS_REPORT_PROTOCOL,
        ExtractionAuthorityKind, ExtractionContractArtifactFormat, ExtractionContractDirection,
        ExtractionContractKind, ExtractionEvidenceDigest, ExtractionPlanContractVersion,
        ExtractionReadinessEffects, ExtractionReadinessReport, ExtractionReadinessSurfaceSummary,
        ExtractionScaffoldArtifact, ExtractionScaffoldEffects, ExtractionScaffoldInputs,
        ExtractionServiceDataEvidence, ServiceTenancyMode, generate_extraction_plan,
        generate_extraction_scaffold,
    };
    use lenso_contracts::{
        ModuleHttpMethod, ModuleHttpRoute, ModuleManifest, ServiceOperationMetadata,
    };
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct FakeWorkload {
        receipts: Mutex<BTreeMap<String, ExtractionOperationReceipt>>,
        executions: Mutex<Vec<String>>,
    }

    impl FakeWorkload {
        fn execution_count(&self) -> usize {
            self.executions.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl ExtractionExpansionWorkload for FakeWorkload {
        async fn inspect_receipt(
            &self,
            request: &ExtractionWorkloadRequest,
        ) -> Result<Option<ExtractionOperationReceipt>, ExtractionWorkloadFailure> {
            Ok(self
                .receipts
                .lock()
                .unwrap()
                .get(&request.operation.operation_id)
                .cloned())
        }

        async fn execute(
            &self,
            request: &ExtractionWorkloadRequest,
        ) -> Result<ExtractionOperationReceipt, ExtractionWorkloadFailure> {
            self.executions
                .lock()
                .unwrap()
                .push(request.operation.operation_id.clone());
            let (outcome, kind) = match request.operation.kind {
                ExtractionExpansionOperationKind::CreateIsolatedStore => (
                    ExtractionOperationOutcome::Created,
                    ExtractionRunEvidenceKind::StoreIsolation,
                ),
                ExtractionExpansionOperationKind::ApplyExpandMigration => (
                    ExtractionOperationOutcome::Applied,
                    ExtractionRunEvidenceKind::MigrationApplied,
                ),
                ExtractionExpansionOperationKind::VerifyMigrationWorkload => (
                    ExtractionOperationOutcome::Healthy,
                    ExtractionRunEvidenceKind::MigrationWorkloadHealth,
                ),
                ExtractionExpansionOperationKind::VerifyCandidateHealth => (
                    ExtractionOperationOutcome::Healthy,
                    ExtractionRunEvidenceKind::CandidateHealth,
                ),
            };
            let evidence = vec![ExtractionRunEvidence {
                kind,
                subject: request.operation.operation_id.clone(),
                digest: extraction_input_digest(request.operation.operation_digest.as_bytes()),
                detail: "verified through candidate Workload behavior".to_owned(),
            }];
            let receipt = build_extraction_operation_receipt(request, outcome, evidence)
                .expect("fake receipt must build");
            self.receipts
                .lock()
                .unwrap()
                .insert(request.operation.operation_id.clone(), receipt.clone());
            Ok(receipt)
        }
    }

    fn module() -> ModuleManifest {
        ModuleManifest::builder("support-ticket")
            .capabilities(vec!["support.tickets.read".to_owned()])
            .http_routes(vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/v1/tickets/{ticket_id}".to_owned(),
                capability: Some("support.tickets.read".to_owned()),
                display_name: Some("Get ticket".to_owned()),
                story_title: Some("Support ticket opened".to_owned()),
                operation: Some(ServiceOperationMetadata {
                    operation_id: Some("getTicket".to_owned()),
                    summary: Some("Get ticket".to_owned()),
                    ..ServiceOperationMetadata::default()
                }),
            }])
            .build()
    }

    #[allow(clippy::too_many_lines)]
    fn run_inputs(sql: &str) -> ExtractionRunInputs {
        let module = module();
        let migration_reference = "modules/support-ticket/migrations/0001_tickets.sql";
        let migration_digest = extraction_input_digest(sql.as_bytes());
        let report = ExtractionReadinessReport {
            protocol: EXTRACTION_READINESS_REPORT_PROTOCOL.to_owned(),
            analyzer_version: EXTRACTION_READINESS_ANALYZER_VERSION.to_owned(),
            target_module: module.name.clone(),
            system_id: Some("support-system".to_owned()),
            target_owner: Some("support-host".to_owned()),
            classification: crate::CompatibilityCategory::Safe,
            ready: true,
            issue_codes: Vec::new(),
            contract_evidence: Vec::new(),
            active_consumers: Vec::new(),
            surfaces: ExtractionReadinessSurfaceSummary::default(),
            service_data: ExtractionServiceDataEvidence {
                complete: true,
                migrations: vec![crate::ExtractionMigrationEvidence {
                    migration: "0001_create_support_tickets".to_owned(),
                    owner_module: Some("support-ticket".to_owned()),
                    source: crate::ExtractionDataEvidenceSource::StaticDeclaration,
                    evidence_references: vec![migration_reference.to_owned()],
                }],
                ..ExtractionServiceDataEvidence::default()
            },
            findings: Vec::new(),
            effects: ExtractionReadinessEffects::default(),
        };
        let current_plan_inputs = ExtractionPlanInputs {
            readiness_report: report,
            module: module.clone(),
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
                artifact_reference: "contracts/openapi/support.v1.yaml".to_owned(),
                artifact_digest: extraction_input_digest(
                    DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML.as_bytes(),
                ),
                artifact_format: ExtractionContractArtifactFormat::Openapi,
                tenancy_mode: ServiceTenancyMode::Required,
                required_context: vec![CommonContextRequirement::Tenant],
                producer_id: None,
                consumer_ids: Vec::new(),
            }],
            expected_authority: ExtractionExpectedAuthority {
                kind: ExtractionAuthorityKind::LinkedHost,
                owner_id: "support-host".to_owned(),
                revision: "support-authority-r7".to_owned(),
            },
            evidence_digests: vec![ExtractionEvidenceDigest {
                reference: migration_reference.to_owned(),
                digest: migration_digest.clone(),
            }],
        };
        let plan = generate_extraction_plan(&current_plan_inputs).expect("plan");
        let scaffold = generate_extraction_scaffold(&ExtractionScaffoldInputs {
            plan: plan.clone(),
            module,
            artifacts: vec![ExtractionScaffoldArtifact {
                contract_id: "support-ticket-http.v1".to_owned(),
                version: "v1".to_owned(),
                contents: DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML.to_owned(),
                protobuf_descriptor: None,
            }],
        })
        .expect("scaffold");
        let unchanged_files = scaffold
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect();
        let scaffold_apply_result = ExtractionScaffoldApplyResult {
            protocol: "lenso.extraction-scaffold-apply.v1".to_owned(),
            scaffold_id: scaffold.scaffold_id.clone(),
            plan_id: plan.plan_id.clone(),
            created_files: Vec::new(),
            unchanged_files,
            linked_authority_remains_authoritative: true,
            effects: ExtractionScaffoldEffects::default(),
        };
        ExtractionRunInputs {
            plan,
            current_plan_inputs,
            scaffold,
            scaffold_apply_result,
            migrations: vec![ExtractionMigrationArtifact {
                migration_id: "0001_create_support_tickets".to_owned(),
                source_reference: migration_reference.to_owned(),
                source_digest: migration_digest,
                sql: sql.to_owned(),
            }],
        }
    }

    fn safe_sql() -> &'static str {
        "create schema if not exists support;\ncreate table if not exists support.tickets (id text primary key);\n"
    }

    fn request_for(
        run: &ExtractionRun,
        operation: ExtractionExpansionOperation,
    ) -> ExtractionWorkloadRequest {
        ExtractionWorkloadRequest {
            run_id: run.run_id.clone(),
            plan_id: run.plan.plan_id.clone(),
            plan_digest: run.plan.plan_digest.clone(),
            expected_state: run.expected_state.clone(),
            expected_state_digest: run.expected_state_digest.clone(),
            operation,
        }
    }

    #[test]
    fn dry_run_reports_the_exact_apply_operations_without_effects() {
        let inputs = run_inputs(safe_sql());
        let apply = start_destination_expansion(&inputs).expect("apply run");
        let dry_run = dry_run_destination_expansion(&inputs).expect("dry run");

        assert_eq!(dry_run.run_id, apply.run_id);
        assert_eq!(dry_run.expected_state, apply.expected_state);
        assert_eq!(dry_run.ordered_operations, apply.ordered_operations);
        assert_eq!(dry_run.effects, ExtractionRunEffects::default());
        assert!(extraction_run_integrity_is_valid(&dry_run));
        assert_eq!(
            dry_run
                .ordered_operations
                .iter()
                .map(|operation| operation.kind)
                .collect::<Vec<_>>(),
            vec![
                ExtractionExpansionOperationKind::CreateIsolatedStore,
                ExtractionExpansionOperationKind::ApplyExpandMigration,
                ExtractionExpansionOperationKind::VerifyMigrationWorkload,
                ExtractionExpansionOperationKind::VerifyCandidateHealth,
            ]
        );
    }

    #[test]
    fn destructive_or_data_mutating_sql_is_rejected_before_workload_behavior() {
        let inputs = run_inputs("drop table support.tickets;");
        let error = start_destination_expansion(&inputs).expect_err("drop must fail closed");
        assert_eq!(
            error.code,
            ExtractionRunStartErrorCode::MigrationNotExpandFirst
        );
        assert_eq!(error.effects, ExtractionRunEffects::default());
        assert!(!validate_expand_first_postgres_sql(
            "alter table support.tickets drop column title;"
        ));
        assert!(!validate_expand_first_postgres_sql(
            "insert into support.tickets values ('x');"
        ));
    }

    #[tokio::test]
    async fn interrupted_run_recovers_the_workload_receipt_without_repeating_effects() {
        let inputs = run_inputs(safe_sql());
        let workload = FakeWorkload::default();
        let mut run = start_destination_expansion(&inputs).expect("run");

        run = advance_destination_expansion(run, &inputs.current_plan_inputs, &workload)
            .await
            .expect("create Store");
        assert_eq!(workload.execution_count(), 1);

        let migration = run
            .ordered_operations
            .iter()
            .find(|operation| {
                operation.kind == ExtractionExpansionOperationKind::ApplyExpandMigration
            })
            .cloned()
            .unwrap();
        workload
            .execute(&request_for(&run, migration))
            .await
            .expect("commit effect and durable receipt before simulated crash");
        assert_eq!(workload.execution_count(), 2);

        run = advance_destination_expansion(run, &inputs.current_plan_inputs, &workload)
            .await
            .expect("recover receipt");
        assert_eq!(workload.execution_count(), 2, "migration must not repeat");
        while run.current_phase.status != ExtractionRunStatus::Succeeded {
            run = advance_destination_expansion(run, &inputs.current_plan_inputs, &workload)
                .await
                .expect("advance remaining health checks");
        }

        assert_eq!(run.receipts.len(), run.ordered_operations.len());
        assert!(run.effects.creates_destination_store);
        assert!(run.effects.applies_destination_schema);
        assert!(!run.effects.copies_service_data);
        assert!(!run.effects.mutates_source_store);
        assert!(!run.effects.mutates_linked_implementation);
        assert!(!run.effects.changes_authority);
        assert!(!run.effects.performs_destructive_cleanup);
        assert!(extraction_run_integrity_is_valid(&run));
    }

    #[tokio::test]
    async fn stale_plan_blocks_before_workload_behavior() {
        let inputs = run_inputs(safe_sql());
        let workload = FakeWorkload::default();
        let run = start_destination_expansion(&inputs).expect("run");
        let mut changed = inputs.current_plan_inputs.clone();
        changed.expected_authority.revision = "support-authority-r8".to_owned();

        let blocked = advance_destination_expansion(run, &changed, &workload)
            .await
            .expect("stale plan becomes blocked evidence");
        assert_eq!(blocked.current_phase.status, ExtractionRunStatus::Blocked);
        assert_eq!(blocked.errors[0].code, ExtractionRunErrorCode::PlanStale);
        assert_eq!(workload.execution_count(), 0);
        assert!(extraction_run_integrity_is_valid(&blocked));
    }

    #[test]
    fn public_schema_accepts_a_versioned_run() {
        let run = dry_run_destination_expansion(&run_inputs(safe_sql())).expect("dry run");
        let value = serde_json::to_value(&run).unwrap();
        let validator = jsonschema::validator_for(&extraction_run_schema()).unwrap();
        assert!(validator.is_valid(&value));
    }
}
