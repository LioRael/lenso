use crate::{
    ServiceRuntimeState, WorkflowErrorCode, WorkflowFailureEvidence, WorkflowMutationError,
    WorkflowTenantScope, WorkflowTransitionDisposition, encode_pinned_definition, postgres_now,
    resolve_definition, resolve_pinned_definition,
};
use chrono::{DateTime, Utc};
use lenso_service::{CausationContext, EventContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowChildStartRequest {
    pub start_id: String,
    pub definition_owner: String,
    pub definition_name: String,
    pub definition_version: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowChildStartResult {
    pub disposition: WorkflowTransitionDisposition,
    pub link_id: String,
    pub parent_instance_id: String,
    pub parent_step_id: String,
    pub child_instance_id: Option<String>,
    pub failure: Option<WorkflowFailureEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowChildResumeResult {
    pub disposition: WorkflowTransitionDisposition,
    pub link_id: String,
    pub parent_instance_id: String,
    pub parent_step_id: String,
    pub child_instance_id: String,
    pub completion_delivery_id: String,
    pub next_step_id: Option<String>,
    pub failure: Option<WorkflowFailureEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFailureTransitionResult {
    pub disposition: WorkflowTransitionDisposition,
    pub instance_id: String,
    pub transition_id: String,
    pub failure: WorkflowFailureEvidence,
}

#[derive(Debug, FromRow)]
struct ParentStepRow {
    definition_version: String,
    instance_state: String,
    control_state: String,
    story_context: Value,
    tenant_scope: Option<Value>,
    workflow_context: Option<Value>,
    step_state: String,
}

#[derive(Debug, FromRow)]
struct ChildLinkRow {
    link_id: String,
    start_id: String,
    child_definition_owner: String,
    child_definition_name: String,
    child_definition_version: String,
    child_instance_id: Option<String>,
    failure_evidence: Option<Value>,
}

#[derive(Debug, FromRow)]
struct FailureRow {
    state: String,
    terminal_transition_id: Option<String>,
    failure_evidence: Option<Value>,
}

#[derive(Debug, FromRow)]
struct ChildResumeRow {
    link_id: String,
    link_state: String,
    completion_delivery_id: Option<String>,
    link_failure_evidence: Option<Value>,
    parent_definition_owner: String,
    parent_definition_name: String,
    parent_definition_version: String,
    parent_definition_artifact: Option<Value>,
    parent_definition_digest: Option<String>,
    parent_state: String,
    parent_control_state: String,
    parent_step_name: String,
    parent_step_position: i32,
    parent_step_state: String,
    child_definition_owner: String,
    child_definition_name: String,
    child_definition_version: String,
    child_definition_artifact: Option<Value>,
    child_definition_digest: Option<String>,
    child_state: String,
    child_failure_evidence: Option<Value>,
}

/// Starts one version-pinned child Workflow in the parent's Service Store.
/// The parent step becomes durably waiting before the caller commits. Reusing
/// `start_id` for the same parent step returns the existing child.
#[allow(clippy::too_many_lines)]
pub async fn start_child_workflow_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    parent_instance_id: &str,
    parent_step_id: &str,
    request: &WorkflowChildStartRequest,
) -> Result<WorkflowChildStartResult, WorkflowMutationError> {
    validate_child_start(parent_instance_id, parent_step_id, request)?;
    let parent = sqlx::query_as::<_, ParentStepRow>(
        r#"
        select instance.definition_version, instance.state as instance_state,
               instance.control_state,
               instance.story_context, instance.tenant_scope, instance.workflow_context,
               step.state as step_state
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.service_id = $1 and instance.instance_id = $2 and step.step_id = $3
        for update of instance, step
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock parent workflow step: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StepNotFound,
            format!(
                "Parent workflow step `{parent_step_id}` was not found in instance `{parent_instance_id}`"
            ),
        )
    })?;

    let existing = sqlx::query_as::<_, ChildLinkRow>(
        r#"
        select link_id, start_id, child_definition_owner, child_definition_name,
               child_definition_version, child_instance_id, failure_evidence
        from platform.service_workflow_child_links
        where parent_instance_id = $1 and parent_step_id = $2
        "#,
    )
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect child workflow link: {error}"))
    })?;
    if let Some(existing) = existing {
        if existing.start_id != request.start_id
            || existing.child_definition_owner != request.definition_owner
            || existing.child_definition_name != request.definition_name
            || existing.child_definition_version != request.definition_version
        {
            return Err(WorkflowMutationError::new(
                WorkflowErrorCode::TransitionConflict,
                format!(
                    "Parent workflow step `{parent_step_id}` already started a different child workflow"
                ),
            ));
        }
        let failure = decode_failure(existing.failure_evidence)?;
        return Ok(WorkflowChildStartResult {
            disposition: WorkflowTransitionDisposition::Duplicate,
            link_id: existing.link_id,
            parent_instance_id: parent_instance_id.to_owned(),
            parent_step_id: parent_step_id.to_owned(),
            child_instance_id: existing.child_instance_id,
            failure,
        });
    }
    if parent.instance_state != "running" || parent.step_state != "pending" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Parent workflow step `{parent_step_id}` is not pending in a running instance"),
        ));
    }
    if parent.control_state != "active" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Parent Workflow Instance `{parent_instance_id}` is paused; resume it before starting child work"
            ),
        ));
    }

    let mut context = decode_inherited_context(&parent, parent_instance_id)?;
    context.causation = Some(CausationContext {
        causation_id: parent_step_id.to_owned(),
        correlation_id: context
            .causation
            .as_ref()
            .and_then(|causation| causation.correlation_id.clone()),
    });
    let now = postgres_now();
    let link_id = format!("workflow_child_{}", Uuid::now_v7());
    let definition = match resolve_definition(
        state,
        &request.definition_owner,
        &request.definition_name,
        &request.definition_version,
    ) {
        Ok(definition) => definition,
        Err(error)
            if matches!(
                error.code,
                WorkflowErrorCode::DefinitionNotFound
                    | WorkflowErrorCode::DefinitionVersionNotFound
            ) =>
        {
            let failure = WorkflowFailureEvidence::new(
                "workflow_child_definition_version_unsupported",
                format!(
                    "Child Workflow Definition `{}/{}` version `{}` is not supported by this worker",
                    request.definition_owner, request.definition_name, request.definition_version
                ),
                "deploy_worker_supporting_child_workflow_version",
            );
            persist_unsupported_child_start(
                transaction,
                &link_id,
                parent_instance_id,
                parent_step_id,
                &parent.definition_version,
                request,
                &failure,
                now,
            )
            .await?;
            return Ok(WorkflowChildStartResult {
                disposition: WorkflowTransitionDisposition::Applied,
                link_id,
                parent_instance_id: parent_instance_id.to_owned(),
                parent_step_id: parent_step_id.to_owned(),
                child_instance_id: None,
                failure: Some(failure),
            });
        }
        Err(error) => return Err(error),
    };

    let child_instance_id = format!("workflow_{}", Uuid::now_v7());
    let child_step_id = format!("workflow_step_{}", Uuid::now_v7());
    let (definition_artifact, definition_digest) = encode_pinned_definition(&definition)?;
    let context_json = serde_json::to_value(&context).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode inherited child workflow context: {error}"),
        )
    })?;
    let trigger_id = format!("{parent_instance_id}:{}", request.start_id);
    sqlx::query(
        r#"
        insert into platform.service_workflow_instances (
            instance_id, service_id, definition_owner, definition_name,
            definition_version, definition_artifact, definition_digest,
            state, input, result, story_context,
            tenant_scope, initial_step_id, start_trigger_kind, start_trigger_id,
            workflow_context, parent_instance_id, parent_step_id, causation_id,
            created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, 'running', $8, null, $9,
                  $10, $11, 'child', $12, $13, $14, $15, $16, $17, $17)
        "#,
    )
    .bind(&child_instance_id)
    .bind(&state.identity.service_id)
    .bind(&definition.owner)
    .bind(&definition.name)
    .bind(&definition.version)
    .bind(definition_artifact)
    .bind(definition_digest)
    .bind(&request.input)
    .bind(&parent.story_context)
    .bind(&parent.tenant_scope)
    .bind(&child_step_id)
    .bind(trigger_id)
    .bind(context_json)
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(parent_step_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist child workflow: {error}"))
    })?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_steps (
            step_id, instance_id, definition_step_name, step_position,
            state, created_at, updated_at
        ) values ($1, $2, $3, 0, 'pending', $4, $4)
        "#,
    )
    .bind(&child_step_id)
    .bind(&child_instance_id)
    .bind(&definition.steps[0].name)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist child workflow step: {error}"))
    })?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_child_links (
            link_id, start_id, parent_instance_id, parent_step_id,
            parent_definition_version, child_definition_owner,
            child_definition_name, child_definition_version, child_instance_id,
            state, next_action, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                  'waiting', 'wait_for_child_workflow', $10, $10)
        "#,
    )
    .bind(&link_id)
    .bind(&request.start_id)
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(&parent.definition_version)
    .bind(&definition.owner)
    .bind(&definition.name)
    .bind(&definition.version)
    .bind(&child_instance_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist child workflow link: {error}"))
    })?;
    let updated = sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'waiting_for_child', updated_at = $3
        where instance_id = $1 and step_id = $2 and state = 'pending'
        "#,
    )
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not wait for child workflow: {error}"))
    })?;
    if updated.rows_affected() != 1 {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Parent workflow step `{parent_step_id}` lost its pending transition"),
        ));
    }
    sqlx::query(
        "update platform.service_workflow_instances set updated_at = $2 where instance_id = $1",
    )
    .bind(parent_instance_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not update parent workflow: {error}"))
    })?;

    Ok(WorkflowChildStartResult {
        disposition: WorkflowTransitionDisposition::Applied,
        link_id,
        parent_instance_id: parent_instance_id.to_owned(),
        parent_step_id: parent_step_id.to_owned(),
        child_instance_id: Some(child_instance_id),
        failure: None,
    })
}

/// Records a terminal Workflow failure with a stable transition identity.
/// Redelivery of the same failure is harmless.
pub async fn fail_workflow_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    transition_id: &str,
    failure: &WorkflowFailureEvidence,
) -> Result<WorkflowFailureTransitionResult, WorkflowMutationError> {
    if instance_id.trim().is_empty()
        || transition_id.trim().is_empty()
        || failure.code.trim().is_empty()
        || failure.message.trim().is_empty()
        || failure.next_action.trim().is_empty()
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Workflow failure identity and evidence must not be empty",
        ));
    }
    let row = sqlx::query_as::<_, FailureRow>(
        r#"
        select state, terminal_transition_id, failure_evidence
        from platform.service_workflow_instances
        where service_id = $1 and instance_id = $2
        for update
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock workflow failure state: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::InstanceNotFound,
            format!("Workflow Instance `{instance_id}` was not found in this Service Store"),
        )
    })?;
    if row.state == "failed" {
        let stored = decode_failure(row.failure_evidence)?.ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                "Failed Workflow Instance is missing failure evidence",
            )
        })?;
        if row.terminal_transition_id.as_deref() == Some(transition_id) && stored == *failure {
            return Ok(WorkflowFailureTransitionResult {
                disposition: WorkflowTransitionDisposition::Duplicate,
                instance_id: instance_id.to_owned(),
                transition_id: transition_id.to_owned(),
                failure: stored,
            });
        }
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Workflow Instance `{instance_id}` already failed through another transition"),
        ));
    }
    if row.state != "running" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Workflow Instance `{instance_id}` is already terminal"),
        ));
    }
    let now = postgres_now();
    let failure_json = encode_failure(failure)?;
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'failed', transition_id = $2, completed_at = $3, updated_at = $3
        where instance_id = $1 and state in ('pending', 'waiting_for_child')
        "#,
    )
    .bind(instance_id)
    .bind(transition_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail workflow step: {error}"))
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
    .bind(failure_json)
    .bind(transition_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist workflow failure: {error}"))
    })?;
    Ok(WorkflowFailureTransitionResult {
        disposition: WorkflowTransitionDisposition::Applied,
        instance_id: instance_id.to_owned(),
        transition_id: transition_id.to_owned(),
        failure: failure.clone(),
    })
}

/// Resumes a parent exactly once after a terminal child completion delivery.
/// The selected parent and child definition versions are read from durable
/// state after restart; unsupported versions become parent-side evidence.
#[allow(clippy::too_many_lines)]
pub async fn resume_parent_from_child_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    parent_instance_id: &str,
    parent_step_id: &str,
    child_instance_id: &str,
    completion_delivery_id: &str,
) -> Result<WorkflowChildResumeResult, WorkflowMutationError> {
    if [
        parent_instance_id,
        parent_step_id,
        child_instance_id,
        completion_delivery_id,
    ]
    .iter()
    .any(|identity| identity.trim().is_empty())
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Parent, child, step, and completion delivery identities must not be empty",
        ));
    }
    let row = sqlx::query_as::<_, ChildResumeRow>(
        r#"
        select link.link_id, link.state as link_state, link.completion_delivery_id,
               link.failure_evidence as link_failure_evidence,
               parent.definition_owner as parent_definition_owner,
               parent.definition_name as parent_definition_name,
               parent.definition_version as parent_definition_version,
               parent.definition_artifact as parent_definition_artifact,
               parent.definition_digest as parent_definition_digest,
               parent.state as parent_state,
               parent.control_state as parent_control_state,
               step.definition_step_name as parent_step_name,
               step.step_position as parent_step_position,
               step.state as parent_step_state,
               child.definition_owner as child_definition_owner,
               child.definition_name as child_definition_name,
               child.definition_version as child_definition_version,
               child.definition_artifact as child_definition_artifact,
               child.definition_digest as child_definition_digest,
               child.state as child_state,
               child.failure_evidence as child_failure_evidence
        from platform.service_workflow_child_links link
        join platform.service_workflow_instances parent
          on parent.instance_id = link.parent_instance_id
        join platform.service_workflow_steps step
          on step.step_id = link.parent_step_id and step.instance_id = parent.instance_id
        join platform.service_workflow_instances child
          on child.instance_id = link.child_instance_id
        where parent.service_id = $1 and parent.instance_id = $2
          and step.step_id = $3 and child.instance_id = $4
        for update of link, parent, step, child
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(child_instance_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not lock child completion state: {error}"))
    })?
    .ok_or_else(|| {
        WorkflowMutationError::new(
            WorkflowErrorCode::ChildLinkNotFound,
            format!(
                "Child Workflow Instance `{child_instance_id}` is not linked to parent step `{parent_step_id}`"
            ),
        )
    })?;

    if row.link_state != "waiting" {
        if row.completion_delivery_id.as_deref() != Some(completion_delivery_id) {
            return Err(WorkflowMutationError::new(
                WorkflowErrorCode::TransitionConflict,
                format!(
                    "Child workflow link `{}` already consumed another completion",
                    row.link_id
                ),
            ));
        }
        let next_step_id =
            next_step_id(transaction, parent_instance_id, row.parent_step_position).await?;
        return Ok(WorkflowChildResumeResult {
            disposition: WorkflowTransitionDisposition::Duplicate,
            link_id: row.link_id,
            parent_instance_id: parent_instance_id.to_owned(),
            parent_step_id: parent_step_id.to_owned(),
            child_instance_id: child_instance_id.to_owned(),
            completion_delivery_id: completion_delivery_id.to_owned(),
            next_step_id,
            failure: decode_failure(row.link_failure_evidence)?,
        });
    }
    if row.parent_state != "running" || row.parent_step_state != "waiting_for_child" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!("Parent workflow step `{parent_step_id}` is not waiting for its child"),
        ));
    }
    if row.parent_control_state != "active" {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TransitionConflict,
            format!(
                "Parent Workflow Instance `{parent_instance_id}` is paused; resume it before consuming child completion"
            ),
        ));
    }

    if let Err(error) = resolve_pinned_definition(
        state,
        &row.child_definition_owner,
        &row.child_definition_name,
        &row.child_definition_version,
        row.child_definition_artifact.as_ref(),
        row.child_definition_digest.as_deref(),
    ) {
        if matches!(
            error.code,
            WorkflowErrorCode::DefinitionNotFound
                | WorkflowErrorCode::DefinitionVersionNotFound
                | WorkflowErrorCode::DefinitionVersionUnsupported
        ) {
            let failure = WorkflowFailureEvidence::new(
                "workflow_child_definition_version_unsupported",
                format!(
                    "Child Workflow Definition `{}/{}` version `{}` is not supported by this worker",
                    row.child_definition_owner,
                    row.child_definition_name,
                    row.child_definition_version
                ),
                "deploy_worker_supporting_child_workflow_version",
            );
            return fail_parent_from_child(
                transaction,
                &row,
                parent_instance_id,
                parent_step_id,
                child_instance_id,
                completion_delivery_id,
                &failure,
                row.child_state == "running",
            )
            .await;
        }
        return Err(error);
    }

    match row.child_state.as_str() {
        "running" => Err(WorkflowMutationError::new(
            WorkflowErrorCode::ChildNotTerminal,
            format!("Child Workflow Instance `{child_instance_id}` is still running"),
        )),
        "failed" => {
            let failure = decode_failure(row.child_failure_evidence.clone())?.ok_or_else(|| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    format!("Failed child Workflow Instance `{child_instance_id}` has no evidence"),
                )
            })?;
            fail_parent_from_child(
                transaction,
                &row,
                parent_instance_id,
                parent_step_id,
                child_instance_id,
                completion_delivery_id,
                &failure,
                false,
            )
            .await
        }
        "completed" => {
            let parent_definition = match resolve_pinned_definition(
                state,
                &row.parent_definition_owner,
                &row.parent_definition_name,
                &row.parent_definition_version,
                row.parent_definition_artifact.as_ref(),
                row.parent_definition_digest.as_deref(),
            ) {
                Ok(definition) => definition,
                Err(error)
                    if matches!(
                        error.code,
                        WorkflowErrorCode::DefinitionNotFound
                            | WorkflowErrorCode::DefinitionVersionNotFound
                            | WorkflowErrorCode::DefinitionVersionUnsupported
                    ) =>
                {
                    let failure = WorkflowFailureEvidence::new(
                        "workflow_parent_definition_version_unsupported",
                        format!(
                            "Parent Workflow Definition `{}/{}` version `{}` is not supported by this worker",
                            row.parent_definition_owner,
                            row.parent_definition_name,
                            row.parent_definition_version
                        ),
                        "deploy_worker_supporting_parent_workflow_version",
                    );
                    return fail_parent_from_child(
                        transaction,
                        &row,
                        parent_instance_id,
                        parent_step_id,
                        child_instance_id,
                        completion_delivery_id,
                        &failure,
                        false,
                    )
                    .await;
                }
                Err(error) => return Err(error),
            };
            let position = usize::try_from(row.parent_step_position).map_err(|_| {
                WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    format!("Parent workflow step `{parent_step_id}` has an invalid position"),
                )
            })?;
            if parent_definition
                .steps
                .get(position)
                .map(|step| step.name.as_str())
                != Some(row.parent_step_name.as_str())
            {
                return Err(WorkflowMutationError::new(
                    WorkflowErrorCode::StoredStateInvalid,
                    format!(
                        "Parent workflow step `{parent_step_id}` does not match its pinned definition"
                    ),
                ));
            }
            complete_parent_from_child(
                transaction,
                &row,
                &parent_definition,
                parent_instance_id,
                parent_step_id,
                child_instance_id,
                completion_delivery_id,
                position,
            )
            .await
        }
        other => Err(WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!(
                "Child Workflow Instance `{child_instance_id}` has unsupported state `{other}`"
            ),
        )),
    }
}

fn validate_child_start(
    parent_instance_id: &str,
    parent_step_id: &str,
    request: &WorkflowChildStartRequest,
) -> Result<(), WorkflowMutationError> {
    if [
        parent_instance_id,
        parent_step_id,
        &request.start_id,
        &request.definition_owner,
        &request.definition_name,
        &request.definition_version,
    ]
    .iter()
    .any(|identity| identity.trim().is_empty())
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::InvalidRequest,
            "Parent and child workflow identities must not be empty",
        ));
    }
    Ok(())
}

fn decode_inherited_context(
    parent: &ParentStepRow,
    parent_instance_id: &str,
) -> Result<EventContext, WorkflowMutationError> {
    let context: EventContext = parent
        .workflow_context
        .clone()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored parent workflow context is invalid: {error}"),
            )
        })?
        .ok_or_else(|| {
            WorkflowMutationError::new(
                WorkflowErrorCode::ContextRequired,
                "Child workflows require persisted parent execution context",
            )
        })?;
    if context.protocol != lenso_service::COMMON_CONTEXT_PROTOCOL
        || context.story.is_none()
        || context.delegated_actor.is_none()
        || context.tenant.is_none()
        || context.deadline.is_none()
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::ContextRequired,
            "Child workflows require valid Story, Delegated Actor, Tenant, and Deadline context",
        ));
    }
    let delegated_actor = context.delegated_actor.as_ref().expect("validated above");
    let tenant = context.tenant.as_ref().expect("validated above");
    let deadline = context.deadline.as_ref().expect("validated above");
    let invalid_idempotency = context
        .idempotency_key
        .as_ref()
        .is_some_and(|key| key.value.trim().is_empty() || key.scope.trim().is_empty());
    if delegated_actor.issuer.trim().is_empty()
        || delegated_actor.subject.trim().is_empty()
        || delegated_actor.intent.trim().is_empty()
        || delegated_actor.delegation_id.trim().is_empty()
        || delegated_actor.permissions.is_empty()
        || delegated_actor
            .permissions
            .iter()
            .any(|permission| permission.trim().is_empty())
        || tenant.issuer.trim().is_empty()
        || tenant.tenant_id.trim().is_empty()
        || tenant.actor_subject.trim().is_empty()
        || tenant.delegation_id.trim().is_empty()
        || tenant.claim_id.trim().is_empty()
        || deadline.expires_at_unix_ms == 0
        || invalid_idempotency
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::ContextRequired,
            "Child workflow inherited context failed semantic validation",
        ));
    }
    let story: crate::WorkflowStoryContext = serde_json::from_value(parent.story_context.clone())
        .map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Stored parent Story Context is invalid: {error}"),
        )
    })?;
    let inherited_story = context.story.as_ref().expect("validated above");
    if inherited_story.story_id != story.story_id || inherited_story.segment_id != story.segment_id
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!(
                "Parent Workflow Instance `{parent_instance_id}` has inconsistent Story Context"
            ),
        ));
    }
    let tenant_scope = parent
        .tenant_scope
        .clone()
        .map(serde_json::from_value::<WorkflowTenantScope>)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored parent tenant scope is invalid: {error}"),
            )
        })?;
    let inherited_tenant = tenant;
    if tenant_scope.as_ref().map(|scope| scope.tenant_id.as_str())
        != Some(inherited_tenant.tenant_id.as_str())
    {
        return Err(WorkflowMutationError::new(
            WorkflowErrorCode::TenantIncompatible,
            format!(
                "Parent Workflow Instance `{parent_instance_id}` has inconsistent Tenant Context"
            ),
        ));
    }
    Ok(context)
}

#[allow(clippy::too_many_arguments)]
async fn persist_unsupported_child_start(
    transaction: &mut Transaction<'_, Postgres>,
    link_id: &str,
    parent_instance_id: &str,
    parent_step_id: &str,
    parent_definition_version: &str,
    request: &WorkflowChildStartRequest,
    failure: &WorkflowFailureEvidence,
    now: DateTime<Utc>,
) -> Result<(), WorkflowMutationError> {
    let failure_json = encode_failure(failure)?;
    sqlx::query(
        r#"
        insert into platform.service_workflow_child_links (
            link_id, start_id, parent_instance_id, parent_step_id,
            parent_definition_version, child_definition_owner,
            child_definition_name, child_definition_version, child_instance_id,
            state, failure_evidence, next_action, created_at, updated_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, null,
                  'unsupported_version', $9, $10, $11, $11)
        "#,
    )
    .bind(link_id)
    .bind(&request.start_id)
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(parent_definition_version)
    .bind(&request.definition_owner)
    .bind(&request.definition_name)
    .bind(&request.definition_version)
    .bind(&failure_json)
    .bind(&failure.next_action)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!(
            "Could not persist unsupported child evidence: {error}"
        ))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'failed', transition_id = $3, completed_at = $4, updated_at = $4
        where instance_id = $1 and step_id = $2 and state = 'pending'
        "#,
    )
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(&request.start_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail unsupported child step: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = 'failed', failure_evidence = $2, terminal_transition_id = $3,
            updated_at = $4
        where instance_id = $1 and state = 'running'
        "#,
    )
    .bind(parent_instance_id)
    .bind(failure_json)
    .bind(&request.start_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail parent workflow: {error}"))
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn fail_parent_from_child(
    transaction: &mut Transaction<'_, Postgres>,
    row: &ChildResumeRow,
    parent_instance_id: &str,
    parent_step_id: &str,
    child_instance_id: &str,
    completion_delivery_id: &str,
    failure: &WorkflowFailureEvidence,
    fail_running_child: bool,
) -> Result<WorkflowChildResumeResult, WorkflowMutationError> {
    let now = postgres_now();
    let failure_json = encode_failure(failure)?;
    if fail_running_child {
        sqlx::query(
            r#"
            update platform.service_workflow_instances
            set state = 'failed', failure_evidence = $2, terminal_transition_id = $3,
                updated_at = $4
            where instance_id = $1 and state = 'running'
            "#,
        )
        .bind(child_instance_id)
        .bind(&failure_json)
        .bind(completion_delivery_id)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not fail unsupported child: {error}"))
        })?;
        sqlx::query(
            r#"
            update platform.service_workflow_steps
            set state = 'failed', transition_id = $2, completed_at = $3, updated_at = $3
            where instance_id = $1 and state = 'pending'
            "#,
        )
        .bind(child_instance_id)
        .bind(completion_delivery_id)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!("Could not fail unsupported child step: {error}"))
        })?;
    }
    sqlx::query(
        r#"
        update platform.service_workflow_child_links
        set state = 'failed', completion_delivery_id = $2, failure_evidence = $3,
            next_action = $4, updated_at = $5
        where link_id = $1 and state = 'waiting'
        "#,
    )
    .bind(&row.link_id)
    .bind(completion_delivery_id)
    .bind(&failure_json)
    .bind(&failure.next_action)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not persist child failure evidence: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'failed', transition_id = $3, completed_at = $4, updated_at = $4
        where instance_id = $1 and step_id = $2 and state = 'waiting_for_child'
        "#,
    )
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(completion_delivery_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail parent workflow step: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = 'failed', failure_evidence = $2, terminal_transition_id = $3,
            updated_at = $4
        where instance_id = $1 and state = 'running'
        "#,
    )
    .bind(parent_instance_id)
    .bind(failure_json)
    .bind(completion_delivery_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not fail parent workflow: {error}"))
    })?;
    Ok(WorkflowChildResumeResult {
        disposition: WorkflowTransitionDisposition::Applied,
        link_id: row.link_id.clone(),
        parent_instance_id: parent_instance_id.to_owned(),
        parent_step_id: parent_step_id.to_owned(),
        child_instance_id: child_instance_id.to_owned(),
        completion_delivery_id: completion_delivery_id.to_owned(),
        next_step_id: None,
        failure: Some(failure.clone()),
    })
}

#[allow(clippy::too_many_arguments)]
async fn complete_parent_from_child(
    transaction: &mut Transaction<'_, Postgres>,
    row: &ChildResumeRow,
    parent_definition: &lenso_contracts::WorkflowDefinition,
    parent_instance_id: &str,
    parent_step_id: &str,
    child_instance_id: &str,
    completion_delivery_id: &str,
    position: usize,
) -> Result<WorkflowChildResumeResult, WorkflowMutationError> {
    let now = postgres_now();
    sqlx::query(
        r#"
        update platform.service_workflow_steps
        set state = 'completed', transition_id = $3, completed_at = $4, updated_at = $4
        where instance_id = $1 and step_id = $2 and state = 'waiting_for_child'
        "#,
    )
    .bind(parent_instance_id)
    .bind(parent_step_id)
    .bind(completion_delivery_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not complete parent workflow step: {error}"))
    })?;
    let next_step_id = if let Some(next_step) = parent_definition.steps.get(position + 1) {
        let next_step_id = format!("workflow_step_{}", Uuid::now_v7());
        sqlx::query(
            r#"
            insert into platform.service_workflow_steps (
                step_id, instance_id, definition_step_name, step_position,
                state, created_at, updated_at
            ) values ($1, $2, $3, $4, 'pending', $5, $5)
            "#,
        )
        .bind(&next_step_id)
        .bind(parent_instance_id)
        .bind(&next_step.name)
        .bind(row.parent_step_position + 1)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            WorkflowMutationError::store(format!(
                "Could not persist resumed parent workflow step: {error}"
            ))
        })?;
        Some(next_step_id)
    } else {
        None
    };
    sqlx::query(
        r#"
        update platform.service_workflow_instances
        set state = $2, updated_at = $3
        where instance_id = $1 and state = 'running'
        "#,
    )
    .bind(parent_instance_id)
    .bind(if next_step_id.is_some() {
        "running"
    } else {
        "completed"
    })
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not resume parent workflow: {error}"))
    })?;
    sqlx::query(
        r#"
        update platform.service_workflow_child_links
        set state = 'completed', completion_delivery_id = $2,
            next_action = 'continue_parent_workflow', updated_at = $3
        where link_id = $1 and state = 'waiting'
        "#,
    )
    .bind(&row.link_id)
    .bind(completion_delivery_id)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not complete child workflow link: {error}"))
    })?;
    Ok(WorkflowChildResumeResult {
        disposition: WorkflowTransitionDisposition::Applied,
        link_id: row.link_id.clone(),
        parent_instance_id: parent_instance_id.to_owned(),
        parent_step_id: parent_step_id.to_owned(),
        child_instance_id: child_instance_id.to_owned(),
        completion_delivery_id: completion_delivery_id.to_owned(),
        next_step_id,
        failure: None,
    })
}

async fn next_step_id(
    transaction: &mut Transaction<'_, Postgres>,
    parent_instance_id: &str,
    parent_step_position: i32,
) -> Result<Option<String>, WorkflowMutationError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select step_id from platform.service_workflow_steps
        where instance_id = $1 and step_position = $2
        "#,
    )
    .bind(parent_instance_id)
    .bind(parent_step_position + 1)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| {
        WorkflowMutationError::store(format!("Could not inspect resumed parent step: {error}"))
    })
}

fn encode_failure(failure: &WorkflowFailureEvidence) -> Result<Value, WorkflowMutationError> {
    serde_json::to_value(failure).map_err(|error| {
        WorkflowMutationError::new(
            WorkflowErrorCode::StoredStateInvalid,
            format!("Could not encode workflow failure evidence: {error}"),
        )
    })
}

fn decode_failure(
    failure: Option<Value>,
) -> Result<Option<WorkflowFailureEvidence>, WorkflowMutationError> {
    failure
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| {
            WorkflowMutationError::new(
                WorkflowErrorCode::StoredStateInvalid,
                format!("Stored workflow failure evidence is invalid: {error}"),
            )
        })
}
