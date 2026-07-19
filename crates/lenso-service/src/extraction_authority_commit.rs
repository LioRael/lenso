use crate::{
    ExtractionProvisionalCutoverRun, ExtractionProvisionalCutoverStatus, ExtractionQuiescenceRun,
    ExtractionQuiescenceStatus, ExtractionReconciliationResult, ExtractionReconciliationStatus,
    ExtractionVerificationResult, ExtractionVerificationStatus, extraction_input_digest,
    extraction_provisional_cutover_integrity_is_valid, extraction_quiescence_integrity_is_valid,
    extraction_reconciliation_integrity_is_valid, extraction_verification_integrity_is_valid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const EXTRACTION_AUTHORITY_COMMIT_PROTOCOL: &str = "lenso.extraction-authority-commit.v1";
pub const EXTRACTION_CANDIDATE_HEALTH_PROTOCOL: &str = "lenso.extraction-candidate-health.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionCandidateHealthEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub plan_id: String,
    pub candidate_service_id: String,
    pub endpoint: String,
    pub endpoint_reachable: bool,
    pub store_ready: bool,
    pub healthy: bool,
}

impl ExtractionCandidateHealthEvidence {
    #[must_use]
    pub fn bind(
        plan_id: impl Into<String>,
        candidate_service_id: impl Into<String>,
        endpoint: impl Into<String>,
        endpoint_reachable: bool,
        store_ready: bool,
    ) -> Self {
        let mut evidence = Self {
            protocol: EXTRACTION_CANDIDATE_HEALTH_PROTOCOL.to_owned(),
            evidence_id: String::new(),
            evidence_digest: String::new(),
            plan_id: plan_id.into(),
            candidate_service_id: candidate_service_id.into(),
            endpoint: endpoint.into(),
            endpoint_reachable,
            store_ready,
            healthy: endpoint_reachable && store_ready,
        };
        let identity = digest(&(
            evidence.plan_id.as_str(),
            evidence.candidate_service_id.as_str(),
            evidence.endpoint.as_str(),
        ));
        evidence.evidence_id = format!("extraction-candidate-health:{identity}");
        evidence.evidence_digest = candidate_health_digest(&evidence);
        evidence
    }
}

#[must_use]
pub fn extraction_candidate_health_integrity_is_valid(
    evidence: &ExtractionCandidateHealthEvidence,
) -> bool {
    evidence.protocol == EXTRACTION_CANDIDATE_HEALTH_PROTOCOL
        && !evidence.evidence_id.trim().is_empty()
        && !evidence.plan_id.trim().is_empty()
        && !evidence.candidate_service_id.trim().is_empty()
        && !evidence.endpoint.trim().is_empty()
        && evidence.healthy == (evidence.endpoint_reachable && evidence.store_ready)
        && evidence.evidence_digest == candidate_health_digest(evidence)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionApproval {
    pub approval_id: String,
    pub approval_digest: String,
    pub approver: String,
    pub authorized: bool,
    pub cutover_id: String,
    pub cutover_digest: String,
    pub plan_digest: String,
    pub authority_revision: String,
    pub destination_checkpoint: String,
    pub verification_digest: String,
    pub quiescence_digest: String,
    pub candidate_service_id: String,
    pub candidate_health_digest: String,
}

impl ExtractionApproval {
    #[must_use]
    pub fn bind(
        cutover: &ExtractionProvisionalCutoverRun,
        candidate_health: &ExtractionCandidateHealthEvidence,
        approval_id: impl Into<String>,
        approver: impl Into<String>,
        authorized: bool,
    ) -> Self {
        let mut approval = Self {
            approval_id: approval_id.into(),
            approval_digest: String::new(),
            approver: approver.into(),
            authorized,
            cutover_id: cutover.cutover_id.clone(),
            cutover_digest: cutover.cutover_digest.clone(),
            plan_digest: cutover.plan_digest.clone(),
            authority_revision: cutover.authority_revision.clone(),
            destination_checkpoint: cutover.destination_checkpoint.clone(),
            verification_digest: cutover.verification_digest.clone(),
            quiescence_digest: cutover.quiescence_digest.clone(),
            candidate_service_id: cutover.candidate_service_id.clone(),
            candidate_health_digest: candidate_health.evidence_digest.clone(),
        };
        approval.approval_digest = approval_digest(&approval);
        approval
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitInputs {
    pub cutover: ExtractionProvisionalCutoverRun,
    pub approval: ExtractionApproval,
    pub current_authority_revision: String,
    pub current_routing_revision: String,
    pub current_system_graph_revision: String,
    pub revalidation: ExtractionAuthorityCommitRevalidation,
}

pub trait ExtractionApprovalVerifier: Send + Sync {
    fn verify(&self, approval: &ExtractionApproval) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionTopologyState {
    pub authority_revision: String,
    pub routing_revision: String,
    pub system_graph_revision: String,
    pub authority_kind: String,
    pub owner_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitRevalidation {
    pub reconciliation: ExtractionReconciliationResult,
    pub verification: ExtractionVerificationResult,
    pub quiescence: ExtractionQuiescenceRun,
    pub candidate_health: ExtractionCandidateHealthEvidence,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionAuthorityCommitStatus {
    Committed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionAuthorityCommitErrorCode {
    ProvisionalVerificationIncomplete,
    ApprovalUnauthorized,
    ApprovalInvalid,
    ApprovalStale,
    AuthorityChanged,
    RoutingChanged,
    CandidateUnhealthy,
    FinalStateInvalid,
    ConcurrentStateChange,
    PersistenceFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitError {
    pub code: ExtractionAuthorityCommitErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
    pub mutation_started: bool,
}

impl fmt::Display for ExtractionAuthorityCommitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionAuthorityCommitError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitReceipt {
    pub receipt_id: String,
    pub receipt_digest: String,
    pub expected_authority_revision: String,
    pub expected_routing_revision: String,
    pub expected_system_graph_revision: String,
    pub committed_authority_revision: String,
    pub committed_routing_revision: String,
    pub committed_system_graph_revision: String,
    pub candidate_service_id: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionAuthorityCommitResult {
    pub protocol: String,
    pub commit_id: String,
    pub commit_digest: String,
    pub status: ExtractionAuthorityCommitStatus,
    pub plan_digest: String,
    pub approval: ExtractionApproval,
    pub authority_revision: String,
    pub routing_revision: String,
    pub system_graph_revision: String,
    pub candidate_service_id: String,
    pub candidate_authoritative: bool,
    pub linked_authoritative: bool,
    pub candidate_mutations_open: bool,
    pub linked_recovery_read_only: bool,
    pub source_cleanup_performed: bool,
    #[serde(default)]
    pub autonomous_mutation_ids: Vec<String>,
    pub fast_rollback_blocked: bool,
    pub business_execution_requires_runtime_console: bool,
    pub business_execution_requires_system_plane: bool,
    pub commit_receipts: Vec<ExtractionAuthorityCommitReceipt>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionFastRollbackIssueCode {
    ReverseMigrationEvidenceRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionFastRollbackError {
    pub code: ExtractionFastRollbackIssueCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionFastRollbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionFastRollbackError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReverseMigrationEvidence {
    pub plan_digest: String,
    pub reconciliation_digest: String,
    pub reviewed_by: String,
    pub approved: bool,
}

pub fn commit_extraction_authority(
    inputs: ExtractionAuthorityCommitInputs,
) -> Result<ExtractionAuthorityCommitResult, ExtractionAuthorityCommitError> {
    let cutover = &inputs.cutover;
    if !extraction_provisional_cutover_integrity_is_valid(cutover)
        || cutover.status != ExtractionProvisionalCutoverStatus::Verified
        || !cutover.external_mutations_paused
        || !cutover.linked_authoritative
        || cutover.candidate_authoritative
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ProvisionalVerificationIncomplete,
            "Provisional Cutover is not verified in a single-authority paused state.",
            "Repeat provisional verification before requesting approval.",
        ));
    }
    if !inputs.approval.authorized || inputs.approval.approver.trim().is_empty() {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalUnauthorized,
            "The Approval Boundary was not crossed by an authorized identity.",
            "Request approval from an authorized operator.",
        ));
    }
    if inputs.approval.approval_digest != approval_digest(&inputs.approval) {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalInvalid,
            "Approval integrity validation failed.",
            "Discard the changed approval and bind a new one.",
        ));
    }
    if inputs.approval.cutover_id != cutover.cutover_id
        || inputs.approval.cutover_digest != cutover.cutover_digest
        || inputs.approval.plan_digest != cutover.plan_digest
        || inputs.approval.authority_revision != cutover.authority_revision
        || inputs.approval.destination_checkpoint != cutover.destination_checkpoint
        || inputs.approval.verification_digest != cutover.verification_digest
        || inputs.approval.quiescence_digest != cutover.quiescence_digest
        || inputs.approval.candidate_service_id != cutover.candidate_service_id
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalStale,
            "Approval does not match the exact verified Cutover state.",
            "Bind a fresh approval to the current Cutover evidence.",
        ));
    }
    if inputs.current_authority_revision != cutover.authority_revision {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::AuthorityChanged,
            "Authority revision changed before commit.",
            "Regenerate Cutover evidence from the current authority revision.",
        ));
    }
    if inputs.current_routing_revision != cutover.routing_revision_current {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::RoutingChanged,
            "Routing revision changed before commit.",
            "Restore the verified provisional route or restart Cutover.",
        ));
    }
    let revalidation = &inputs.revalidation;
    let drain_complete = revalidation.quiescence.drain.as_ref().is_some_and(|drain| {
        drain.in_flight_requests == 0
            && drain.outbox_messages == 0
            && drain.inbox_messages == 0
            && drain.scheduled_functions == 0
            && drain.timers == 0
            && drain.durable_workflows == 0
            && drain.unresolved.is_empty()
            && !drain.timed_out
    });
    if !extraction_reconciliation_integrity_is_valid(&revalidation.reconciliation)
        || !extraction_verification_integrity_is_valid(&revalidation.verification)
        || !extraction_quiescence_integrity_is_valid(&revalidation.quiescence)
        || revalidation.reconciliation.plan_digest != cutover.plan_digest
        || revalidation.reconciliation.destination_checkpoint != cutover.destination_checkpoint
        || revalidation.verification.verification_digest != cutover.verification_digest
        || revalidation.verification.reconciliation_id
            != revalidation.reconciliation.reconciliation_id
        || revalidation.verification.reconciliation_digest
            != revalidation.reconciliation.reconciliation_digest
        || revalidation.quiescence.plan_digest != cutover.plan_digest
        || revalidation.quiescence.expected_authority_revision != cutover.authority_revision
        || revalidation.quiescence.destination_checkpoint.as_deref()
            != Some(cutover.destination_checkpoint.as_str())
        || revalidation.quiescence.quiescence_digest != cutover.quiescence_digest
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalStale,
            "Final revalidation pins do not match the approved Cutover state.",
            "Repeat final revalidation and bind a fresh approval.",
        ));
    }
    if revalidation.reconciliation.status != ExtractionReconciliationStatus::Matched
        || !revalidation.reconciliation.issues.is_empty()
        || revalidation.verification.status != ExtractionVerificationStatus::Verified
        || !revalidation.verification.issues.is_empty()
        || revalidation
            .verification
            .compatibility
            .iter()
            .any(|evidence| !evidence.compatible)
        || revalidation
            .verification
            .policy
            .iter()
            .any(|evidence| !evidence.passed)
        || revalidation.quiescence.status != ExtractionQuiescenceStatus::Quiesced
        || !revalidation.quiescence.linked_mutations_paused
        || !drain_complete
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::FinalStateInvalid,
            "Final Cutover safety revalidation failed before commit.",
            "Restore quiescence, drain, reconciliation, compatibility, and policy evidence.",
        ));
    }
    if !extraction_candidate_health_integrity_is_valid(&revalidation.candidate_health)
        || revalidation.candidate_health.plan_id != cutover.plan_id
        || revalidation.candidate_health.candidate_service_id != cutover.candidate_service_id
        || !revalidation.candidate_health.healthy
        || !cutover.candidate_healthy
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::CandidateUnhealthy,
            "Candidate health changed before commit.",
            "Restore candidate health and repeat verification.",
        ));
    }
    if inputs.approval.candidate_health_digest != revalidation.candidate_health.evidence_digest {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalStale,
            "Approval does not bind the persisted candidate health evidence.",
            "Probe candidate health and bind a fresh approval to that exact evidence.",
        ));
    }
    let commit_identity = digest(&(
        inputs.approval.approval_digest.as_str(),
        inputs.current_authority_revision.as_str(),
        inputs.current_routing_revision.as_str(),
        inputs.current_system_graph_revision.as_str(),
    ));
    let authority_revision = format!("autonomous-authority:{commit_identity}");
    let routing_revision = format!("autonomous-routing:{commit_identity}");
    let system_graph_revision = format!("autonomous-system-graph:{commit_identity}");
    let mut receipt = ExtractionAuthorityCommitReceipt {
        receipt_id: format!("extraction-authority-commit-receipt:{commit_identity}"),
        receipt_digest: String::new(),
        expected_authority_revision: inputs.current_authority_revision,
        expected_routing_revision: inputs.current_routing_revision,
        expected_system_graph_revision: inputs.current_system_graph_revision,
        committed_authority_revision: authority_revision.clone(),
        committed_routing_revision: routing_revision.clone(),
        committed_system_graph_revision: system_graph_revision.clone(),
        candidate_service_id: cutover.candidate_service_id.clone(),
        outcome: "committed_single_compare_and_set".to_owned(),
    };
    receipt.receipt_digest = digest(&receipt_without_digest(&receipt));
    let mut result = ExtractionAuthorityCommitResult {
        protocol: EXTRACTION_AUTHORITY_COMMIT_PROTOCOL.to_owned(),
        commit_id: format!("extraction-authority-commit:{commit_identity}"),
        commit_digest: String::new(),
        status: ExtractionAuthorityCommitStatus::Committed,
        plan_digest: cutover.plan_digest.clone(),
        approval: inputs.approval,
        authority_revision,
        routing_revision,
        system_graph_revision,
        candidate_service_id: cutover.candidate_service_id.clone(),
        candidate_authoritative: true,
        linked_authoritative: false,
        candidate_mutations_open: true,
        linked_recovery_read_only: true,
        source_cleanup_performed: false,
        autonomous_mutation_ids: Vec::new(),
        fast_rollback_blocked: false,
        business_execution_requires_runtime_console: false,
        business_execution_requires_system_plane: false,
        commit_receipts: vec![receipt],
    };
    refresh(&mut result);
    Ok(result)
}

/// Atomically compares and changes the persisted authority, routing, and
/// System graph revisions. The receipt and committed state share one database
/// transaction, so a partial topology transfer cannot escape.
pub async fn commit_extraction_authority_postgres(
    pool: &sqlx::PgPool,
    mut inputs: ExtractionAuthorityCommitInputs,
    verifier: &dyn ExtractionApprovalVerifier,
) -> Result<ExtractionAuthorityCommitResult, ExtractionAuthorityCommitError> {
    if !verifier.verify(&inputs.approval) {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalUnauthorized,
            "The configured Approval Authority rejected this approval.",
            "Obtain a fresh approval through the protected workflow.",
        ));
    }
    let mut tx = pool.begin().await.map_err(persistence_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtext('lenso-extraction-topology'))")
        .execute(&mut *tx)
        .await
        .map_err(persistence_error)?;
    inputs.revalidation.reconciliation = load_commit_artifact(
        &mut tx,
        &inputs.cutover.plan_id,
        "lenso.extraction-reconciliation.v1",
    )
    .await?;
    inputs.revalidation.verification = load_commit_artifact(
        &mut tx,
        &inputs.cutover.plan_id,
        "lenso.extraction-verification.v1",
    )
    .await?;
    inputs.revalidation.quiescence = load_commit_artifact(
        &mut tx,
        &inputs.cutover.plan_id,
        "lenso.extraction-quiescence.v1",
    )
    .await?;
    inputs.revalidation.candidate_health = load_commit_artifact(
        &mut tx,
        &inputs.cutover.plan_id,
        EXTRACTION_CANDIDATE_HEALTH_PROTOCOL,
    )
    .await?;
    let result = commit_extraction_authority(inputs.clone())?;
    sqlx::raw_sql(
        r#"
        create schema if not exists lenso_extraction;
        create table if not exists lenso_extraction.authority_states (
            state_id text primary key,
            authority_revision text not null,
            routing_revision text not null,
            system_graph_revision text not null,
            authority_kind text not null,
            owner_id text not null,
            updated_at timestamptz not null default now()
        );
        create table if not exists lenso_extraction.authority_commits (
            approval_digest text primary key,
            plan_digest text not null,
            result_json jsonb not null,
            committed_at timestamptz not null default now()
        );
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(persistence_error)?;
    if let Some(value) = sqlx::query_scalar::<_, serde_json::Value>(
        "select result_json from lenso_extraction.authority_commits where approval_digest = $1",
    )
    .bind(&inputs.approval.approval_digest)
    .fetch_optional(&mut *tx)
    .await
    .map_err(persistence_error)?
    {
        let _: ExtractionAuthorityCommitResult =
            serde_json::from_value(value).map_err(|source| {
                error(
                    ExtractionAuthorityCommitErrorCode::PersistenceFailed,
                    format!("Stored authority commit is unreadable: {source}"),
                    "Repair or restore the last valid authority commit receipt.",
                )
            })?;
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ApprovalStale,
            "This approval has already been committed and cannot be replayed.",
            "Inspect the persisted commit receipt and current topology state.",
        ));
    }
    let persisted = sqlx::query_as::<_, (String, String, String, String)>(
        "select authority_revision, routing_revision, system_graph_revision, authority_kind from lenso_extraction.authority_states where state_id = 'system' for update",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(persistence_error)?
    .ok_or_else(|| error(
        ExtractionAuthorityCommitErrorCode::FinalStateInvalid,
        "Authoritative topology state has not been initialized by the composition root.",
        "Install the linked authority, routing, and System graph state before Cutover.",
    ))?;
    if persisted
        != (
            inputs.current_authority_revision.clone(),
            inputs.current_routing_revision.clone(),
            inputs.current_system_graph_revision.clone(),
            "linked".to_owned(),
        )
    {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ConcurrentStateChange,
            "Persisted authority, routing, or System graph state changed before commit.",
            "Reload persisted topology state and repeat Cutover verification.",
        ));
    }
    let changed = sqlx::query(
        r#"
        update lenso_extraction.authority_states
        set authority_revision = $2, routing_revision = $3, system_graph_revision = $4,
            authority_kind = 'autonomous', owner_id = $5, updated_at = now()
        where state_id = 'system' and authority_revision = $6 and routing_revision = $7
          and system_graph_revision = $8 and authority_kind = 'linked'
        "#,
    )
    .bind("system")
    .bind(&result.authority_revision)
    .bind(&result.routing_revision)
    .bind(&result.system_graph_revision)
    .bind(&result.candidate_service_id)
    .bind(&result.commit_receipts[0].expected_authority_revision)
    .bind(&result.commit_receipts[0].expected_routing_revision)
    .bind(&result.commit_receipts[0].expected_system_graph_revision)
    .execute(&mut *tx)
    .await
    .map_err(persistence_error)?;
    if changed.rows_affected() != 1 {
        return Err(error(
            ExtractionAuthorityCommitErrorCode::ConcurrentStateChange,
            "Atomic authority compare-and-set lost a concurrent race.",
            "Reload topology state; do not retry with stale approval evidence.",
        ));
    }
    sqlx::query(
        "insert into lenso_extraction.authority_commits (approval_digest, plan_digest, result_json) values ($1, $2, $3)",
    )
    .bind(&result.approval.approval_digest)
    .bind(&result.plan_digest)
    .bind(serde_json::to_value(&result).map_err(|source| {
        error(
            ExtractionAuthorityCommitErrorCode::PersistenceFailed,
            format!("Authority commit could not serialize: {source}"),
            "Abort before committing topology state.",
        )
    })?)
    .execute(&mut *tx)
    .await
    .map_err(persistence_error)?;
    tx.commit().await.map_err(persistence_error)?;
    Ok(result)
}

async fn load_commit_artifact<T: serde::de::DeserializeOwned>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    plan_id: &str,
    protocol: &str,
) -> Result<T, ExtractionAuthorityCommitError> {
    let value = sqlx::query_scalar::<_, serde_json::Value>(
        "select artifact_json from platform.extraction_artifacts where plan_id = $1 and protocol = $2 order by recorded_at desc, artifact_id desc limit 1",
    )
    .bind(plan_id)
    .bind(protocol)
    .fetch_optional(&mut **tx)
    .await
    .map_err(persistence_error)?
    .ok_or_else(|| {
        error(
            ExtractionAuthorityCommitErrorCode::FinalStateInvalid,
            format!("Required persisted final artifact `{protocol}` is missing."),
            "Persist fresh reconciliation, verification, and quiescence artifacts before approval commit.",
        )
    })?;
    serde_json::from_value(value).map_err(|source| {
        error(
            ExtractionAuthorityCommitErrorCode::FinalStateInvalid,
            format!("Persisted final artifact `{protocol}` is unreadable: {source}"),
            "Regenerate and persist an integrity-valid final artifact.",
        )
    })
}

pub async fn initialize_extraction_topology_state(
    pool: &sqlx::PgPool,
    state: &ExtractionTopologyState,
) -> Result<(), ExtractionAuthorityCommitError> {
    sqlx::raw_sql(
        r#"
        create schema if not exists lenso_extraction;
        create table if not exists lenso_extraction.authority_states (
            state_id text primary key,
            authority_revision text not null,
            routing_revision text not null,
            system_graph_revision text not null,
            authority_kind text not null,
            owner_id text not null,
            updated_at timestamptz not null default now()
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(persistence_error)?;
    sqlx::query(
        "insert into lenso_extraction.authority_states (state_id, authority_revision, routing_revision, system_graph_revision, authority_kind, owner_id) values ('system',$1,$2,$3,$4,$5) on conflict (state_id) do nothing",
    )
    .bind(&state.authority_revision)
    .bind(&state.routing_revision)
    .bind(&state.system_graph_revision)
    .bind(&state.authority_kind)
    .bind(&state.owner_id)
    .execute(pool)
    .await
    .map_err(persistence_error)?;
    Ok(())
}

fn persistence_error(source: sqlx::Error) -> ExtractionAuthorityCommitError {
    error(
        ExtractionAuthorityCommitErrorCode::PersistenceFailed,
        format!("PostgreSQL authority commit failed: {source}"),
        "Restore PostgreSQL and reload the persisted topology state.",
    )
}

#[must_use]
pub fn record_autonomous_mutation(
    mut result: ExtractionAuthorityCommitResult,
    mutation_id: impl Into<String>,
) -> ExtractionAuthorityCommitResult {
    let mutation_id = mutation_id.into();
    if !result.autonomous_mutation_ids.contains(&mutation_id) {
        result.autonomous_mutation_ids.push(mutation_id);
        result.autonomous_mutation_ids.sort();
    }
    result.fast_rollback_blocked = !result.autonomous_mutation_ids.is_empty();
    refresh(&mut result);
    result
}

pub fn request_fast_extraction_rollback(
    result: &ExtractionAuthorityCommitResult,
    evidence: Option<&ExtractionReverseMigrationEvidence>,
) -> Result<(), ExtractionFastRollbackError> {
    let reviewed = evidence.is_some_and(|evidence| {
        evidence.approved
            && !evidence.reviewed_by.trim().is_empty()
            && evidence.plan_digest == result.plan_digest
            && !evidence.reconciliation_digest.trim().is_empty()
    });
    if result.fast_rollback_blocked && !reviewed {
        return Err(ExtractionFastRollbackError {
            code: ExtractionFastRollbackIssueCode::ReverseMigrationEvidenceRequired,
            message: "Fast rollback is blocked after Autonomous writes began.".to_owned(),
            next_actions: vec!["Review a reverse-migration and reconciliation plan before changing authority again.".to_owned()],
        });
    }
    Ok(())
}

fn error(
    code: ExtractionAuthorityCommitErrorCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ExtractionAuthorityCommitError {
    ExtractionAuthorityCommitError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
        mutation_started: false,
    }
}

fn approval_digest(approval: &ExtractionApproval) -> String {
    let mut value = approval.clone();
    value.approval_digest.clear();
    digest(&value)
}

fn candidate_health_digest(evidence: &ExtractionCandidateHealthEvidence) -> String {
    let mut value = evidence.clone();
    value.evidence_digest.clear();
    digest(&value)
}

fn receipt_without_digest(
    receipt: &ExtractionAuthorityCommitReceipt,
) -> ExtractionAuthorityCommitReceipt {
    let mut value = receipt.clone();
    value.receipt_digest.clear();
    value
}

fn refresh(result: &mut ExtractionAuthorityCommitResult) {
    result.commit_digest.clear();
    result.commit_digest = digest(result);
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("Authority commit values serialize"))
}
