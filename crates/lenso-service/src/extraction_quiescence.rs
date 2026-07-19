use crate::{
    ExtractionBackfillRun, ExtractionBackfillStatus, ExtractionPlan,
    ExtractionReconciliationResult, ExtractionReconciliationStatus, extraction_input_digest,
    extraction_plan_integrity_is_valid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const EXTRACTION_QUIESCENCE_PROTOCOL: &str = "lenso.extraction-quiescence.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDrainSnapshot {
    pub in_flight_requests: u64,
    pub outbox_messages: u64,
    pub inbox_messages: u64,
    pub scheduled_functions: u64,
    pub timers: u64,
    pub durable_workflows: u64,
    #[serde(default)]
    pub unresolved: Vec<String>,
    pub timed_out: bool,
}

impl ExtractionDrainSnapshot {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            in_flight_requests: 0,
            outbox_messages: 0,
            inbox_messages: 0,
            scheduled_functions: 0,
            timers: 0,
            durable_workflows: 0,
            unresolved: Vec::new(),
            timed_out: false,
        }
    }

    fn pending_count(&self) -> u64 {
        self.in_flight_requests
            + self.outbox_messages
            + self.inbox_messages
            + self.scheduled_functions
            + self.timers
            + self.durable_workflows
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionQuiescenceStatus {
    MutationsPaused,
    Draining,
    Blocked,
    Drained,
    Quiesced,
    Cancelled,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionQuiescenceIssueCode {
    PlanInvalid,
    PlanStale,
    AuthorityChanged,
    DrainIncomplete,
    DrainTimedOut,
    FinalBackfillIncomplete,
    FinalReconciliationMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionQuiescenceIssue {
    pub code: ExtractionQuiescenceIssueCode,
    pub subject: String,
    pub detail: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionQuiescenceEvidence {
    pub kind: String,
    pub subject: String,
    pub digest: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionQuiescenceEffects {
    pub pauses_linked_mutations: bool,
    pub drains_in_flight_work: bool,
    pub copies_final_delta: bool,
    pub routes_candidate_traffic: bool,
    pub changes_authority: bool,
    pub requires_runtime_console: bool,
    pub requires_system_plane_for_business_execution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionQuiescenceRun {
    pub protocol: String,
    pub quiescence_id: String,
    pub quiescence_digest: String,
    pub revision: u64,
    pub status: ExtractionQuiescenceStatus,
    pub plan_id: String,
    pub plan_digest: String,
    pub expected_authority_revision: String,
    pub linked_mutations_paused: bool,
    pub linked_read_inspection_available: bool,
    pub linked_authority_remains_authoritative: bool,
    pub candidate_authoritative: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain: Option<ExtractionDrainSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_source_high_water_mark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_checkpoint: Option<String>,
    #[serde(default)]
    pub issues: Vec<ExtractionQuiescenceIssue>,
    #[serde(default)]
    pub evidence: Vec<ExtractionQuiescenceEvidence>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    pub effects: ExtractionQuiescenceEffects,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionQuiescenceStartError {
    pub code: ExtractionQuiescenceIssueCode,
    pub message: String,
}

impl fmt::Display for ExtractionQuiescenceStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionQuiescenceStartError {}

pub fn start_extraction_quiescence(
    plan: &ExtractionPlan,
    current_authority_revision: &str,
) -> Result<ExtractionQuiescenceRun, ExtractionQuiescenceStartError> {
    if !extraction_plan_integrity_is_valid(plan) {
        return Err(ExtractionQuiescenceStartError {
            code: ExtractionQuiescenceIssueCode::PlanInvalid,
            message: "Extraction Plan integrity validation failed before write pause.".to_owned(),
        });
    }
    if plan.expected_authority.revision != current_authority_revision {
        return Err(ExtractionQuiescenceStartError {
            code: ExtractionQuiescenceIssueCode::AuthorityChanged,
            message: "Authority changed after the Extraction Plan was generated.".to_owned(),
        });
    }
    let identity = digest(&(
        plan.plan_id.as_str(),
        plan.plan_digest.as_str(),
        current_authority_revision,
    ));
    let mut run = ExtractionQuiescenceRun {
        protocol: EXTRACTION_QUIESCENCE_PROTOCOL.to_owned(),
        quiescence_id: format!("extraction-quiescence:{identity}"),
        quiescence_digest: String::new(),
        revision: 1,
        status: ExtractionQuiescenceStatus::MutationsPaused,
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        expected_authority_revision: current_authority_revision.to_owned(),
        linked_mutations_paused: true,
        linked_read_inspection_available: true,
        linked_authority_remains_authoritative: true,
        candidate_authoritative: false,
        drain: None,
        stable_source_high_water_mark: None,
        destination_checkpoint: None,
        issues: Vec::new(),
        evidence: vec![evidence(
            "write_pause",
            "linked-mutations",
            current_authority_revision,
            "New linked mutations are paused while read-only inspection remains available.",
        )],
        next_actions: vec![
            "Drain in-flight requests, Outbox, Inbox, schedules, timers, and Durable Workflows."
                .to_owned(),
        ],
        effects: ExtractionQuiescenceEffects {
            pauses_linked_mutations: true,
            ..ExtractionQuiescenceEffects::default()
        },
    };
    refresh(&mut run);
    Ok(run)
}

#[must_use]
pub fn record_extraction_drain(
    mut run: ExtractionQuiescenceRun,
    mut snapshot: ExtractionDrainSnapshot,
) -> ExtractionQuiescenceRun {
    snapshot.unresolved.sort();
    snapshot.unresolved.dedup();
    run.drain = Some(snapshot.clone());
    run.effects.drains_in_flight_work = true;
    run.issues.clear();
    if snapshot.timed_out {
        push_issue(
            &mut run,
            ExtractionQuiescenceIssueCode::DrainTimedOut,
            "drain",
            "The bounded drain timed out.",
            "Cancel safely or remediate unresolved work before retrying.",
        );
    } else if snapshot.pending_count() > 0 || !snapshot.unresolved.is_empty() {
        push_issue(
            &mut run,
            ExtractionQuiescenceIssueCode::DrainIncomplete,
            "drain",
            "Eligible work has not drained completely.",
            "Resolve each recorded work identity; do not abandon it.",
        );
    } else {
        run.status = ExtractionQuiescenceStatus::Drained;
        run.evidence.push(evidence(
            "drain_complete",
            "linked-runtime-work",
            &digest(&snapshot),
            "Requests, Outbox, Inbox, schedules, timers, and Durable Workflows are drained.",
        ));
        run.next_actions = vec![
            "Copy the final Postgres delta and reconcile at the stable high-water mark.".to_owned(),
        ];
    }
    run.revision += 1;
    refresh(&mut run);
    run
}

#[must_use]
pub fn complete_extraction_quiescence(
    mut run: ExtractionQuiescenceRun,
    backfill: &ExtractionBackfillRun,
    reconciliation: &ExtractionReconciliationResult,
    current_plan_digest: &str,
    current_authority_revision: &str,
) -> ExtractionQuiescenceRun {
    run.issues.clear();
    if run.plan_digest != current_plan_digest {
        push_issue(
            &mut run,
            ExtractionQuiescenceIssueCode::PlanStale,
            "plan",
            "Plan inputs changed during drain.",
            "Regenerate the plan before retrying Cutover.",
        );
    }
    if run.expected_authority_revision != current_authority_revision {
        push_issue(
            &mut run,
            ExtractionQuiescenceIssueCode::AuthorityChanged,
            "authority",
            "Authority changed during drain.",
            "Return to the recorded linked authority before retrying.",
        );
    }
    if backfill.status != ExtractionBackfillStatus::Succeeded
        || backfill.scope.plan_id != run.plan_id
    {
        push_issue(
            &mut run,
            ExtractionQuiescenceIssueCode::FinalBackfillIncomplete,
            "final-delta",
            "Final delta backfill is incomplete or belongs to another plan.",
            "Complete the final checkpointed delta under the write pause.",
        );
    }
    if reconciliation.status != ExtractionReconciliationStatus::Matched
        || reconciliation.plan_id != run.plan_id
        || reconciliation.source_high_water_mark != backfill.progress.source_high_water_mark
        || reconciliation.destination_checkpoint
            != backfill
                .progress
                .destination_checkpoint
                .clone()
                .unwrap_or_default()
    {
        push_issue(
            &mut run,
            ExtractionQuiescenceIssueCode::FinalReconciliationMismatch,
            "reconciliation",
            "Final reconciliation does not match the stable delta checkpoint.",
            "Reconcile the exact final high-water mark and checkpoint.",
        );
    }
    if run.issues.is_empty() && run.status == ExtractionQuiescenceStatus::Drained {
        run.status = ExtractionQuiescenceStatus::Quiesced;
        run.stable_source_high_water_mark = Some(backfill.progress.source_high_water_mark.clone());
        run.destination_checkpoint = backfill.progress.destination_checkpoint.clone();
        run.effects.copies_final_delta = true;
        run.evidence.push(evidence(
            "quiesced",
            "linked-authority",
            &reconciliation.reconciliation_digest,
            "Final delta and reconciliation are stable; authority has not transferred.",
        ));
        run.next_actions = vec![
            "Use this evidence for provisional Cutover while external mutations remain paused."
                .to_owned(),
        ];
    }
    run.revision += 1;
    refresh(&mut run);
    run
}

#[must_use]
pub fn cancel_extraction_quiescence(
    mut run: ExtractionQuiescenceRun,
    reason: impl Into<String>,
) -> ExtractionQuiescenceRun {
    let reason = reason.into();
    run.status = ExtractionQuiescenceStatus::Cancelled;
    run.linked_mutations_paused = false;
    run.effects.pauses_linked_mutations = false;
    run.evidence.push(evidence(
        "write_pause_released",
        "linked-mutations",
        &reason,
        &reason,
    ));
    run.next_actions = vec![
        "Linked implementation remains authoritative; create fresh evidence before retrying."
            .to_owned(),
    ];
    run.revision += 1;
    refresh(&mut run);
    run
}

fn push_issue(
    run: &mut ExtractionQuiescenceRun,
    code: ExtractionQuiescenceIssueCode,
    subject: &str,
    detail: &str,
    next_action: &str,
) {
    run.status = ExtractionQuiescenceStatus::Blocked;
    run.issues.push(ExtractionQuiescenceIssue {
        code,
        subject: subject.to_owned(),
        detail: detail.to_owned(),
        next_actions: vec![next_action.to_owned()],
    });
    run.next_actions = vec![next_action.to_owned()];
}

fn evidence(kind: &str, subject: &str, value: &str, detail: &str) -> ExtractionQuiescenceEvidence {
    ExtractionQuiescenceEvidence {
        kind: kind.to_owned(),
        subject: subject.to_owned(),
        digest: extraction_input_digest(value.as_bytes()),
        detail: detail.to_owned(),
    }
}

fn refresh(run: &mut ExtractionQuiescenceRun) {
    run.quiescence_digest.clear();
    run.quiescence_digest = digest(run);
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("quiescence values serialize"))
}
