use super::{
    WorkflowApiError, WorkflowEventPublication, WorkflowFailureEvidence, WorkflowMutationError,
    WorkflowStepInspection, WorkflowStepTransitionResult,
};
use crate::ServiceRuntimeState;
use chrono::{DateTime, Duration, Utc};
use lenso_contracts::WorkflowStepDeclaration;
use platform_core::Clock;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::{collections::HashMap, sync::Mutex};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFailureClassification {
    Retryable,
    Timeout,
    Permanent,
}

impl WorkflowFailureClassification {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Timeout => "timeout",
            Self::Permanent => "permanent",
        }
    }

    fn parse(value: &str) -> Result<Self, WorkflowApiError> {
        match value {
            "retryable" => Ok(Self::Retryable),
            "timeout" => Ok(Self::Timeout),
            "permanent" => Ok(Self::Permanent),
            other => Err(WorkflowApiError::stored_state(format!(
                "Stored workflow failure classification `{other}` is invalid"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStepFailure {
    pub classification: WorkflowFailureClassification,
    pub code: String,
    pub message: String,
}

impl WorkflowStepFailure {
    #[must_use]
    pub fn retryable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            classification: WorkflowFailureClassification::Retryable,
            code: code.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn timeout(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            classification: WorkflowFailureClassification::Timeout,
            code: code.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn permanent(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            classification: WorkflowFailureClassification::Permanent,
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepFailureInspection {
    pub classification: WorkflowFailureClassification,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepAttemptState {
    Running,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepAttemptInspection {
    pub attempt_id: String,
    pub attempt_number: u32,
    pub transition_id: String,
    pub state: WorkflowStepAttemptState,
    pub failure: Option<WorkflowStepFailureInspection>,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTimerKind {
    Retry,
    StepTimeout,
}

impl WorkflowTimerKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::StepTimeout => "step_timeout",
        }
    }

    fn parse(value: &str) -> Result<Self, WorkflowApiError> {
        match value {
            "retry" => Ok(Self::Retry),
            "step_timeout" => Ok(Self::StepTimeout),
            other => Err(WorkflowApiError::stored_state(format!(
                "Stored workflow timer kind `{other}` is invalid"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTimerState {
    Pending,
    Claimed,
    Completed,
    Cancelled,
}

impl WorkflowTimerState {
    fn parse(value: &str) -> Result<Self, WorkflowApiError> {
        match value {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(WorkflowApiError::stored_state(format!(
                "Stored workflow timer state `{other}` is invalid"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTimerInspection {
    pub timer_id: String,
    pub kind: WorkflowTimerKind,
    pub attempt_number: u32,
    pub transition_id: String,
    pub due_at: DateTime<Utc>,
    pub state: WorkflowTimerState,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowWorkClaim {
    pub timer_id: String,
    pub instance_id: String,
    pub step_id: String,
    pub kind: WorkflowTimerKind,
    pub attempt_number: u32,
    pub transition_id: String,
    pub attempt_transition_id: String,
    pub due_at: DateTime<Utc>,
    pub claimed_by: String,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowFailureDisposition {
    RetryScheduled,
    Exhausted,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowFailureResult {
    pub disposition: WorkflowFailureDisposition,
    pub instance_id: String,
    pub step_id: String,
    pub attempt_number: u32,
    pub attempt_count: u32,
    pub classification: WorkflowFailureClassification,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub terminal_exhausted: bool,
}

/// Development-only controlled clock for deterministic System Sandbox proofs.
#[derive(Debug)]
pub struct SystemSandboxWorkflowClock {
    now: Mutex<DateTime<Utc>>,
}

impl SystemSandboxWorkflowClock {
    #[must_use]
    pub const fn new(now: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    pub fn set(&self, now: DateTime<Utc>) {
        *self.now.lock().expect("workflow clock lock poisoned") = now;
    }

    #[must_use]
    pub fn advance(&self, duration: Duration) -> DateTime<Utc> {
        let mut now = self.now.lock().expect("workflow clock lock poisoned");
        *now += duration;
        *now
    }
}

impl Clock for SystemSandboxWorkflowClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().expect("workflow clock lock poisoned")
    }
}

pub(super) fn workflow_now(state: &ServiceRuntimeState) -> DateTime<Utc> {
    postgres_precision(state.workflow_clock.now())
}

fn postgres_precision(now: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_micros(now.timestamp_micros())
        .expect("workflow timestamp must fit PostgreSQL microsecond precision")
}

fn add_milliseconds(
    at: DateTime<Utc>,
    milliseconds: u64,
) -> Result<DateTime<Utc>, WorkflowMutationError> {
    let milliseconds = i64::try_from(milliseconds).map_err(|_| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StoredStateInvalid,
            "Workflow duration exceeds the supported range",
        )
    })?;
    at.checked_add_signed(Duration::milliseconds(milliseconds))
        .ok_or_else(|| {
            WorkflowMutationError::new(
                super::WorkflowErrorCode::StoredStateInvalid,
                "Workflow duration exceeds the supported timestamp range",
            )
        })
}

pub(super) fn pending_step_inspection(
    step_id: String,
    declaration: &WorkflowStepDeclaration,
    position: u32,
    timers: Vec<WorkflowTimerInspection>,
    now: DateTime<Utc>,
) -> WorkflowStepInspection {
    let (max_attempts, retry_schedule_ms) = step_policy(declaration);
    WorkflowStepInspection {
        step_id,
        definition_step_name: declaration.name.clone(),
        position,
        state: super::WorkflowStepState::Pending,
        transition_id: None,
        completed_at: None,
        outgoing_work: None,
        attempt_count: 0,
        max_attempts,
        retry_schedule_ms,
        next_attempt_at: None,
        latest_failure: None,
        exhausted_at: None,
        attempts: Vec::new(),
        timers,
        child_workflow: None,
        created_at: now,
        updated_at: now,
    }
}

fn step_policy(declaration: &WorkflowStepDeclaration) -> (u32, Vec<u64>) {
    declaration.retry_policy.as_ref().map_or_else(
        || (1, Vec::new()),
        |policy| (policy.max_attempts, policy.delays_ms.clone()),
    )
}

pub(super) async fn persist_workflow_step_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: &str,
    declaration: &WorkflowStepDeclaration,
    position: i32,
    now: DateTime<Utc>,
) -> Result<Vec<WorkflowTimerInspection>, WorkflowMutationError> {
    let (max_attempts, retry_schedule) = step_policy(declaration);
    let retry_schedule_json = serde_json::to_value(&retry_schedule).map_err(|error| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode workflow retry schedule: {error}"),
        )
    })?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_steps (
            step_id, instance_id, definition_step_name, step_position,
            state, attempt_count, max_attempts, retry_schedule, timeout_ms,
            created_at, updated_at
        ) values ($1, $2, $3, $4, 'pending', 0, $5, $6, $7, $8, $8)
        "#,
    )
    .bind(step_id)
    .bind(instance_id)
    .bind(&declaration.name)
    .bind(position)
    .bind(i32::try_from(max_attempts).map_err(|_| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StoredStateInvalid,
            "Workflow retry policy exceeds the supported attempt count",
        )
    })?)
    .bind(retry_schedule_json)
    .bind(
        declaration
            .timeout_ms
            .map(i64::try_from)
            .transpose()
            .map_err(|_| {
                WorkflowMutationError::new(
                    super::WorkflowErrorCode::StoredStateInvalid,
                    "Workflow timeout exceeds the supported range",
                )
            })?,
    )
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist workflow step: {error}"))
    })?;

    let Some(timeout_ms) = declaration.timeout_ms else {
        return Ok(Vec::new());
    };
    let due_at = add_milliseconds(now, timeout_ms)?;
    let timer_id = format!("workflow_timer_{}", Uuid::now_v7());
    let attempt_transition_id = format!("{step_id}:attempt:1");
    let transition_id = format!("{attempt_transition_id}:timeout");
    sqlx::query(
        r#"
        insert into platform.service_workflow_timers (
            timer_id, instance_id, step_id, kind, attempt_number,
            transition_id, attempt_transition_id, due_at, state,
            created_at, updated_at
        ) values ($1, $2, $3, 'step_timeout', 1, $4, $5, $6,
                  'pending', $7, $7)
        "#,
    )
    .bind(&timer_id)
    .bind(instance_id)
    .bind(step_id)
    .bind(&transition_id)
    .bind(&attempt_transition_id)
    .bind(due_at)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist workflow timeout: {error}"))
    })?;
    Ok(vec![WorkflowTimerInspection {
        timer_id,
        kind: WorkflowTimerKind::StepTimeout,
        attempt_number: 1,
        transition_id,
        due_at,
        state: WorkflowTimerState::Pending,
        claimed_by: None,
        claimed_at: None,
        completed_at: None,
    }])
}

#[derive(Debug, FromRow)]
pub(super) struct WorkflowAttemptRow {
    step_id: String,
    attempt_id: String,
    attempt_number: i32,
    transition_id: String,
    state: String,
    failure_classification: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    scheduled_at: DateTime<Utc>,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
pub(super) struct WorkflowTimerRow {
    step_id: String,
    timer_id: String,
    kind: String,
    attempt_number: i32,
    transition_id: String,
    due_at: DateTime<Utc>,
    state: String,
    claimed_by: Option<String>,
    claimed_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub(super) struct WorkflowRecoveryInspection {
    pub attempts: Vec<WorkflowStepAttemptInspection>,
    pub timers: Vec<WorkflowTimerInspection>,
}

pub(super) type WorkflowRecoveryByStep = HashMap<String, WorkflowRecoveryInspection>;

pub(super) async fn load_recovery(
    pool: &PgPool,
    instance_id: &str,
) -> Result<WorkflowRecoveryByStep, WorkflowApiError> {
    let attempts = sqlx::query_as::<_, WorkflowAttemptRow>(
        r#"
        select step_id, attempt_id, attempt_number, transition_id, state,
               failure_classification, failure_code, failure_message,
               scheduled_at, started_at, completed_at
        from platform.service_workflow_step_attempts
        where instance_id = $1
        order by step_id, attempt_number
        "#,
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not inspect workflow attempts: {error}"))
    })?;
    let timers = sqlx::query_as::<_, WorkflowTimerRow>(
        r#"
        select step_id, timer_id, kind, attempt_number, transition_id, due_at,
               state, claimed_by, claimed_at, completed_at
        from platform.service_workflow_timers
        where instance_id = $1
        order by step_id, attempt_number, created_at, timer_id
        "#,
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!("Could not inspect workflow timers: {error}"))
    })?;
    recovery_from_rows(attempts, timers)
}

pub(super) async fn load_recovery_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
) -> Result<WorkflowRecoveryByStep, WorkflowMutationError> {
    let attempts = sqlx::query_as::<_, WorkflowAttemptRow>(
        r#"
        select step_id, attempt_id, attempt_number, transition_id, state,
               failure_classification, failure_code, failure_message,
               scheduled_at, started_at, completed_at
        from platform.service_workflow_step_attempts
        where instance_id = $1
        order by step_id, attempt_number
        "#,
    )
    .bind(instance_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect workflow attempts: {error}"))
    })?;
    let timers = sqlx::query_as::<_, WorkflowTimerRow>(
        r#"
        select step_id, timer_id, kind, attempt_number, transition_id, due_at,
               state, claimed_by, claimed_at, completed_at
        from platform.service_workflow_timers
        where instance_id = $1
        order by step_id, attempt_number, created_at, timer_id
        "#,
    )
    .bind(instance_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect workflow timers: {error}"))
    })?;
    recovery_from_rows(attempts, timers)
        .map_err(|error| WorkflowMutationError::new(error.code, error.message))
}

fn recovery_from_rows(
    attempts: Vec<WorkflowAttemptRow>,
    timers: Vec<WorkflowTimerRow>,
) -> Result<WorkflowRecoveryByStep, WorkflowApiError> {
    let mut recovery = WorkflowRecoveryByStep::new();
    for row in attempts {
        let state = match row.state.as_str() {
            "running" => WorkflowStepAttemptState::Running,
            "failed" => WorkflowStepAttemptState::Failed,
            "succeeded" => WorkflowStepAttemptState::Succeeded,
            other => {
                return Err(WorkflowApiError::stored_state(format!(
                    "Stored workflow attempt state `{other}` is invalid"
                )));
            }
        };
        let failure = row
            .failure_classification
            .as_deref()
            .map(WorkflowFailureClassification::parse)
            .transpose()?
            .map(|classification| WorkflowStepFailureInspection {
                classification,
                code: row.failure_code.unwrap_or_default(),
                message: row.failure_message.unwrap_or_default(),
            });
        recovery
            .entry(row.step_id)
            .or_default()
            .attempts
            .push(WorkflowStepAttemptInspection {
                attempt_id: row.attempt_id,
                attempt_number: u32::try_from(row.attempt_number).map_err(|_| {
                    WorkflowApiError::stored_state("Stored workflow attempt number is invalid")
                })?,
                transition_id: row.transition_id,
                state,
                failure,
                scheduled_at: row.scheduled_at,
                started_at: row.started_at,
                completed_at: row.completed_at,
            });
    }
    for row in timers {
        recovery
            .entry(row.step_id)
            .or_default()
            .timers
            .push(WorkflowTimerInspection {
                timer_id: row.timer_id,
                kind: WorkflowTimerKind::parse(&row.kind)?,
                attempt_number: u32::try_from(row.attempt_number).map_err(|_| {
                    WorkflowApiError::stored_state("Stored workflow timer attempt is invalid")
                })?,
                transition_id: row.transition_id,
                due_at: row.due_at,
                state: WorkflowTimerState::parse(&row.state)?,
                claimed_by: row.claimed_by,
                claimed_at: row.claimed_at,
                completed_at: row.completed_at,
            });
    }
    Ok(recovery)
}

pub(super) fn latest_failure(
    classification: Option<&str>,
    code: Option<String>,
    message: Option<String>,
) -> Result<Option<WorkflowStepFailureInspection>, WorkflowApiError> {
    classification
        .map(WorkflowFailureClassification::parse)
        .transpose()
        .map(|classification| {
            classification.map(|classification| WorkflowStepFailureInspection {
                classification,
                code: code.unwrap_or_default(),
                message: message.unwrap_or_default(),
            })
        })
}

pub(super) fn retry_schedule(value: serde_json::Value) -> Result<Vec<u64>, WorkflowApiError> {
    serde_json::from_value(value).map_err(|error| {
        WorkflowApiError::stored_state(format!(
            "Stored workflow retry schedule is invalid: {error}"
        ))
    })
}

#[derive(Debug, FromRow)]
struct ClaimedTimerRow {
    timer_id: String,
    instance_id: String,
    step_id: String,
    kind: String,
    attempt_number: i32,
    transition_id: String,
    attempt_transition_id: String,
    due_at: DateTime<Utc>,
    claimed_by: String,
    claimed_at: DateTime<Utc>,
    timeout_ms: Option<i64>,
}

/// Claims due retry and timeout work. Claims older than the lease are safely
/// reclaimed after a worker restart. A due timeout takes precedence over the
/// abandoned retry attempt it governs.
pub async fn claim_due_workflow_work_at(
    state: &ServiceRuntimeState,
    worker_id: &str,
    now: DateTime<Utc>,
    claim_lease: Duration,
    limit: i64,
) -> Result<Vec<WorkflowWorkClaim>, WorkflowMutationError> {
    if worker_id.trim().is_empty() || limit <= 0 || claim_lease <= Duration::zero() {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::InvalidRequest,
            "Workflow claim requires a worker identity, positive lease, and positive limit",
        ));
    }
    let now = postgres_precision(now);
    let stale_before = now.checked_sub_signed(claim_lease).ok_or_else(|| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::InvalidRequest,
            "Workflow claim lease exceeds the supported timestamp range",
        )
    })?;
    let pool = state
        .store()
        .map_err(|error| WorkflowMutationError::store(error.public_message))?;
    let mut transaction = pool.begin().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not begin workflow claim: {error}"))
    })?;
    let rows = sqlx::query_as::<_, ClaimedTimerRow>(
        r#"
        with candidates as (
            select timer.timer_id
            from platform.service_workflow_timers timer
            join platform.service_workflow_steps step on step.step_id = timer.step_id
            join platform.service_workflow_instances instance
              on instance.instance_id = timer.instance_id
            where instance.service_id = $1
              and instance.state = 'running'
              and step.state = 'pending'
              and (
                (timer.state = 'pending' and timer.due_at <= $2)
                or (
                    timer.state = 'claimed'
                    and timer.claimed_at is not null
                    and timer.claimed_at <= $3
                )
              )
              and not exists (
                select 1
                from platform.service_workflow_timers preferred
                where preferred.step_id = timer.step_id
                  and (
                    (preferred.state = 'pending' and preferred.due_at <= $2)
                    or (
                        preferred.state = 'claimed'
                        and preferred.claimed_at is not null
                        and preferred.claimed_at <= $3
                    )
                  )
                  and (
                    case when preferred.kind = 'step_timeout' then 0 else 1 end
                        < case when timer.kind = 'step_timeout' then 0 else 1 end
                    or (
                        preferred.kind = timer.kind
                        and (preferred.due_at, preferred.timer_id)
                            < (timer.due_at, timer.timer_id)
                    )
                  )
              )
            order by timer.due_at, timer.timer_id
            limit $4
            for update of timer skip locked
        )
        update platform.service_workflow_timers timer
        set state = 'claimed', claimed_by = $5, claimed_at = $2, updated_at = $2
        from candidates, platform.service_workflow_steps step
        where timer.timer_id = candidates.timer_id and step.step_id = timer.step_id
        returning timer.timer_id, timer.instance_id, timer.step_id, timer.kind,
                  timer.attempt_number, timer.transition_id,
                  timer.attempt_transition_id, timer.due_at,
                  timer.claimed_by, timer.claimed_at, step.timeout_ms
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(now)
    .bind(stale_before)
    .bind(limit)
    .bind(worker_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not claim due workflow work: {error}"))
    })?;

    let mut claims = Vec::with_capacity(rows.len());
    for row in rows {
        let kind = match row.kind.as_str() {
            "retry" => WorkflowTimerKind::Retry,
            "step_timeout" => WorkflowTimerKind::StepTimeout,
            other => {
                return Err(WorkflowMutationError::new(
                    super::WorkflowErrorCode::StoredStateInvalid,
                    format!("Stored workflow timer kind `{other}` is invalid"),
                ));
            }
        };
        let attempt_number = u32::try_from(row.attempt_number).map_err(|_| {
            WorkflowMutationError::new(
                super::WorkflowErrorCode::StoredStateInvalid,
                "Stored workflow timer attempt number is invalid",
            )
        })?;
        if kind == WorkflowTimerKind::Retry {
            sqlx::query(
                r#"
                insert into platform.service_workflow_step_attempts (
                    attempt_id, instance_id, step_id, attempt_number,
                    transition_id, state, scheduled_at, started_at
                ) values ($1, $2, $3, $4, $5, 'running', $6, $7)
                on conflict (step_id, attempt_number) do nothing
                "#,
            )
            .bind(format!("workflow_attempt_{}", Uuid::now_v7()))
            .bind(&row.instance_id)
            .bind(&row.step_id)
            .bind(row.attempt_number)
            .bind(&row.attempt_transition_id)
            .bind(row.due_at)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                WorkflowMutationError::store(format!(
                    "Could not persist claimed workflow attempt: {error}"
                ))
            })?;
            if let Some(timeout_ms) = row.timeout_ms {
                let timeout_ms = u64::try_from(timeout_ms).map_err(|_| {
                    WorkflowMutationError::new(
                        super::WorkflowErrorCode::StoredStateInvalid,
                        "Stored workflow timeout is invalid",
                    )
                })?;
                let due_at = add_milliseconds(now, timeout_ms)?;
                let timeout_transition_id = format!("{}:timeout", row.attempt_transition_id);
                sqlx::query(
                    r#"
                    insert into platform.service_workflow_timers (
                        timer_id, instance_id, step_id, kind, attempt_number,
                        transition_id, attempt_transition_id, due_at, state,
                        created_at, updated_at
                    ) values ($1, $2, $3, 'step_timeout', $4, $5, $6, $7,
                              'pending', $8, $8)
                    on conflict (step_id, transition_id) do nothing
                    "#,
                )
                .bind(format!("workflow_timer_{}", Uuid::now_v7()))
                .bind(&row.instance_id)
                .bind(&row.step_id)
                .bind(row.attempt_number)
                .bind(timeout_transition_id)
                .bind(&row.attempt_transition_id)
                .bind(due_at)
                .bind(now)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    WorkflowMutationError::store(format!(
                        "Could not persist claimed workflow timeout: {error}"
                    ))
                })?;
            }
        }
        claims.push(WorkflowWorkClaim {
            timer_id: row.timer_id,
            instance_id: row.instance_id,
            step_id: row.step_id,
            kind,
            attempt_number,
            transition_id: row.transition_id,
            attempt_transition_id: row.attempt_transition_id,
            due_at: row.due_at,
            claimed_by: row.claimed_by,
            claimed_at: row.claimed_at,
        });
    }
    transaction.commit().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not commit workflow claims: {error}"))
    })?;
    Ok(claims)
}

pub async fn claim_due_workflow_work(
    state: &ServiceRuntimeState,
    worker_id: &str,
    limit: i64,
) -> Result<Vec<WorkflowWorkClaim>, WorkflowMutationError> {
    claim_due_workflow_work_at(
        state,
        worker_id,
        workflow_now(state),
        Duration::seconds(30),
        limit,
    )
    .await
}

pub async fn record_workflow_step_failure_at(
    state: &ServiceRuntimeState,
    instance_id: &str,
    step_id: &str,
    transition_id: &str,
    failure: WorkflowStepFailure,
    now: DateTime<Utc>,
) -> Result<WorkflowFailureResult, WorkflowMutationError> {
    if transition_id.trim().is_empty() {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::InvalidRequest,
            "Workflow attempt transition identity must not be empty",
        ));
    }
    let pool = state
        .store()
        .map_err(|error| WorkflowMutationError::store(error.public_message))?;
    let mut transaction = pool.begin().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not begin workflow failure: {error}"))
    })?;
    let now = postgres_precision(now);
    let result = record_failure_in_tx(
        state,
        &mut transaction,
        instance_id,
        step_id,
        InitialAttempt {
            transition_id: transition_id.to_owned(),
            scheduled_at: now,
        },
        failure,
        now,
    )
    .await?;
    transaction.commit().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not commit workflow failure: {error}"))
    })?;
    Ok(result)
}

pub async fn record_workflow_step_failure(
    state: &ServiceRuntimeState,
    instance_id: &str,
    step_id: &str,
    transition_id: &str,
    failure: WorkflowStepFailure,
) -> Result<WorkflowFailureResult, WorkflowMutationError> {
    record_workflow_step_failure_at(
        state,
        instance_id,
        step_id,
        transition_id,
        failure,
        workflow_now(state),
    )
    .await
}

pub async fn record_claimed_workflow_step_failure_at(
    state: &ServiceRuntimeState,
    claim: &WorkflowWorkClaim,
    failure: WorkflowStepFailure,
    now: DateTime<Utc>,
) -> Result<WorkflowFailureResult, WorkflowMutationError> {
    let pool = state
        .store()
        .map_err(|error| WorkflowMutationError::store(error.public_message))?;
    let mut transaction = pool.begin().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not begin claimed workflow failure: {error}"))
    })?;
    validate_claim_in_tx(&mut transaction, claim).await?;
    let result = record_failure_in_tx(
        state,
        &mut transaction,
        &claim.instance_id,
        &claim.step_id,
        claim,
        failure,
        postgres_precision(now),
    )
    .await?;
    transaction.commit().await.map_err(|error| {
        WorkflowMutationError::store(format!(
            "Could not commit claimed workflow failure: {error}"
        ))
    })?;
    Ok(result)
}

pub async fn record_claimed_workflow_step_failure(
    state: &ServiceRuntimeState,
    claim: &WorkflowWorkClaim,
    failure: WorkflowStepFailure,
) -> Result<WorkflowFailureResult, WorkflowMutationError> {
    record_claimed_workflow_step_failure_at(state, claim, failure, workflow_now(state)).await
}

pub async fn advance_claimed_workflow_retry_with_event_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    claim: &WorkflowWorkClaim,
    publication: WorkflowEventPublication,
) -> Result<WorkflowStepTransitionResult, WorkflowMutationError> {
    if claim.kind != WorkflowTimerKind::Retry {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::InvalidRequest,
            "Only a claimed workflow retry can complete business work",
        ));
    }
    validate_claim_in_tx(transaction, claim).await?;
    super::advance_workflow_step_with_event_in_tx(
        state,
        transaction,
        &claim.instance_id,
        &claim.step_id,
        &claim.attempt_transition_id,
        publication,
    )
    .await
}

#[derive(Debug)]
struct InitialAttempt {
    transition_id: String,
    scheduled_at: DateTime<Utc>,
}

trait AttemptSource {
    fn attempt_number(&self) -> u32;
    fn attempt_transition_id(&self) -> &str;
    fn scheduled_at(&self) -> DateTime<Utc>;
    fn timer_id(&self) -> Option<&str>;
}

impl AttemptSource for InitialAttempt {
    fn attempt_number(&self) -> u32 {
        1
    }

    fn attempt_transition_id(&self) -> &str {
        &self.transition_id
    }

    fn scheduled_at(&self) -> DateTime<Utc> {
        self.scheduled_at
    }

    fn timer_id(&self) -> Option<&str> {
        None
    }
}

impl AttemptSource for &WorkflowWorkClaim {
    fn attempt_number(&self) -> u32 {
        self.attempt_number
    }

    fn attempt_transition_id(&self) -> &str {
        &self.attempt_transition_id
    }

    fn scheduled_at(&self) -> DateTime<Utc> {
        self.due_at
    }

    fn timer_id(&self) -> Option<&str> {
        Some(&self.timer_id)
    }
}

#[derive(Debug, FromRow)]
struct FailureStepRow {
    instance_state: String,
    step_state: String,
    attempt_count: i32,
    max_attempts: i32,
    retry_schedule: serde_json::Value,
    next_attempt_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct ExistingAttemptRow {
    transition_id: String,
    state: String,
    failure_classification: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn record_failure_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: &str,
    source: impl AttemptSource,
    failure: WorkflowStepFailure,
    now: DateTime<Utc>,
) -> Result<WorkflowFailureResult, WorkflowMutationError> {
    if failure.code.trim().is_empty() || failure.message.trim().is_empty() {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::InvalidRequest,
            "Workflow failure code and message must not be empty",
        ));
    }
    let row = sqlx::query_as::<_, FailureStepRow>(
        r#"
        select instance.state as instance_state, step.state as step_state,
               step.attempt_count, step.max_attempts, step.retry_schedule,
               step.next_attempt_at
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.service_id = $1 and instance.instance_id = $2 and step.step_id = $3
        for update of instance, step
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .bind(step_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock failed workflow step: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StepNotFound,
            format!("Workflow step `{step_id}` was not found in instance `{instance_id}`"),
        )
    })?;
    let attempt_number = source.attempt_number();
    let previous_attempt_count = u32::try_from(row.attempt_count).map_err(|_| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StoredStateInvalid,
            "Stored workflow attempt count is invalid",
        )
    })?;
    let existing = sqlx::query_as::<_, ExistingAttemptRow>(
        r#"
        select transition_id, state, failure_classification
        from platform.service_workflow_step_attempts
        where step_id = $1 and attempt_number = $2
        for update
        "#,
    )
    .bind(step_id)
    .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect workflow attempt: {error}"))
    })?;
    if let Some(existing) = &existing {
        if existing.transition_id != source.attempt_transition_id() {
            return Err(WorkflowMutationError::new(
                super::WorkflowErrorCode::TransitionConflict,
                format!("Workflow attempt {attempt_number} already has another transition"),
            ));
        }
        if existing.state == "failed" {
            let classification = existing
                .failure_classification
                .as_deref()
                .ok_or_else(|| {
                    WorkflowMutationError::new(
                        super::WorkflowErrorCode::StoredStateInvalid,
                        "Failed workflow attempt is missing its failure classification",
                    )
                })
                .and_then(|value| {
                    WorkflowFailureClassification::parse(value)
                        .map_err(|error| WorkflowMutationError::new(error.code, error.message))
                })?;
            return Ok(WorkflowFailureResult {
                disposition: WorkflowFailureDisposition::Duplicate,
                instance_id: instance_id.to_owned(),
                step_id: step_id.to_owned(),
                attempt_number,
                attempt_count: previous_attempt_count,
                classification,
                next_attempt_at: row.next_attempt_at,
                terminal_exhausted: row.step_state == "exhausted",
            });
        }
        if existing.state != "running" {
            return Err(WorkflowMutationError::new(
                super::WorkflowErrorCode::TransitionConflict,
                format!("Workflow attempt {attempt_number} already completed successfully"),
            ));
        }
    }
    if source.timer_id().is_none() && previous_attempt_count != 0 {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::TransitionConflict,
            "A persisted workflow retry claim is required after the original attempt",
        ));
    }
    if row.instance_state != "running" || row.step_state != "pending" {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::TransitionConflict,
            format!("Workflow step `{step_id}` is not pending in a running instance"),
        ));
    }
    if attempt_number != previous_attempt_count + 1 {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::TransitionConflict,
            format!("Workflow attempt {attempt_number} is not the next durable attempt"),
        ));
    }
    let max_attempts = u32::try_from(row.max_attempts).map_err(|_| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StoredStateInvalid,
            "Stored workflow maximum attempt count is invalid",
        )
    })?;
    let retry_schedule: Vec<u64> = serde_json::from_value(row.retry_schedule).map_err(|error| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::StoredStateInvalid,
            format!("Stored workflow retry schedule is invalid: {error}"),
        )
    })?;
    let terminal = failure.classification == WorkflowFailureClassification::Permanent
        || attempt_number >= max_attempts;
    let next_attempt_at = if terminal {
        None
    } else {
        let delay_index = usize::try_from(attempt_number - 1).map_err(|_| {
            WorkflowMutationError::new(
                super::WorkflowErrorCode::StoredStateInvalid,
                "Workflow retry schedule index is invalid",
            )
        })?;
        let delay = retry_schedule.get(delay_index).copied().ok_or_else(|| {
            WorkflowMutationError::new(
                super::WorkflowErrorCode::StoredStateInvalid,
                "Workflow retry schedule is missing the next attempt delay",
            )
        })?;
        Some(add_milliseconds(now, delay)?)
    };
    if existing.is_some() {
        sqlx::query(
            r#"
            update platform.service_workflow_step_attempts
            set state = 'failed', failure_classification = $3,
                failure_code = $4, failure_message = $5, completed_at = $6
            where step_id = $1 and attempt_number = $2 and state = 'running'
            "#,
        )
        .bind(step_id)
        .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
        .bind(failure.classification.as_str())
        .bind(&failure.code)
        .bind(&failure.message)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not fail workflow attempt: {error}"))
        })?;
    } else {
        sqlx::query(
            r#"
            insert into platform.service_workflow_step_attempts (
                attempt_id, instance_id, step_id, attempt_number,
                transition_id, state, failure_classification,
                failure_code, failure_message, scheduled_at, started_at, completed_at
            ) values ($1, $2, $3, $4, $5, 'failed', $6, $7, $8, $9, $10, $10)
            "#,
        )
        .bind(format!("workflow_attempt_{}", Uuid::now_v7()))
        .bind(instance_id)
        .bind(step_id)
        .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
        .bind(source.attempt_transition_id())
        .bind(failure.classification.as_str())
        .bind(&failure.code)
        .bind(&failure.message)
        .bind(source.scheduled_at())
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not persist failed workflow attempt: {error}"
            ))
        })?;
    }
    if let Some(timer_id) = source.timer_id() {
        sqlx::query(
            r#"
            update platform.service_workflow_timers
            set state = 'completed', completed_at = $2, updated_at = $2
            where timer_id = $1 and state in ('claimed', 'completed')
            "#,
        )
        .bind(timer_id)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not complete workflow timer: {error}"))
        })?;
    }
    sqlx::query(
        r#"
        update platform.service_workflow_timers
        set state = 'cancelled', completed_at = $3, updated_at = $3
        where step_id = $1 and attempt_number = $2
          and state in ('pending', 'claimed')
        "#,
    )
    .bind(step_id)
    .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not cancel workflow attempt timers: {error}"))
    })?;
    if let Some(due_at) = next_attempt_at {
        let next_attempt = attempt_number + 1;
        let transition_id = format!("{step_id}:retry:{next_attempt}");
        sqlx::query(
            r#"
            insert into platform.service_workflow_timers (
                timer_id, instance_id, step_id, kind, attempt_number,
                transition_id, attempt_transition_id, due_at, state,
                created_at, updated_at
            ) values ($1, $2, $3, 'retry', $4, $5, $5, $6,
                      'pending', $7, $7)
            on conflict (step_id, transition_id) do nothing
            "#,
        )
        .bind(format!("workflow_timer_{}", Uuid::now_v7()))
        .bind(instance_id)
        .bind(step_id)
        .bind(i32::try_from(next_attempt).unwrap_or(i32::MAX))
        .bind(transition_id)
        .bind(due_at)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not schedule workflow retry: {error}"))
        })?;
    }
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = $4, attempt_count = $3, next_attempt_at = $5,
            failure_classification = $6, failure_code = $7,
            failure_message = $8, exhausted_at = $9, updated_at = $10
        where instance_id = $1 and step_id = $2 and state = 'pending'
        "#,
    )
    .bind(instance_id)
    .bind(step_id)
    .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
    .bind(if terminal { "exhausted" } else { "pending" })
    .bind(next_attempt_at)
    .bind(failure.classification.as_str())
    .bind(&failure.code)
    .bind(&failure.message)
    .bind(terminal.then_some(now))
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist workflow failure: {error}"))
    })?;
    if terminal {
        let failure_evidence = serde_json::to_value(WorkflowFailureEvidence::new(
            failure.code.clone(),
            failure.message.clone(),
            "inspect_workflow",
        ))
        .map_err(|error| {
            WorkflowMutationError::new(
                super::WorkflowErrorCode::StoredStateInvalid,
                format!("Could not encode exhausted workflow failure evidence: {error}"),
            )
        })?;
        sqlx::query(
            r#"
            update platform.service_workflow_instances
            set state = 'failed', failure_evidence = $2, terminal_transition_id = $3,
                updated_at = $4
            where instance_id = $1 and state = 'running'
            "#,
        )
        .bind(instance_id)
        .bind(failure_evidence)
        .bind(source.attempt_transition_id())
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not exhaust workflow instance: {error}"))
        })?;
    }
    Ok(WorkflowFailureResult {
        disposition: if terminal {
            WorkflowFailureDisposition::Exhausted
        } else {
            WorkflowFailureDisposition::RetryScheduled
        },
        instance_id: instance_id.to_owned(),
        step_id: step_id.to_owned(),
        attempt_number,
        attempt_count: attempt_number,
        classification: failure.classification,
        next_attempt_at,
        terminal_exhausted: terminal,
    })
}

#[derive(Debug, FromRow)]
struct ClaimValidationRow {
    instance_id: String,
    step_id: String,
    kind: String,
    attempt_number: i32,
    transition_id: String,
    attempt_transition_id: String,
    due_at: DateTime<Utc>,
    state: String,
    claimed_by: Option<String>,
    claimed_at: Option<DateTime<Utc>>,
}

pub(crate) async fn validate_claim_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    claim: &WorkflowWorkClaim,
) -> Result<(), WorkflowMutationError> {
    let stored = sqlx::query_as::<_, ClaimValidationRow>(
        r#"
        select instance_id, step_id, kind, attempt_number, transition_id,
               attempt_transition_id, due_at, state, claimed_by, claimed_at
        from platform.service_workflow_timers
        where timer_id = $1
        for update
        "#,
    )
    .bind(&claim.timer_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not validate workflow claim: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            super::WorkflowErrorCode::TransitionConflict,
            "Workflow claim no longer exists",
        )
    })?;
    let identity_matches = stored.instance_id == claim.instance_id
        && stored.step_id == claim.step_id
        && stored.kind == claim.kind.as_str()
        && stored.attempt_number == i32::try_from(claim.attempt_number).unwrap_or(i32::MAX)
        && stored.transition_id == claim.transition_id
        && stored.attempt_transition_id == claim.attempt_transition_id
        && stored.due_at == claim.due_at;
    let active_matches = stored.state == "claimed"
        && stored.claimed_by.as_deref() == Some(claim.claimed_by.as_str())
        && stored.claimed_at == Some(claim.claimed_at);
    let already_resolved = matches!(stored.state.as_str(), "completed" | "cancelled");
    if !identity_matches || (!active_matches && !already_resolved) {
        return Err(WorkflowMutationError::new(
            super::WorkflowErrorCode::TransitionConflict,
            "Workflow claim was superseded or reclaimed by another worker",
        ));
    }
    Ok(())
}

pub(super) async fn record_workflow_step_success_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: &str,
    attempt_number: u32,
    transition_id: &str,
    now: DateTime<Utc>,
) -> Result<(), WorkflowMutationError> {
    let existing = sqlx::query_as::<_, ExistingAttemptRow>(
        r#"
        select transition_id, state, failure_classification
        from platform.service_workflow_step_attempts
        where step_id = $1 and attempt_number = $2
        for update
        "#,
    )
    .bind(step_id)
    .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!(
            "Could not inspect successful workflow attempt: {error}"
        ))
    })?;
    match existing {
        Some(existing)
            if existing.transition_id == transition_id && existing.state == "running" =>
        {
            sqlx::query(
                r#"
                update platform.service_workflow_step_attempts
                set state = 'succeeded', completed_at = $3
                where step_id = $1 and attempt_number = $2 and state = 'running'
                "#,
            )
            .bind(step_id)
            .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
            .bind(now)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                WorkflowMutationError::store(format!(
                    "Could not complete workflow attempt: {error}"
                ))
            })?;
        }
        Some(existing) => {
            return Err(WorkflowMutationError::new(
                super::WorkflowErrorCode::TransitionConflict,
                format!(
                    "Workflow attempt {attempt_number} already uses transition `{}` with state `{}`",
                    existing.transition_id, existing.state
                ),
            ));
        }
        None if attempt_number == 1 => {
            sqlx::query(
                r#"
                insert into platform.service_workflow_step_attempts (
                    attempt_id, instance_id, step_id, attempt_number,
                    transition_id, state, scheduled_at, started_at, completed_at
                ) values ($1, $2, $3, $4, $5, 'succeeded', $6, $6, $6)
                "#,
            )
            .bind(format!("workflow_attempt_{}", Uuid::now_v7()))
            .bind(instance_id)
            .bind(step_id)
            .bind(i32::try_from(attempt_number).unwrap_or(i32::MAX))
            .bind(transition_id)
            .bind(now)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                WorkflowMutationError::store(format!(
                    "Could not persist successful workflow attempt: {error}"
                ))
            })?;
        }
        None => {
            return Err(WorkflowMutationError::new(
                super::WorkflowErrorCode::TransitionConflict,
                "A persisted workflow retry claim is required after the original attempt",
            ));
        }
    }
    sqlx::query(
        r#"
        update platform.service_workflow_timers
        set state = case when transition_id = $2 then 'completed' else 'cancelled' end,
            completed_at = $3, updated_at = $3
        where step_id = $1 and state in ('pending', 'claimed')
        "#,
    )
    .bind(step_id)
    .bind(transition_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not resolve workflow timers: {error}"))
    })?;
    Ok(())
}
