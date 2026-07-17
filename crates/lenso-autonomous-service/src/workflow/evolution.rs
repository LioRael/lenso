use super::{
    WorkflowApiError, WorkflowDefinitionIdentity, WorkflowErrorCode, encode_pinned_definition,
    resolve_definition, sha256_hex,
};
use crate::ServiceRuntimeState;
use axum::{
    Json,
    extract::{Path, State},
};
use lenso_contracts::{
    WorkflowCompatibilityCategory, WorkflowCompatibilityReason, WorkflowCompatibilityResult,
    WorkflowDefinition, evaluate_workflow_compatibility,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::BTreeMap;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

pub const WORKFLOW_MIGRATION_PLAN_PROTOCOL: &str = "lenso.workflow-migration-plan.v1";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowCompatibilityRequest {
    pub before: WorkflowDefinition,
    pub after: WorkflowDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowMigrationDryRunRequest {
    pub from_version: String,
    pub target_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStateMappingStatus {
    Preserved,
    Moved,
    Unmapped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStateMapping {
    pub from_step: String,
    pub from_position: u32,
    pub to_step: Option<String>,
    pub to_position: Option<u32>,
    pub status: WorkflowStateMappingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMigrationAffectedStep {
    pub name: String,
    pub position: u32,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMigrationAffectedInstance {
    pub instance_id: String,
    pub definition_digest: Option<String>,
    pub state: String,
    pub steps: Vec<WorkflowMigrationAffectedStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowMigrationApprovalBoundary {
    InFlightWorkflowMigration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowMigrationDryRunPlan {
    pub protocol: String,
    pub plan_id: String,
    pub mutates_state: bool,
    pub service_id: String,
    pub source_definition: WorkflowDefinitionIdentity,
    pub source_definition_digest: String,
    pub target_definition: WorkflowDefinitionIdentity,
    pub target_definition_digest: String,
    pub compatibility: WorkflowCompatibilityResult,
    pub affected_instances: Vec<WorkflowMigrationAffectedInstance>,
    pub state_mapping: Vec<WorkflowStateMapping>,
    pub rollback_constraints: Vec<String>,
    pub approval_boundary: WorkflowMigrationApprovalBoundary,
    pub next_actions: Vec<String>,
}

#[derive(Debug, FromRow)]
struct MigrationStateRow {
    instance_id: String,
    definition_digest: Option<String>,
    instance_state: String,
    step_name: String,
    step_position: i32,
    step_state: String,
}

pub(super) fn workflow_evolution_router() -> OpenApiRouter<ServiceRuntimeState> {
    OpenApiRouter::new()
        .routes(routes!(check_workflow_compatibility))
        .routes(routes!(dry_run_workflow_migration))
}

#[utoipa::path(
    post,
    path = "/runtime/workflows/definitions/compatibility",
    request_body = WorkflowCompatibilityRequest,
    responses(
        (status = 200, body = WorkflowCompatibilityResult),
        (status = 400, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn check_workflow_compatibility(
    request: Result<Json<WorkflowCompatibilityRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<WorkflowCompatibilityResult>, WorkflowApiError> {
    let Json(request) = request.map_err(|_| {
        WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow compatibility input must match the v1 request contract",
        )
    })?;
    Ok(Json(evaluate_workflow_compatibility(
        &request.before,
        &request.after,
    )))
}

#[utoipa::path(
    post,
    path = "/runtime/workflows/{owner}/{name}/migration-plans/dry-run",
    params(
        ("owner" = String, Path, description = "Owning Module identity"),
        ("name" = String, Path, description = "Stable Workflow Definition name")
    ),
    request_body = WorkflowMigrationDryRunRequest,
    responses(
        (status = 200, body = WorkflowMigrationDryRunPlan),
        (status = 400, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 404, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
async fn dry_run_workflow_migration(
    State(state): State<ServiceRuntimeState>,
    Path((owner, name)): Path<(String, String)>,
    request: Result<Json<WorkflowMigrationDryRunRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<WorkflowMigrationDryRunPlan>, WorkflowApiError> {
    let Json(request) = request.map_err(|_| {
        WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow migration dry run must match the v1 request contract",
        )
    })?;
    if owner.trim().is_empty()
        || name.trim().is_empty()
        || request.from_version.trim().is_empty()
        || request.target_version.trim().is_empty()
        || request.from_version == request.target_version
    {
        return Err(WorkflowApiError::invalid(
            WorkflowErrorCode::InvalidRequest,
            "Workflow migration requires distinct non-empty source and target versions",
        ));
    }
    let before = resolve_definition(&state, &owner, &name, &request.from_version)?;
    let after = resolve_definition(&state, &owner, &name, &request.target_version)?;
    let mut compatibility = evaluate_workflow_compatibility(&before, &after);
    let (_, source_definition_digest) = encode_pinned_definition(&before)?;
    let (_, target_definition_digest) = encode_pinned_definition(&after)?;
    let pool = state
        .store()
        .map_err(|error| WorkflowApiError::store(error.public_message))?;
    let rows = sqlx::query_as::<_, MigrationStateRow>(
        r#"
        select instance.instance_id, instance.definition_digest,
               instance.state as instance_state,
               step.definition_step_name as step_name,
               step.step_position, step.state as step_state
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.service_id = $1 and instance.definition_owner = $2
          and instance.definition_name = $3 and instance.definition_version = $4
          and instance.state = 'running'
        order by instance.instance_id, step.step_position, step.step_id
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(&owner)
    .bind(&name)
    .bind(&request.from_version)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        WorkflowApiError::store(format!(
            "Could not inspect in-flight Workflow Instances: {error}"
        ))
    })?;
    let affected_instances = affected_instances(rows)?;
    block_reinterpreted_source_instances(
        &mut compatibility,
        &affected_instances,
        &source_definition_digest,
    );
    let state_mapping = state_mapping(&before, &after)?;
    let source_definition = WorkflowDefinitionIdentity {
        owner: before.owner.clone(),
        name: before.name.clone(),
        version: before.version.clone(),
    };
    let target_definition = WorkflowDefinitionIdentity {
        owner: after.owner.clone(),
        name: after.name.clone(),
        version: after.version.clone(),
    };
    let rollback_constraints = vec![
        "The source definition must remain deployable until every approved migration completes."
            .to_owned(),
        "Rollback is not automatic after target-version business work has started.".to_owned(),
        "This dry-run plan never changes an instance, timer, attempt, or worker claim.".to_owned(),
    ];
    let approval_boundary = WorkflowMigrationApprovalBoundary::InFlightWorkflowMigration;
    let next_action = match compatibility.category {
        WorkflowCompatibilityCategory::Safe | WorkflowCompatibilityCategory::NeedsAttention => {
            "request_in_flight_workflow_migration_approval"
        }
        WorkflowCompatibilityCategory::Breaking | WorkflowCompatibilityCategory::Blocked => {
            "revise_workflow_definition_or_state_mapping"
        }
    };
    let next_actions = vec![next_action.to_owned()];
    let plan_material = serde_json::json!({
        "protocol": WORKFLOW_MIGRATION_PLAN_PROTOCOL,
        "mutatesState": false,
        "serviceId": &state.identity.service_id,
        "sourceDefinition": &source_definition,
        "sourceDefinitionDigest": &source_definition_digest,
        "targetDefinition": &target_definition,
        "targetDefinitionDigest": &target_definition_digest,
        "compatibility": &compatibility,
        "affectedInstances": &affected_instances,
        "stateMapping": &state_mapping,
        "rollbackConstraints": &rollback_constraints,
        "approvalBoundary": approval_boundary,
        "nextActions": &next_actions,
    });
    let plan_id = workflow_migration_plan_id(&plan_material)?;
    Ok(Json(WorkflowMigrationDryRunPlan {
        protocol: WORKFLOW_MIGRATION_PLAN_PROTOCOL.to_owned(),
        plan_id,
        mutates_state: false,
        service_id: state.identity.service_id.clone(),
        source_definition,
        source_definition_digest,
        target_definition,
        target_definition_digest,
        compatibility,
        affected_instances,
        state_mapping,
        rollback_constraints,
        approval_boundary,
        next_actions,
    }))
}

fn workflow_migration_plan_id(
    plan_material: &serde_json::Value,
) -> Result<String, WorkflowApiError> {
    let bytes = serde_json::to_vec(plan_material)
        .map_err(|error| WorkflowApiError::stored_state(error.to_string()))?;
    Ok(format!("workflow_migration_sha256_{}", sha256_hex(&bytes)))
}

fn block_reinterpreted_source_instances(
    compatibility: &mut WorkflowCompatibilityResult,
    affected_instances: &[WorkflowMigrationAffectedInstance],
    expected_source_digest: &str,
) {
    for (index, instance) in affected_instances.iter().enumerate() {
        if instance.definition_digest.as_deref() == Some(expected_source_digest) {
            continue;
        }
        compatibility.category = WorkflowCompatibilityCategory::Blocked;
        compatibility.reasons.push(WorkflowCompatibilityReason {
            code: "workflow_in_flight_source_artifact_mismatch".to_owned(),
            path: format!("$.affectedInstances[{index}].definitionDigest"),
            message: format!(
                "Workflow Instance `{}` is pinned to a different source artifact than this worker registered.",
                instance.instance_id
            ),
            next_action: "Deploy a worker that retains the exact pinned source definition before planning migration."
                .to_owned(),
        });
    }
    compatibility.reasons.sort();
    compatibility.reasons.dedup();
}

fn affected_instances(
    rows: Vec<MigrationStateRow>,
) -> Result<Vec<WorkflowMigrationAffectedInstance>, WorkflowApiError> {
    let mut instances = BTreeMap::<String, WorkflowMigrationAffectedInstance>::new();
    for row in rows {
        let position = u32::try_from(row.step_position).map_err(|_| {
            WorkflowApiError::stored_state(format!(
                "Workflow Instance `{}` has an invalid step position",
                row.instance_id
            ))
        })?;
        let instance = instances.entry(row.instance_id.clone()).or_insert_with(|| {
            WorkflowMigrationAffectedInstance {
                instance_id: row.instance_id,
                definition_digest: row.definition_digest,
                state: row.instance_state,
                steps: Vec::new(),
            }
        });
        instance.steps.push(WorkflowMigrationAffectedStep {
            name: row.step_name,
            position,
            state: row.step_state,
        });
    }
    Ok(instances.into_values().collect())
}

fn state_mapping(
    before: &WorkflowDefinition,
    after: &WorkflowDefinition,
) -> Result<Vec<WorkflowStateMapping>, WorkflowApiError> {
    before
        .steps
        .iter()
        .enumerate()
        .map(|(from_position, step)| {
            let target = after
                .steps
                .iter()
                .position(|candidate| candidate.name == step.name);
            let status = match target {
                Some(to_position) if to_position == from_position => {
                    WorkflowStateMappingStatus::Preserved
                }
                Some(_) => WorkflowStateMappingStatus::Moved,
                None => WorkflowStateMappingStatus::Unmapped,
            };
            Ok(WorkflowStateMapping {
                from_step: step.name.clone(),
                from_position: u32::try_from(from_position).map_err(|_| {
                    WorkflowApiError::stored_state("Workflow source step position exceeds u32")
                })?,
                to_step: target.map(|position| after.steps[position].name.clone()),
                to_position: target.map(u32::try_from).transpose().map_err(|_| {
                    WorkflowApiError::stored_state("Workflow target step position exceeds u32")
                })?,
                status,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_plan_id_binds_the_exact_target_definition_digest() {
        let first = serde_json::json!({
            "protocol": WORKFLOW_MIGRATION_PLAN_PROTOCOL,
            "targetDefinition": {
                "owner": "support-sla",
                "name": "ticket_sla",
                "version": "v2"
            },
            "targetDefinitionDigest": "sha256:target-8s",
            "compatibility": {
                "category": "needs-attention",
                "reasons": [{"code": "workflow_timeout_changed", "path": "$.after.steps[0].timeoutMs"}]
            }
        });
        let mut second = first.clone();
        second["targetDefinitionDigest"] = "sha256:target-9s".into();

        assert_ne!(
            workflow_migration_plan_id(&first).unwrap(),
            workflow_migration_plan_id(&second).unwrap()
        );
    }
}
