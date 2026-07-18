use crate::{
    ServiceEventPublisher, ServiceRuntimeState, WorkflowApiError, WorkflowErrorCode,
    WorkflowEventPublication, WorkflowFailureEvidence, WorkflowInstanceState,
    WorkflowMutationError, WorkflowOutgoingWorkInspection, WorkflowTransitionDisposition,
    WorkflowWorkClaim, append_persisted_workflow_story_segment_in_tx, event_type_for_contract,
    postgres_now, validate_outgoing_context,
};
use chrono::{DateTime, Utc};
use lenso_contracts::{WorkflowCompensationDeclaration, WorkflowDataContract};
use lenso_service::{CausationContext, EventContent, EventContext, EventEnvelope};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEffectState {
    Completed,
    Compensated,
    CompensationFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEffectInspection {
    pub effect_id: String,
    pub step_id: String,
    pub definition_step_name: String,
    pub transition_id: String,
    pub outgoing_work: WorkflowOutgoingWorkInspection,
    pub compensation_name: String,
    pub compensation_order: u32,
    pub compensation_contract: WorkflowDataContract,
    pub compensation_completion_contract: WorkflowDataContract,
    pub state: WorkflowEffectState,
    pub completed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCompensationState {
    Pending,
    Dispatched,
    Compensated,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCompensationSelectionKind {
    Timeout,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCompensationAttemptState {
    Dispatched,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompensationAttemptInspection {
    pub attempt_id: String,
    pub attempt_number: u32,
    pub transition_id: String,
    pub state: WorkflowCompensationAttemptState,
    pub failure: Option<WorkflowFailureEvidence>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompensationInspection {
    pub compensation_id: String,
    pub effect_id: String,
    pub step_id: String,
    pub name: String,
    pub execution_order: u32,
    pub contract: WorkflowDataContract,
    pub completion_contract: WorkflowDataContract,
    pub state: WorkflowCompensationState,
    pub attempt_count: u32,
    pub transition_id: Option<String>,
    pub outgoing_work: Option<WorkflowOutgoingWorkInspection>,
    pub failure: Option<WorkflowFailureEvidence>,
    pub selection_kind: WorkflowCompensationSelectionKind,
    pub selected_by_transition_id: String,
    /// Legacy v1 field retained for compatibility. Use
    /// `selected_by_transition_id` together with `selection_kind`.
    pub selected_by_timeout_transition_id: String,
    pub attempts: Vec<WorkflowCompensationAttemptInspection>,
    pub selected_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowHistoryEntry {
    pub history_id: String,
    pub step_id: Option<String>,
    pub compensation_id: Option<String>,
    pub kind: String,
    pub detail: Value,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompensationSelection {
    pub compensation_id: String,
    pub effect_id: String,
    pub step_id: String,
    pub name: String,
    pub execution_order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompensationSelectionResult {
    pub disposition: WorkflowTransitionDisposition,
    pub instance_id: String,
    pub timed_out_step_id: String,
    pub timeout_transition_id: String,
    pub compensations: Vec<WorkflowCompensationSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompensationResult {
    pub disposition: WorkflowTransitionDisposition,
    pub instance_id: String,
    pub compensation_id: String,
    pub effect_id: String,
    pub transition_id: String,
    pub outgoing_event_id: String,
    pub workflow_state: WorkflowInstanceState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompensationFailureResult {
    pub disposition: WorkflowTransitionDisposition,
    pub instance_id: String,
    pub compensation_id: String,
    pub effect_id: String,
    pub transition_id: String,
    pub failure: WorkflowFailureEvidence,
    pub workflow_state: WorkflowInstanceState,
}

#[derive(Debug, Default)]
pub(crate) struct WorkflowCompensationEvidence {
    pub effects: Vec<WorkflowEffectInspection>,
    pub compensations: Vec<WorkflowCompensationInspection>,
    pub history: Vec<WorkflowHistoryEntry>,
}

#[derive(Debug, FromRow)]
struct EffectRow {
    effect_id: String,
    step_id: String,
    definition_step_name: String,
    effect_transition_id: String,
    effect_outgoing_work: Value,
    compensation_name: String,
    compensation_order: i32,
    compensation_contract_id: String,
    compensation_contract_version: String,
    compensation_completion_contract_id: String,
    compensation_completion_contract_version: String,
    state: String,
    completed_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct CompensationRow {
    compensation_id: String,
    effect_id: String,
    step_id: String,
    name: String,
    execution_order: i32,
    contract_id: String,
    contract_version: String,
    completion_contract_id: String,
    completion_contract_version: String,
    state: String,
    attempt_count: i32,
    transition_id: Option<String>,
    outgoing_work: Option<Value>,
    failure_evidence: Option<Value>,
    selection_kind: String,
    selected_by_timeout_transition_id: String,
    selected_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct CompensationAttemptRow {
    compensation_id: String,
    attempt_id: String,
    attempt_number: i32,
    transition_id: String,
    state: String,
    failure_evidence: Option<Value>,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct HistoryRow {
    history_id: String,
    step_id: Option<String>,
    compensation_id: Option<String>,
    kind: String,
    detail: Value,
    recorded_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct TimeoutSelectionStateRow {
    instance_state: String,
    step_state: String,
}

#[derive(Debug, FromRow)]
struct CompensationExecutionRow {
    compensation_id: String,
    effect_id: String,
    instance_id: String,
    step_id: String,
    name: String,
    execution_order: i32,
    contract_id: String,
    contract_version: String,
    completion_contract_id: String,
    completion_contract_version: String,
    state: String,
    attempt_count: i32,
    transition_id: Option<String>,
    outgoing_work: Option<Value>,
    failure_evidence: Option<Value>,
    instance_state: String,
    control_state: String,
    terminal_intent: Option<String>,
    workflow_context: Option<Value>,
}

pub(crate) async fn record_compensatable_effect_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: &str,
    definition_step_name: &str,
    transition_id: &str,
    outgoing_work: &WorkflowOutgoingWorkInspection,
    compensation: &WorkflowCompensationDeclaration,
    now: DateTime<Utc>,
) -> Result<String, WorkflowMutationError> {
    let effect_id = format!("{step_id}:effect");
    let effect_contract_id = outgoing_work.contract_id.clone();
    let effect_contract_version = outgoing_work.contract_version.clone();
    let outgoing_work = serde_json::to_value(outgoing_work).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode workflow effect evidence: {error}"),
        )
    })?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_effects (
            effect_id, instance_id, step_id, definition_step_name,
            effect_transition_id, effect_outgoing_work, compensation_name,
            compensation_order, compensation_contract_id,
            compensation_contract_version, compensation_completion_contract_id,
            compensation_completion_contract_version, state, completed_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                  'completed', $13, $13)
        "#,
    )
    .bind(&effect_id)
    .bind(instance_id)
    .bind(step_id)
    .bind(definition_step_name)
    .bind(transition_id)
    .bind(outgoing_work)
    .bind(&compensation.name)
    .bind(i32::try_from(compensation.order).unwrap_or(i32::MAX))
    .bind(&compensation.contract.contract_id)
    .bind(&compensation.contract.version)
    .bind(&compensation.completion_contract.contract_id)
    .bind(&compensation.completion_contract.version)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist workflow effect: {error}"))
    })?;
    insert_history(
        transaction,
        &format!("{effect_id}:history:completed"),
        instance_id,
        Some(step_id),
        None,
        "effect_completed",
        json!({
            "effectId": effect_id,
            "transitionId": transition_id,
            "compensationName": compensation.name,
            "compensationOrder": compensation.order,
        }),
        now,
    )
    .await?;
    insert_story_segment(
        state,
        transaction,
        instance_id,
        Some(step_id),
        None,
        None,
        &format!("workflow:{effect_id}:completed"),
        &format!("workflow effect {effect_id}"),
        &effect_contract_id,
        &effect_contract_version,
        "completed",
        1,
        Some(transition_id),
        now,
    )
    .await?;
    Ok(effect_id)
}

/// Selects all completed declared effects for compensation after one claimed
/// durable timeout. The selection and deterministic order survive restart.
#[allow(clippy::too_many_lines)]
pub async fn select_workflow_compensations_after_timeout_at(
    state: &ServiceRuntimeState,
    claim: &WorkflowWorkClaim,
    now: DateTime<Utc>,
) -> Result<WorkflowCompensationSelectionResult, WorkflowMutationError> {
    if claim.kind != crate::WorkflowTimerKind::StepTimeout {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Only a claimed Workflow step timeout can select compensation",
        ));
    }
    let now = postgres_precision(now);
    let pool = state
        .store()
        .map_err(|error| WorkflowMutationError::store(error.public_message))?;
    let mut transaction = pool.begin().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not begin compensation selection: {error}"))
    })?;
    crate::workflow::recovery::validate_claim_in_tx(&mut transaction, claim).await?;
    let selection_state = sqlx::query_as::<_, TimeoutSelectionStateRow>(
        r#"
        select instance.state as instance_state, step.state as step_state
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.service_id = $1 and instance.instance_id = $2 and step.step_id = $3
        for update of instance, step
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(&claim.instance_id)
    .bind(&claim.step_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock timeout selection: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StepNotFound,
            format!(
                "Workflow step `{}` was not found in instance `{}`",
                claim.step_id, claim.instance_id
            ),
        )
    })?;

    if selection_state.instance_state != "running" || selection_state.step_state != "pending" {
        let existing =
            load_selection_in_tx(&mut transaction, &claim.instance_id, &claim.transition_id)
                .await?;
        if !existing.is_empty()
            && matches!(
                selection_state.instance_state.as_str(),
                "compensating" | "compensated" | "compensation_failed"
            )
        {
            transaction.commit().await.map_err(|error| {
                WorkflowMutationError::store(format!(
                    "Could not commit duplicate compensation selection: {error}"
                ))
            })?;
            return Ok(WorkflowCompensationSelectionResult {
                disposition: WorkflowTransitionDisposition::Duplicate,
                instance_id: claim.instance_id.clone(),
                timed_out_step_id: claim.step_id.clone(),
                timeout_transition_id: claim.transition_id.clone(),
                compensations: existing,
            });
        }
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Workflow step `{}` cannot select compensation from state `{}`",
                claim.step_id, selection_state.instance_state
            ),
        ));
    }

    let effects = sqlx::query_as::<_, EffectRow>(
        r#"
        select effect_id, step_id, definition_step_name, effect_transition_id,
               effect_outgoing_work, compensation_name, compensation_order,
               compensation_contract_id, compensation_contract_version,
               compensation_completion_contract_id,
               compensation_completion_contract_version,
               state, completed_at, updated_at
        from platform.service_workflow_effects
        where instance_id = $1 and state = 'completed'
        order by compensation_order, effect_id
        for update
        "#,
    )
    .bind(&claim.instance_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock compensatable effects: {error}"))
    })?;
    if effects.is_empty() {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            "Timed-out Workflow has no completed declared effects to compensate",
        ));
    }

    persist_timeout_failure(&mut transaction, claim, now).await?;
    let mut selections = Vec::with_capacity(effects.len());
    for effect in effects {
        let compensation_id = format!(
            "{}:compensation:{}",
            effect.effect_id, effect.compensation_name
        );
        sqlx::query(
            r#"
            insert into platform.service_workflow_compensations (
                compensation_id, effect_id, instance_id, step_id, name,
                execution_order, contract_id, contract_version, state,
                completion_contract_id, completion_contract_version,
                selected_by_timeout_transition_id, selected_at, updated_at
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9, $10, $11, $12, $12)
            "#,
        )
        .bind(&compensation_id)
        .bind(&effect.effect_id)
        .bind(&claim.instance_id)
        .bind(&effect.step_id)
        .bind(&effect.compensation_name)
        .bind(effect.compensation_order)
        .bind(&effect.compensation_contract_id)
        .bind(&effect.compensation_contract_version)
        .bind(&effect.compensation_completion_contract_id)
        .bind(&effect.compensation_completion_contract_version)
        .bind(&claim.transition_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not persist Workflow compensation: {error}"
            ))
        })?;
        insert_history(
            &mut transaction,
            &format!("{compensation_id}:history:selected"),
            &claim.instance_id,
            Some(&effect.step_id),
            Some(&compensation_id),
            "compensation_selected",
            json!({
                "effectId": effect.effect_id,
                "compensationId": compensation_id,
                "executionOrder": effect.compensation_order,
                "timeoutTransitionId": claim.transition_id,
            }),
            now,
        )
        .await?;
        selections.push(WorkflowCompensationSelection {
            compensation_id,
            effect_id: effect.effect_id,
            step_id: effect.step_id,
            name: effect.compensation_name,
            execution_order: u32::try_from(effect.compensation_order).map_err(|_| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    "Stored compensation order is invalid",
                )
            })?,
        });
    }
    sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = 'compensating', updated_at = $2
        where instance_id = $1 and state = 'running'
        "#,
    )
    .bind(&claim.instance_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not select Workflow compensation: {error}"))
    })?;
    insert_history(
        &mut transaction,
        &format!("{}:history:timeout", claim.transition_id),
        &claim.instance_id,
        Some(&claim.step_id),
        None,
        "step_timed_out",
        json!({
            "attemptNumber": claim.attempt_number,
            "timerId": claim.timer_id,
            "timeoutTransitionId": claim.transition_id,
            "selectedCompensationCount": selections.len(),
        }),
        now,
    )
    .await?;
    insert_story_segment(
        state,
        &mut transaction,
        &claim.instance_id,
        Some(&claim.step_id),
        None,
        None,
        &format!(
            "workflow:{}:timeout:{}",
            claim.instance_id, claim.transition_id
        ),
        &format!(
            "workflow {} timeout selected compensation",
            claim.instance_id
        ),
        "lenso.workflow-timer",
        "v1",
        "timed_out",
        claim.attempt_number,
        Some(&claim.transition_id),
        now,
    )
    .await?;
    transaction.commit().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not commit compensation selection: {error}"))
    })?;
    Ok(WorkflowCompensationSelectionResult {
        disposition: WorkflowTransitionDisposition::Applied,
        instance_id: claim.instance_id.clone(),
        timed_out_step_id: claim.step_id.clone(),
        timeout_transition_id: claim.transition_id.clone(),
        compensations: selections,
    })
}

/// Selects declared compensation for a cooperative, Approval Boundary guarded
/// cancellation. The caller owns the surrounding Workflow Instance lock and
/// transaction so ordinary work and terminal intent change atomically.
pub(crate) async fn select_workflow_compensations_for_cancel_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    cancellation_transition_id: &str,
    now: DateTime<Utc>,
) -> Result<Vec<WorkflowCompensationSelection>, WorkflowMutationError> {
    let effects = sqlx::query_as::<_, EffectRow>(
        r#"
        select effect_id, step_id, definition_step_name, effect_transition_id,
               effect_outgoing_work, compensation_name, compensation_order,
               compensation_contract_id, compensation_contract_version,
               compensation_completion_contract_id,
               compensation_completion_contract_version,
               state, completed_at, updated_at
        from platform.service_workflow_effects
        where instance_id = $1 and state = 'completed'
        order by compensation_order, effect_id
        for update
        "#,
    )
    .bind(instance_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!(
            "Could not lock cancellable Workflow effects: {error}"
        ))
    })?;
    let mut selections = Vec::with_capacity(effects.len());
    for effect in effects {
        let compensation_id = format!(
            "{}:compensation:{}",
            effect.effect_id, effect.compensation_name
        );
        sqlx::query(
            r#"
            insert into platform.service_workflow_compensations (
                compensation_id, effect_id, instance_id, step_id, name,
                execution_order, contract_id, contract_version, state,
                completion_contract_id, completion_contract_version,
                selection_kind, selected_by_timeout_transition_id,
                selected_at, updated_at
            ) values (
                $1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9, $10,
                'cancel', $11, $12, $12
            )
            "#,
        )
        .bind(&compensation_id)
        .bind(&effect.effect_id)
        .bind(instance_id)
        .bind(&effect.step_id)
        .bind(&effect.compensation_name)
        .bind(effect.compensation_order)
        .bind(&effect.compensation_contract_id)
        .bind(&effect.compensation_contract_version)
        .bind(&effect.compensation_completion_contract_id)
        .bind(&effect.compensation_completion_contract_version)
        .bind(cancellation_transition_id)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not persist cancellation compensation: {error}"
            ))
        })?;
        insert_history(
            transaction,
            &format!("{compensation_id}:history:selected"),
            instance_id,
            Some(&effect.step_id),
            Some(&compensation_id),
            "compensation_selected",
            json!({
                "effectId": effect.effect_id,
                "compensationId": compensation_id,
                "executionOrder": effect.compensation_order,
                "selectionKind": "cancel",
                "cancellationTransitionId": cancellation_transition_id,
            }),
            now,
        )
        .await?;
        selections.push(WorkflowCompensationSelection {
            compensation_id,
            effect_id: effect.effect_id,
            step_id: effect.step_id,
            name: effect.compensation_name,
            execution_order: u32::try_from(effect.compensation_order).map_err(|_| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    "Stored compensation order is invalid",
                )
            })?,
        });
    }
    insert_history(
        transaction,
        &format!("{cancellation_transition_id}:history:cancel-requested"),
        instance_id,
        None,
        None,
        "workflow_cancel_requested",
        json!({
            "cancellationTransitionId": cancellation_transition_id,
            "selectedCompensationCount": selections.len(),
            "expectedTerminalState": "cancelled",
        }),
        now,
    )
    .await?;
    insert_story_segment(
        state,
        transaction,
        instance_id,
        None,
        None,
        Some(cancellation_transition_id),
        &format!("workflow:{instance_id}:cancel:{cancellation_transition_id}"),
        &format!("workflow {instance_id} cooperative cancel requested"),
        "lenso.workflow-operator-result",
        "v1",
        "cancelling",
        1,
        Some(cancellation_transition_id),
        now,
    )
    .await?;
    Ok(selections)
}

/// Dispatches one selected compensation through its declared request Event Contract.
/// The Workflow remains compensating until the owning Service confirms the
/// reversed business effect through the declared completion Event Contract.
#[allow(clippy::too_many_lines)]
pub async fn dispatch_workflow_compensation_with_event_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    compensation_id: &str,
    transition_id: &str,
    publication: WorkflowEventPublication,
) -> Result<WorkflowCompensationResult, WorkflowMutationError> {
    if compensation_id.trim().is_empty()
        || transition_id.trim().is_empty()
        || publication.event_id.trim().is_empty()
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Workflow compensation and outgoing Event identities must not be empty",
        ));
    }
    let expected_event_id = format!("{compensation_id}:request");
    if publication.event_id != expected_event_id {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            format!("Workflow compensation Event identity must be `{expected_event_id}`"),
        ));
    }
    let row = lock_compensation(state, transaction, compensation_id).await?;
    if matches!(row.state.as_str(), "dispatched" | "compensated") {
        let outgoing = decode_outgoing_work(row.outgoing_work)?;
        if row.transition_id.as_deref() == Some(transition_id)
            && outgoing.as_ref().is_some_and(|work| {
                work.event_id == publication.event_id
                    && work.consumer_id == publication.consumer_id
                    && work.contract_id == publication.contract_id
                    && work.contract_version == publication.contract_version
            })
        {
            return Ok(WorkflowCompensationResult {
                disposition: WorkflowTransitionDisposition::Duplicate,
                instance_id: row.instance_id,
                compensation_id: row.compensation_id,
                effect_id: row.effect_id,
                transition_id: transition_id.to_owned(),
                outgoing_event_id: publication.event_id,
                workflow_state: current_compensation_workflow_state(transaction, compensation_id)
                    .await?,
            });
        }
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` was dispatched through another transition"),
        ));
    }
    if row.control_state != "active" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Workflow Instance `{}` is paused; resume it before dispatching compensation",
                row.instance_id
            ),
        ));
    }
    if row.state == "failed" || row.instance_state == "compensation_failed" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` requires human intervention"),
        ));
    }
    if row.state != "pending" || row.instance_state != "compensating" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` is not pending in a compensating Workflow"),
        ));
    }
    ensure_compensation_order(transaction, &row).await?;
    if row.contract_id != publication.contract_id
        || row.contract_version != publication.contract_version
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::EventContractNotDeclared,
            format!(
                "Compensation `{compensation_id}` must use declared contract `{}` version `{}`",
                row.contract_id, row.contract_version
            ),
        ));
    }
    let envelope = compensation_envelope(state, &row, &publication)?;
    let outgoing_work = WorkflowOutgoingWorkInspection {
        kind: "event_contract".to_owned(),
        consumer_id: publication.consumer_id.clone(),
        event_id: envelope.event_id.clone(),
        contract_id: envelope.contract_id.clone(),
        contract_version: envelope.contract_version.clone(),
    };
    let outgoing_work_json = serde_json::to_value(&outgoing_work).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode compensation outgoing work: {error}"),
        )
    })?;
    let now = postgres_now();
    ServiceEventPublisher
        .publish_in_tx(transaction, &publication.consumer_id, &envelope)
        .await
        .map_err(|error| WorkflowMutationError::store(error.message))?;
    let attempt_number = row.attempt_count + 1;
    sqlx::query(
        r#"
        insert into platform.service_workflow_compensation_attempts (
            attempt_id, compensation_id, instance_id, attempt_number,
            transition_id, state, started_at
        ) values ($1, $2, $3, $4, $5, 'dispatched', $6)
        "#,
    )
    .bind(format!("workflow_compensation_attempt_{}", Uuid::now_v7()))
    .bind(compensation_id)
    .bind(&row.instance_id)
    .bind(attempt_number)
    .bind(transition_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist compensation attempt: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_compensations
        set state = 'dispatched', attempt_count = $2, transition_id = $3,
            outgoing_work = $4, updated_at = $5
        where compensation_id = $1 and state = 'pending'
        "#,
    )
    .bind(compensation_id)
    .bind(attempt_number)
    .bind(transition_id)
    .bind(outgoing_work_json)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not dispatch compensation: {error}"))
    })?;
    insert_history(
        transaction,
        &format!("{compensation_id}:history:attempt:{attempt_number}:dispatched"),
        &row.instance_id,
        Some(&row.step_id),
        Some(compensation_id),
        "compensation_dispatched",
        json!({
            "attemptNumber": attempt_number,
            "effectId": row.effect_id,
            "requestEventId": envelope.event_id,
            "transitionId": transition_id,
        }),
        now,
    )
    .await?;
    insert_story_segment(
        state,
        transaction,
        &row.instance_id,
        Some(&row.step_id),
        Some(compensation_id),
        None,
        &format!("workflow:{compensation_id}:attempt:{attempt_number}:dispatched"),
        &format!("workflow compensation {compensation_id}"),
        &row.contract_id,
        &row.contract_version,
        "dispatched",
        u32::try_from(attempt_number).unwrap_or(u32::MAX),
        Some(transition_id),
        now,
    )
    .await?;
    Ok(WorkflowCompensationResult {
        disposition: WorkflowTransitionDisposition::Applied,
        instance_id: row.instance_id,
        compensation_id: row.compensation_id,
        effect_id: row.effect_id,
        transition_id: transition_id.to_owned(),
        outgoing_event_id: envelope.event_id,
        workflow_state: WorkflowInstanceState::Compensating,
    })
}

/// Completes a dispatched compensation only after the remote Service confirms
/// that it reversed the stable effect through the declared Event Contract.
#[allow(clippy::too_many_lines)]
pub async fn complete_workflow_compensation_from_event_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    envelope: &EventEnvelope,
) -> Result<WorkflowCompensationResult, WorkflowMutationError> {
    let compensation_id = required_content_identity(envelope, "compensationId")?;
    let row = lock_compensation(state, transaction, compensation_id).await?;
    let expected_event_id = format!("{compensation_id}:completed");
    if envelope.event_id != expected_event_id
        || envelope.contract_id != row.completion_contract_id
        || envelope.contract_version != row.completion_contract_version
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::EventContractNotDeclared,
            format!(
                "Compensation `{compensation_id}` completion must use Event `{expected_event_id}` and contract `{}` version `{}`",
                row.completion_contract_id, row.completion_contract_version
            ),
        ));
    }
    validate_completion_identity(&row, envelope)?;
    if row.state == "compensated" {
        return Ok(WorkflowCompensationResult {
            disposition: WorkflowTransitionDisposition::Duplicate,
            instance_id: row.instance_id,
            compensation_id: row.compensation_id,
            effect_id: row.effect_id,
            transition_id: row.transition_id.ok_or_else(|| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    "Completed compensation is missing its transition identity",
                )
            })?,
            outgoing_event_id: envelope.event_id.clone(),
            workflow_state: current_compensation_workflow_state(transaction, compensation_id)
                .await?,
        });
    }
    if row.state == "failed" || row.instance_state == "compensation_failed" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` requires human intervention"),
        ));
    }
    if row.state != "dispatched" || row.instance_state != "compensating" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` is not awaiting remote completion"),
        ));
    }
    ensure_compensation_order(transaction, &row).await?;
    let transition_id = row.transition_id.clone().ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            "Dispatched compensation is missing its transition identity",
        )
    })?;
    let request_event_id = decode_outgoing_work(row.outgoing_work.clone())?
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                "Dispatched compensation is missing outgoing work",
            )
        })?
        .event_id;
    if envelope
        .context
        .causation
        .as_ref()
        .is_none_or(|causation| causation.causation_id != request_event_id)
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Compensation completion must be caused by its stable request Event",
        ));
    }
    let now = postgres_now();
    sqlx::query(
        r#"
        update platform.service_workflow_compensation_attempts
        set state = 'succeeded', completed_at = $3
        where compensation_id = $1 and transition_id = $2 and state = 'dispatched'
        "#,
    )
    .bind(compensation_id)
    .bind(&transition_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not complete compensation attempt: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_compensations
        set state = 'compensated', completed_at = $2, updated_at = $2
        where compensation_id = $1 and state = 'dispatched'
        "#,
    )
    .bind(compensation_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not complete compensation: {error}"))
    })?;
    sqlx::query(
        "update platform.service_workflow_effects set state = 'compensated', updated_at = $2 where effect_id = $1 and state = 'completed'",
    )
    .bind(&row.effect_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not mark effect compensated: {error}"))
    })?;
    let remaining: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_compensations where instance_id = $1 and state in ('pending', 'dispatched')",
    )
    .bind(&row.instance_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect remaining compensations: {error}"))
    })?;
    let workflow_state = if remaining == 0 {
        let (stored_state, workflow_state) = if row.terminal_intent.as_deref() == Some("cancelled")
        {
            ("cancelled", WorkflowInstanceState::Cancelled)
        } else {
            ("compensated", WorkflowInstanceState::Compensated)
        };
        sqlx::query(
            r#"
            update platform.service_workflow_instances
            set state = $2,
                terminal_evidence = case
                    when $2 = 'cancelled'
                    then jsonb_set(terminal_evidence, '{cleanupReported}', 'true'::jsonb)
                    else terminal_evidence
                end,
                updated_at = $3
            where instance_id = $1 and state = 'compensating'
            "#,
        )
        .bind(&row.instance_id)
        .bind(stored_state)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not finish compensation: {error}"))
        })?;
        workflow_state
    } else {
        WorkflowInstanceState::Compensating
    };
    insert_history(
        transaction,
        &format!(
            "{}:history:attempt:{}:succeeded",
            compensation_id, row.attempt_count
        ),
        &row.instance_id,
        Some(&row.step_id),
        Some(compensation_id),
        "compensation_attempt_succeeded",
        json!({
            "attemptNumber": row.attempt_count,
            "effectId": row.effect_id,
            "requestEventId": request_event_id,
            "completionEventId": envelope.event_id,
            "transitionId": transition_id,
        }),
        now,
    )
    .await?;
    if matches!(
        workflow_state,
        WorkflowInstanceState::Compensated | WorkflowInstanceState::Cancelled
    ) {
        let (history_kind, final_outcome) = if workflow_state == WorkflowInstanceState::Cancelled {
            ("workflow_cancelled", "cancelled")
        } else {
            ("workflow_compensated", "compensated")
        };
        insert_history(
            transaction,
            &format!("{}:history:{final_outcome}", row.instance_id),
            &row.instance_id,
            None,
            None,
            history_kind,
            json!({"finalOutcome": final_outcome}),
            now,
        )
        .await?;
    }
    insert_story_segment(
        state,
        transaction,
        &row.instance_id,
        Some(&row.step_id),
        Some(compensation_id),
        None,
        &format!(
            "workflow:{compensation_id}:attempt:{}:succeeded",
            row.attempt_count
        ),
        &format!("workflow compensation {compensation_id}"),
        &row.completion_contract_id,
        &row.completion_contract_version,
        "compensated",
        u32::try_from(row.attempt_count).unwrap_or(u32::MAX),
        Some(transition_id.as_str()),
        now,
    )
    .await?;
    Ok(WorkflowCompensationResult {
        disposition: WorkflowTransitionDisposition::Applied,
        instance_id: row.instance_id,
        compensation_id: row.compensation_id,
        effect_id: row.effect_id,
        transition_id,
        outgoing_event_id: envelope.event_id.clone(),
        workflow_state,
    })
}

/// Records a compensation failure as a distinct durable terminal state. It is
/// intentionally not collapsed into ordinary Workflow failure or termination.
pub async fn record_workflow_compensation_failure_at(
    state: &ServiceRuntimeState,
    compensation_id: &str,
    transition_id: &str,
    failure: WorkflowFailureEvidence,
    now: DateTime<Utc>,
) -> Result<WorkflowCompensationFailureResult, WorkflowMutationError> {
    if compensation_id.trim().is_empty()
        || transition_id.trim().is_empty()
        || failure.code.trim().is_empty()
        || failure.message.trim().is_empty()
        || failure.next_action.trim().is_empty()
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Compensation failure identity and evidence must not be empty",
        ));
    }
    let now = postgres_precision(now);
    let pool = state
        .store()
        .map_err(|error| WorkflowMutationError::store(error.public_message))?;
    let mut transaction = pool.begin().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not begin compensation failure: {error}"))
    })?;
    let row = lock_compensation(state, &mut transaction, compensation_id).await?;
    if row.state == "failed" {
        let stored = decode_failure(row.failure_evidence)?.ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                "Failed compensation is missing failure evidence",
            )
        })?;
        if row.transition_id.as_deref() == Some(transition_id) && stored == failure {
            transaction.commit().await.map_err(|error| {
                WorkflowMutationError::store(format!(
                    "Could not commit duplicate compensation failure: {error}"
                ))
            })?;
            return Ok(WorkflowCompensationFailureResult {
                disposition: WorkflowTransitionDisposition::Duplicate,
                instance_id: row.instance_id,
                compensation_id: row.compensation_id,
                effect_id: row.effect_id,
                transition_id: transition_id.to_owned(),
                failure: stored,
                workflow_state: WorkflowInstanceState::CompensationFailed,
            });
        }
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` already failed through another transition"),
        ));
    }
    if !matches!(row.state.as_str(), "pending" | "dispatched")
        || row.instance_state != "compensating"
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Compensation `{compensation_id}` is not pending or dispatched in a compensating Workflow"
            ),
        ));
    }
    let was_dispatched = row.state == "dispatched";
    if was_dispatched && row.transition_id.as_deref() != Some(transition_id) {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Compensation `{compensation_id}` was dispatched through another transition"),
        ));
    }
    ensure_compensation_order(&mut transaction, &row).await?;
    let failure_json = serde_json::to_value(&failure).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode compensation failure: {error}"),
        )
    })?;
    let attempt_number = if was_dispatched {
        row.attempt_count
    } else {
        row.attempt_count + 1
    };
    if was_dispatched {
        let updated = sqlx::query(
            r#"
            update platform.service_workflow_compensation_attempts
            set state = 'failed', failure_evidence = $3, completed_at = $4
            where compensation_id = $1 and transition_id = $2 and state = 'dispatched'
            "#,
        )
        .bind(compensation_id)
        .bind(transition_id)
        .bind(&failure_json)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not fail dispatched compensation attempt: {error}"
            ))
        })?;
        if updated.rows_affected() != 1 {
            return Err(WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!(
                    "Dispatched compensation `{compensation_id}` is missing its active attempt"
                ),
            ));
        }
    } else {
        sqlx::query(
            r#"
            insert into platform.service_workflow_compensation_attempts (
                attempt_id, compensation_id, instance_id, attempt_number,
                transition_id, state, failure_evidence, started_at, completed_at
            ) values ($1, $2, $3, $4, $5, 'failed', $6, $7, $7)
            "#,
        )
        .bind(format!("workflow_compensation_attempt_{}", Uuid::now_v7()))
        .bind(compensation_id)
        .bind(&row.instance_id)
        .bind(attempt_number)
        .bind(transition_id)
        .bind(&failure_json)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not persist failed compensation attempt: {error}"
            ))
        })?;
    }
    sqlx::query(
        r#"
        update platform.service_workflow_compensations
        set state = 'failed', attempt_count = $2, transition_id = $3,
            failure_evidence = $4, completed_at = $5, updated_at = $5
        where compensation_id = $1 and state in ('pending', 'dispatched')
        "#,
    )
    .bind(compensation_id)
    .bind(attempt_number)
    .bind(transition_id)
    .bind(&failure_json)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail compensation: {error}"))
    })?;
    sqlx::query(
        "update platform.service_workflow_effects set state = 'compensation_failed', updated_at = $2 where effect_id = $1",
    )
    .bind(&row.effect_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail compensated effect: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = 'compensation_failed', failure_evidence = $2,
            terminal_transition_id = $3, updated_at = $4
        where instance_id = $1 and state = 'compensating'
        "#,
    )
    .bind(&row.instance_id)
    .bind(&failure_json)
    .bind(transition_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist compensation failure: {error}"))
    })?;
    insert_history(
        &mut transaction,
        &format!("{compensation_id}:history:attempt:{attempt_number}:failed"),
        &row.instance_id,
        Some(&row.step_id),
        Some(compensation_id),
        "compensation_attempt_failed",
        json!({
            "attemptNumber": attempt_number,
            "effectId": row.effect_id,
            "transitionId": transition_id,
            "failure": failure,
            "interventionRequired": true,
        }),
        now,
    )
    .await?;
    insert_history(
        &mut transaction,
        &format!("{}:history:compensation-failed", row.instance_id),
        &row.instance_id,
        None,
        Some(compensation_id),
        "workflow_compensation_failed",
        json!({
            "finalOutcome": "compensation_failed",
            "failure": failure,
            "interventionRequired": true,
        }),
        now,
    )
    .await?;
    insert_story_segment(
        state,
        &mut transaction,
        &row.instance_id,
        Some(&row.step_id),
        Some(compensation_id),
        Some(&format!("workflow:{compensation_id}:intervention")),
        &format!("workflow:{compensation_id}:attempt:{attempt_number}:failed"),
        &format!(
            "workflow compensation {compensation_id} failed {}: {}",
            failure.code, failure.message
        ),
        &row.contract_id,
        &row.contract_version,
        "intervention_required",
        u32::try_from(attempt_number).unwrap_or(u32::MAX),
        Some(transition_id),
        now,
    )
    .await?;
    transaction.commit().await.map_err(|error| {
        WorkflowMutationError::store(format!("Could not commit compensation failure: {error}"))
    })?;
    Ok(WorkflowCompensationFailureResult {
        disposition: WorkflowTransitionDisposition::Applied,
        instance_id: row.instance_id,
        compensation_id: row.compensation_id,
        effect_id: row.effect_id,
        transition_id: transition_id.to_owned(),
        failure,
        workflow_state: WorkflowInstanceState::CompensationFailed,
    })
}

pub(crate) async fn load_compensation_evidence(
    pool: &PgPool,
    instance_id: &str,
) -> Result<WorkflowCompensationEvidence, WorkflowApiError> {
    let effects = load_effect_rows(pool, instance_id).await?;
    let compensations = load_compensation_rows(pool, instance_id).await?;
    let attempts = load_compensation_attempt_rows(pool, instance_id).await?;
    let history = load_history_rows(pool, instance_id).await?;
    evidence_from_rows(effects, compensations, attempts, history)
}

pub(crate) async fn load_compensation_evidence_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
) -> Result<WorkflowCompensationEvidence, WorkflowMutationError> {
    let effects = sqlx::query_as::<_, EffectRow>(effect_select())
        .bind(instance_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not inspect workflow effects: {error}"))
        })?;
    let compensations = sqlx::query_as::<_, CompensationRow>(compensation_select())
        .bind(instance_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not inspect workflow compensations: {error}"
            ))
        })?;
    let attempts = sqlx::query_as::<_, CompensationAttemptRow>(compensation_attempt_select())
        .bind(instance_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not inspect compensation attempts: {error}"
            ))
        })?;
    let history = sqlx::query_as::<_, HistoryRow>(history_select())
        .bind(instance_id)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not inspect workflow history: {error}"))
        })?;
    evidence_from_rows(effects, compensations, attempts, history)
        .map_err(|error| WorkflowMutationError::new(error.code, error.message))
}

async fn load_effect_rows(
    pool: &PgPool,
    instance_id: &str,
) -> Result<Vec<EffectRow>, WorkflowApiError> {
    sqlx::query_as::<_, EffectRow>(effect_select())
        .bind(instance_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            WorkflowApiError::store(format!("Could not inspect workflow effects: {error}"))
        })
}

async fn load_compensation_rows(
    pool: &PgPool,
    instance_id: &str,
) -> Result<Vec<CompensationRow>, WorkflowApiError> {
    sqlx::query_as::<_, CompensationRow>(compensation_select())
        .bind(instance_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            WorkflowApiError::store(format!("Could not inspect workflow compensations: {error}"))
        })
}

async fn load_compensation_attempt_rows(
    pool: &PgPool,
    instance_id: &str,
) -> Result<Vec<CompensationAttemptRow>, WorkflowApiError> {
    sqlx::query_as::<_, CompensationAttemptRow>(compensation_attempt_select())
        .bind(instance_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            WorkflowApiError::store(format!("Could not inspect compensation attempts: {error}"))
        })
}

async fn load_history_rows(
    pool: &PgPool,
    instance_id: &str,
) -> Result<Vec<HistoryRow>, WorkflowApiError> {
    sqlx::query_as::<_, HistoryRow>(history_select())
        .bind(instance_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            WorkflowApiError::store(format!("Could not inspect workflow history: {error}"))
        })
}

fn evidence_from_rows(
    effects: Vec<EffectRow>,
    compensations: Vec<CompensationRow>,
    attempts: Vec<CompensationAttemptRow>,
    history: Vec<HistoryRow>,
) -> Result<WorkflowCompensationEvidence, WorkflowApiError> {
    let effects = effects
        .into_iter()
        .map(effect_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let mut attempts_by_compensation: HashMap<String, Vec<_>> = HashMap::new();
    for row in attempts {
        attempts_by_compensation
            .entry(row.compensation_id.clone())
            .or_default()
            .push(compensation_attempt_from_row(row)?);
    }
    let compensations = compensations
        .into_iter()
        .map(|row| {
            let attempts = attempts_by_compensation
                .remove(&row.compensation_id)
                .unwrap_or_default();
            compensation_from_row(row, attempts)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !attempts_by_compensation.is_empty() {
        return Err(WorkflowApiError::stored_state(
            "Workflow contains compensation attempts without a compensation",
        ));
    }
    Ok(WorkflowCompensationEvidence {
        effects,
        compensations,
        history: history
            .into_iter()
            .map(|row| WorkflowHistoryEntry {
                history_id: row.history_id,
                step_id: row.step_id,
                compensation_id: row.compensation_id,
                kind: row.kind,
                detail: row.detail,
                recorded_at: row.recorded_at,
            })
            .collect(),
    })
}

fn effect_from_row(row: EffectRow) -> Result<WorkflowEffectInspection, WorkflowApiError> {
    Ok(WorkflowEffectInspection {
        effect_id: row.effect_id,
        step_id: row.step_id,
        definition_step_name: row.definition_step_name,
        transition_id: row.effect_transition_id,
        outgoing_work: serde_json::from_value(row.effect_outgoing_work).map_err(|error| {
            WorkflowApiError::stored_state(format!("Stored workflow effect is invalid: {error}"))
        })?,
        compensation_name: row.compensation_name,
        compensation_order: u32::try_from(row.compensation_order).map_err(|_| {
            WorkflowApiError::stored_state("Stored workflow compensation order is invalid")
        })?,
        compensation_contract: WorkflowDataContract::new(
            row.compensation_contract_id,
            row.compensation_contract_version,
        ),
        compensation_completion_contract: WorkflowDataContract::new(
            row.compensation_completion_contract_id,
            row.compensation_completion_contract_version,
        ),
        state: parse_effect_state(&row.state)?,
        completed_at: row.completed_at,
        updated_at: row.updated_at,
    })
}

fn compensation_from_row(
    row: CompensationRow,
    attempts: Vec<WorkflowCompensationAttemptInspection>,
) -> Result<WorkflowCompensationInspection, WorkflowApiError> {
    Ok(WorkflowCompensationInspection {
        compensation_id: row.compensation_id,
        effect_id: row.effect_id,
        step_id: row.step_id,
        name: row.name,
        execution_order: u32::try_from(row.execution_order).map_err(|_| {
            WorkflowApiError::stored_state("Stored Workflow compensation order is invalid")
        })?,
        contract: WorkflowDataContract::new(row.contract_id, row.contract_version),
        completion_contract: WorkflowDataContract::new(
            row.completion_contract_id,
            row.completion_contract_version,
        ),
        state: parse_compensation_state(&row.state)?,
        attempt_count: u32::try_from(row.attempt_count).map_err(|_| {
            WorkflowApiError::stored_state("Stored compensation attempt count is invalid")
        })?,
        transition_id: row.transition_id,
        outgoing_work: row
            .outgoing_work
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| {
                WorkflowApiError::stored_state(format!(
                    "Stored compensation outgoing work is invalid: {error}"
                ))
            })?,
        failure: row
            .failure_evidence
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| {
                WorkflowApiError::stored_state(format!(
                    "Stored compensation failure is invalid: {error}"
                ))
            })?,
        selection_kind: match row.selection_kind.as_str() {
            "timeout" => WorkflowCompensationSelectionKind::Timeout,
            "cancel" => WorkflowCompensationSelectionKind::Cancel,
            other => {
                return Err(WorkflowApiError::stored_state(format!(
                    "Stored compensation selection kind `{other}` is invalid"
                )));
            }
        },
        selected_by_transition_id: row.selected_by_timeout_transition_id.clone(),
        selected_by_timeout_transition_id: row.selected_by_timeout_transition_id,
        attempts,
        selected_at: row.selected_at,
        completed_at: row.completed_at,
        updated_at: row.updated_at,
    })
}

fn compensation_attempt_from_row(
    row: CompensationAttemptRow,
) -> Result<WorkflowCompensationAttemptInspection, WorkflowApiError> {
    let state = match row.state.as_str() {
        "dispatched" => WorkflowCompensationAttemptState::Dispatched,
        "succeeded" => WorkflowCompensationAttemptState::Succeeded,
        "failed" => WorkflowCompensationAttemptState::Failed,
        other => {
            return Err(WorkflowApiError::stored_state(format!(
                "Stored compensation attempt state `{other}` is invalid"
            )));
        }
    };
    Ok(WorkflowCompensationAttemptInspection {
        attempt_id: row.attempt_id,
        attempt_number: u32::try_from(row.attempt_number).map_err(|_| {
            WorkflowApiError::stored_state("Stored compensation attempt number is invalid")
        })?,
        transition_id: row.transition_id,
        state,
        failure: row
            .failure_evidence
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| {
                WorkflowApiError::stored_state(format!(
                    "Stored compensation attempt failure is invalid: {error}"
                ))
            })?,
        started_at: row.started_at,
        completed_at: row.completed_at,
    })
}

fn parse_effect_state(value: &str) -> Result<WorkflowEffectState, WorkflowApiError> {
    match value {
        "completed" => Ok(WorkflowEffectState::Completed),
        "compensated" => Ok(WorkflowEffectState::Compensated),
        "compensation_failed" => Ok(WorkflowEffectState::CompensationFailed),
        other => Err(WorkflowApiError::stored_state(format!(
            "Stored workflow effect state `{other}` is invalid"
        ))),
    }
}

fn parse_compensation_state(value: &str) -> Result<WorkflowCompensationState, WorkflowApiError> {
    match value {
        "pending" => Ok(WorkflowCompensationState::Pending),
        "dispatched" => Ok(WorkflowCompensationState::Dispatched),
        "compensated" => Ok(WorkflowCompensationState::Compensated),
        "failed" => Ok(WorkflowCompensationState::Failed),
        other => Err(WorkflowApiError::stored_state(format!(
            "Stored Workflow compensation state `{other}` is invalid"
        ))),
    }
}

fn effect_select() -> &'static str {
    r#"
    select effect_id, step_id, definition_step_name, effect_transition_id,
           effect_outgoing_work, compensation_name, compensation_order,
           compensation_contract_id, compensation_contract_version,
           compensation_completion_contract_id,
           compensation_completion_contract_version,
           state, completed_at, updated_at
    from platform.service_workflow_effects
    where instance_id = $1
    order by compensation_order, effect_id
    "#
}

fn compensation_select() -> &'static str {
    r#"
    select compensation_id, effect_id, step_id, name, execution_order,
           contract_id, contract_version, state, attempt_count, transition_id,
           completion_contract_id, completion_contract_version,
           outgoing_work, failure_evidence, selection_kind,
           selected_by_timeout_transition_id,
           selected_at, completed_at, updated_at
    from platform.service_workflow_compensations
    where instance_id = $1
    order by execution_order, compensation_id
    "#
}

fn compensation_attempt_select() -> &'static str {
    r#"
    select compensation_id, attempt_id, attempt_number, transition_id, state,
           failure_evidence, started_at, completed_at
    from platform.service_workflow_compensation_attempts
    where instance_id = $1
    order by compensation_id, attempt_number
    "#
}

fn history_select() -> &'static str {
    r#"
    select history_id, step_id, compensation_id, kind, detail, recorded_at
    from platform.service_workflow_history
    where instance_id = $1
    order by recorded_at, history_id
    "#
}

async fn persist_timeout_failure(
    transaction: &mut Transaction<'_, Postgres>,
    claim: &WorkflowWorkClaim,
    now: DateTime<Utc>,
) -> Result<(), WorkflowMutationError> {
    sqlx::query(
        r#"
        insert into platform.service_workflow_step_attempts (
            attempt_id, instance_id, step_id, attempt_number, transition_id,
            state, failure_classification, failure_code, failure_message,
            scheduled_at, started_at, completed_at
        ) values ($1, $2, $3, $4, $5, 'failed', 'timeout',
                  'step_timeout', 'Workflow step exceeded its timeout',
                  $6, $7, $7)
        on conflict (step_id, attempt_number) do update
        set state = 'failed', failure_classification = 'timeout',
            failure_code = 'step_timeout',
            failure_message = 'Workflow step exceeded its timeout',
            completed_at = excluded.completed_at
        where platform.service_workflow_step_attempts.transition_id = excluded.transition_id
          and platform.service_workflow_step_attempts.state = 'running'
        "#,
    )
    .bind(format!("workflow_attempt_{}", Uuid::now_v7()))
    .bind(&claim.instance_id)
    .bind(&claim.step_id)
    .bind(i32::try_from(claim.attempt_number).unwrap_or(i32::MAX))
    .bind(&claim.attempt_transition_id)
    .bind(claim.due_at)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist compensation timeout: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_timers
        set state = case when timer_id = $2 then 'completed' else 'cancelled' end,
            completed_at = $3, updated_at = $3
        where step_id = $1 and state in ('pending', 'claimed')
        "#,
    )
    .bind(&claim.step_id)
    .bind(&claim.timer_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not resolve compensation timeout: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'failed', transition_id = $3, completed_at = $4,
            attempt_count = $5, next_attempt_at = null,
            failure_classification = 'timeout', failure_code = 'step_timeout',
            failure_message = 'Workflow step exceeded its timeout', updated_at = $4
        where instance_id = $1 and step_id = $2 and state = 'pending'
        "#,
    )
    .bind(&claim.instance_id)
    .bind(&claim.step_id)
    .bind(&claim.transition_id)
    .bind(now)
    .bind(i32::try_from(claim.attempt_number).unwrap_or(i32::MAX))
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not time out Workflow step: {error}"))
    })?;
    Ok(())
}

async fn load_selection_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    timeout_transition_id: &str,
) -> Result<Vec<WorkflowCompensationSelection>, WorkflowMutationError> {
    let rows: Vec<(String, String, String, String, i32)> = sqlx::query_as(
        r#"
        select compensation_id, effect_id, step_id, name, execution_order
        from platform.service_workflow_compensations
        where instance_id = $1 and selected_by_timeout_transition_id = $2
        order by execution_order, compensation_id
        "#,
    )
    .bind(instance_id)
    .bind(timeout_transition_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect compensation selection: {error}"))
    })?;
    rows.into_iter()
        .map(
            |(compensation_id, effect_id, step_id, name, execution_order)| {
                Ok(WorkflowCompensationSelection {
                    compensation_id,
                    effect_id,
                    step_id,
                    name,
                    execution_order: u32::try_from(execution_order).map_err(|_| {
                        WorkflowMutationError::new(
                            WorkflowErrorCode::StoredStateInvalid,
                            "Stored compensation order is invalid",
                        )
                    })?,
                })
            },
        )
        .collect()
}

async fn lock_compensation(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    compensation_id: &str,
) -> Result<CompensationExecutionRow, WorkflowMutationError> {
    sqlx::query_as::<_, CompensationExecutionRow>(
        r#"
        select compensation.compensation_id, compensation.effect_id,
               compensation.instance_id, compensation.step_id, compensation.name,
               compensation.execution_order, compensation.contract_id,
               compensation.contract_version, compensation.completion_contract_id,
               compensation.completion_contract_version, compensation.state,
               compensation.attempt_count, compensation.transition_id,
               compensation.outgoing_work, compensation.failure_evidence,
               instance.state as instance_state, instance.control_state,
               instance.terminal_intent, instance.workflow_context
        from platform.service_workflow_compensations compensation
        join platform.service_workflow_instances instance
          on instance.instance_id = compensation.instance_id
        where instance.service_id = $1 and compensation.compensation_id = $2
        for update of instance, compensation
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(compensation_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock Workflow compensation: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StepNotFound,
            format!("Workflow compensation `{compensation_id}` was not found"),
        )
    })
}

async fn ensure_compensation_order(
    transaction: &mut Transaction<'_, Postgres>,
    row: &CompensationExecutionRow,
) -> Result<(), WorkflowMutationError> {
    let blocked: bool = sqlx::query_scalar(
        r#"
        select exists (
            select 1 from platform.service_workflow_compensations
            where instance_id = $1 and execution_order < $2 and state <> 'compensated'
        )
        "#,
    )
    .bind(&row.instance_id)
    .bind(row.execution_order)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect compensation order: {error}"))
    })?;
    if blocked {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Compensation `{}` is blocked by an earlier declared compensation",
                row.compensation_id
            ),
        ));
    }
    Ok(())
}

fn compensation_envelope(
    state: &ServiceRuntimeState,
    row: &CompensationExecutionRow,
    publication: &WorkflowEventPublication,
) -> Result<EventEnvelope, WorkflowMutationError> {
    let contract = state
        .event_contracts
        .iter()
        .find(|contract| {
            contract.contract_id == publication.contract_id
                && contract.version == publication.contract_version
        })
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::EventContractNotDeclared,
                format!(
                    "Outgoing compensation Event Contract `{}` version `{}` is not declared by Service `{}`",
                    publication.contract_id,
                    publication.contract_version,
                    state.identity.service_id
                ),
            )
        })?;
    let event_type = event_type_for_contract(contract)?;
    let mut context: EventContext = row
        .workflow_context
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored compensation execution context is invalid: {error}"),
            )
        })?
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::ContextRequired,
                "Cross-Service compensation requires persisted Event Context",
            )
        })?;
    context.service_principal = Some(publication.service_principal.clone());
    context.causation = Some(CausationContext {
        causation_id: row.compensation_id.clone(),
        correlation_id: context
            .causation
            .as_ref()
            .and_then(|causation| causation.correlation_id.clone()),
    });
    validate_outgoing_context(contract, &context)?;
    let mut data = publication.data.clone();
    let object = data.as_object_mut().ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Compensation Event data must be a JSON object",
        )
    })?;
    object.insert(
        "workflowInstanceId".to_owned(),
        Value::String(row.instance_id.clone()),
    );
    object.insert(
        "compensationId".to_owned(),
        Value::String(row.compensation_id.clone()),
    );
    object.insert("effectId".to_owned(), Value::String(row.effect_id.clone()));
    object.insert("action".to_owned(), Value::String(row.name.clone()));
    Ok(EventEnvelope {
        protocol: lenso_service::EVENT_ENVELOPE_PROTOCOL.to_owned(),
        event_id: publication.event_id.clone(),
        event_type,
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version.clone(),
        producer_service_id: state.identity.service_id.clone(),
        module_id: contract.module_id.clone(),
        occurred_at: publication.occurred_at.clone(),
        tenancy_mode: contract.tenancy_mode.clone(),
        context,
        content: EventContent {
            content_type: "application/json".to_owned(),
            schema: contract.artifact.path.clone(),
            data,
        },
    })
}

fn required_content_identity<'a>(
    envelope: &'a EventEnvelope,
    field: &str,
) -> Result<&'a str, WorkflowMutationError> {
    envelope.content.data[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::InvalidRequest,
                format!("Compensation Event field `{field}` must be a non-empty string"),
            )
        })
}

fn validate_completion_identity(
    row: &CompensationExecutionRow,
    envelope: &EventEnvelope,
) -> Result<(), WorkflowMutationError> {
    let effect_id = required_content_identity(envelope, "effectId")?;
    let action = required_content_identity(envelope, "action")?;
    let instance_id = required_content_identity(envelope, "workflowInstanceId")?;
    if effect_id != row.effect_id || action != row.name || instance_id != row.instance_id {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Compensation completion identity does not match the persisted compensation",
        ));
    }
    Ok(())
}

async fn current_compensation_workflow_state(
    transaction: &mut Transaction<'_, Postgres>,
    compensation_id: &str,
) -> Result<WorkflowInstanceState, WorkflowMutationError> {
    let state: String = sqlx::query_scalar(
        r#"
        select instance.state
        from platform.service_workflow_instances instance
        join platform.service_workflow_compensations compensation
          on compensation.instance_id = instance.instance_id
        where compensation.compensation_id = $1
        "#,
    )
    .bind(compensation_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect compensated Workflow: {error}"))
    })?;
    parse_instance_state(&state)
}

fn parse_instance_state(value: &str) -> Result<WorkflowInstanceState, WorkflowMutationError> {
    WorkflowInstanceState::parse(value).ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Stored compensation Workflow state `{value}` is invalid"),
        )
    })
}

fn decode_outgoing_work(
    value: Option<Value>,
) -> Result<Option<WorkflowOutgoingWorkInspection>, WorkflowMutationError> {
    value
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored compensation outgoing work is invalid: {error}"),
            )
        })
}

fn decode_failure(
    value: Option<Value>,
) -> Result<Option<WorkflowFailureEvidence>, WorkflowMutationError> {
    value
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored compensation failure is invalid: {error}"),
            )
        })
}

#[allow(clippy::too_many_arguments)]
async fn insert_history(
    transaction: &mut Transaction<'_, Postgres>,
    history_id: &str,
    instance_id: &str,
    step_id: Option<&str>,
    compensation_id: Option<&str>,
    kind: &str,
    detail: Value,
    recorded_at: DateTime<Utc>,
) -> Result<(), WorkflowMutationError> {
    sqlx::query(
        r#"
        insert into platform.service_workflow_history (
            history_id, instance_id, step_id, compensation_id,
            kind, detail, recorded_at
        ) values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (history_id) do nothing
        "#,
    )
    .bind(history_id)
    .bind(instance_id)
    .bind(step_id)
    .bind(compensation_id)
    .bind(kind)
    .bind(detail)
    .bind(recorded_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist Workflow history: {error}"))
    })?;
    Ok(())
}

async fn insert_story_segment(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: Option<&str>,
    compensation_id: Option<&str>,
    intervention_id: Option<&str>,
    segment_id: &str,
    operation: &str,
    contract_id: &str,
    contract_version: &str,
    status: &str,
    attempt: u32,
    causation_id: Option<&str>,
    recorded_at: DateTime<Utc>,
) -> Result<(), WorkflowMutationError> {
    append_persisted_workflow_story_segment_in_tx(
        state,
        transaction,
        instance_id,
        step_id,
        compensation_id,
        intervention_id,
        segment_id,
        operation,
        contract_id,
        contract_version,
        status,
        attempt,
        causation_id,
        recorded_at,
    )
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!(
            "Could not persist compensation Story Segment: {error}"
        ))
    })?;
    Ok(())
}

fn postgres_precision(value: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_micros(value.timestamp_micros())
        .expect("Workflow timestamp must fit PostgreSQL precision")
}
