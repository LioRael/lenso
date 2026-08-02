//! Console HTTP adapter for the shared Module management kernel.
//! The adapter accepts business changes and returns kernel-owned snapshots or
//! immutable plans; it never reimplements resolution, Cargo, or workspace rules.

mod system_plane;

pub use system_plane::*;

use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
};
use lenso_module_management::{
    ApproveModuleOperation, MANAGEMENT_HOLDER, ManagementActor, MigrationExecutionMode,
    ModuleEffectAdapter, ModuleEffectAdapterError, ModuleEffectExecution, ModuleEffectOutcome,
    ModulePlanEffect, ModuleRootChange, ServiceDeploymentAction, ServiceInstallationPlan,
    StartReviewedModulePlan, WorkspaceModuleManagement, WorkspaceModuleOperator,
    WorkspaceModuleOperatorError, WorkspaceServiceInstallationManager,
    application_module_lock_digest,
};
use platform_core::{AppContext, AppError, DbPool, ErrorCode, Shutdown, apply_module_migration};
use platform_http::{
    AdminActor, ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext,
    OpenApiRouter, routes,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(management_snapshot))
        .routes(routes!(preview_change_plan))
        .routes(routes!(start_operation))
        .routes(routes!(get_operation))
        .routes(routes!(get_operation_journal))
        .routes(routes!(apply_operation))
        .routes(routes!(approve_operation))
        .routes(routes!(cancel_operation))
        .routes(routes!(retry_operation))
        .routes(routes!(resume_operation))
        .routes(routes!(service_installation_snapshot))
        .routes(routes!(preview_service_installation))
        .routes(routes!(apply_service_installation_plan))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartOperationBody {
    idempotency_key: String,
    plan: lenso_module_management::ModuleChangePlan,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionBody {
    expected_revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalBody {
    expected_revision: u64,
    boundary_id: String,
    reason: String,
    nonce: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceInstallationScope {
    system_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewServiceInstallationBody {
    system_id: String,
    environment_id: String,
    change: lenso_module_management::ServiceInstallationChange,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyServiceInstallationBody {
    operation_id: String,
    plan: ServiceInstallationPlan,
}

#[utoipa::path(
    get,
    path = "/admin/modules/management",
    operation_id = "admin_modules_management_snapshot",
    tag = "module-management",
    params(("authorization" = String, Header, description = "Service or system bearer token")),
    responses(
        (status = 200, description = "Target-owned Module composition and planning readiness", body = Value, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 500, description = "Module management state cannot be read", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
#[allow(clippy::result_large_err)]
async fn management_snapshot(
    _admin: AdminActor,
    HttpRequestContext(request_context): HttpRequestContext,
) -> Result<Json<Value>, ApiErrorResponse> {
    let root = std::env::current_dir().map_err(|error| {
        api_error(
            ErrorCode::Internal,
            format!("Module management root is unavailable: {error}"),
            &request_context,
        )
    })?;
    let snapshot = WorkspaceModuleManagement::new(root)
        .snapshot()
        .map_err(|error| {
            api_error(
                ErrorCode::Internal,
                format!("Module management snapshot failed: {error}"),
                &request_context,
            )
        })?;
    serde_json::to_value(snapshot).map(Json).map_err(|error| {
        api_error(
            ErrorCode::Internal,
            format!("Module management snapshot serialization failed: {error}"),
            &request_context,
        )
    })
}

#[utoipa::path(
    post,
    path = "/admin/modules/plans/preview",
    operation_id = "admin_modules_preview_change_plan",
    tag = "module-management",
    params(("authorization" = String, Header, description = "Service or system bearer token")),
    request_body(content = Value, description = "One lenso ModuleRootChange value", content_type = "application/json"),
    responses(
        (status = 200, description = "Immutable complete Module Change Plan", body = Value, content_type = "application/json"),
        (status = 400, description = "Change or planning input is invalid", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 409, description = "Planning context is not available", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
#[allow(clippy::result_large_err)]
async fn preview_change_plan(
    _admin: AdminActor,
    HttpRequestContext(request_context): HttpRequestContext,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let change: ModuleRootChange = serde_json::from_value(body).map_err(|error| {
        api_error(
            ErrorCode::Validation,
            format!("Module change is invalid: {error}"),
            &request_context,
        )
    })?;
    let root = std::env::current_dir().map_err(|error| {
        api_error(
            ErrorCode::Internal,
            format!("Module management root is unavailable: {error}"),
            &request_context,
        )
    })?;
    let plan = WorkspaceModuleManagement::new(root)
        .preview(change, chrono::Utc::now())
        .map_err(|error| {
            let code = if matches!(
                error,
                lenso_module_management::WorkspaceModuleManagementError::PlanningUnavailable
            ) {
                ErrorCode::Conflict
            } else {
                ErrorCode::Validation
            };
            api_error(
                code,
                format!("Module planning failed: {error}"),
                &request_context,
            )
        })?;
    serde_json::to_value(plan).map(Json).map_err(|error| {
        api_error(
            ErrorCode::Internal,
            format!("Module plan serialization failed: {error}"),
            &request_context,
        )
    })
}

#[utoipa::path(post, path = "/admin/modules/operations", operation_id = "admin_modules_start_operation", tag = "module-management", request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Durable Module operation", body = Value), (status = 400, description = "Reviewed plan is stale or invalid", body = ErrorResponse), (status = 401, description = "Authentication is required", body = ErrorResponse), (status = 403, description = "Management authority is required", body = ErrorResponse), (status = 409, description = "Operation conflicts with target state", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn start_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: StartOperationBody = decode(body, &request_context)?;
    let actor = management_actor(admin);
    let root = management_root(&request_context)?;
    let operator = WorkspaceModuleOperator::new(root, HostModuleEffectAdapter::new(&ctx));
    let operation = operator
        .start(
            &StartReviewedModulePlan {
                idempotency_key: body.idempotency_key,
                plan: body.plan,
            },
            &actor,
            MANAGEMENT_HOLDER,
            chrono::Utc::now(),
        )
        .map_err(|error| operator_error(error, &request_context))?;
    value(operation, &request_context)
}

#[utoipa::path(get, path = "/admin/modules/operations/{operation_id}", operation_id = "admin_modules_get_operation", tag = "module-management", params(("operation_id" = String, Path)), responses((status = 200, description = "Current durable Module operation", body = Value), (status = 404, description = "Operation was not found", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn get_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let _actor = management_actor(admin);
    let operator = WorkspaceModuleOperator::new(
        management_root(&request_context)?,
        HostModuleEffectAdapter::new(&ctx),
    );
    value(
        operator
            .operation(&operation_id)
            .map_err(|error| operator_error(error, &request_context))?,
        &request_context,
    )
}

#[utoipa::path(get, path = "/admin/modules/operations/{operation_id}/journal", operation_id = "admin_modules_get_operation_journal", tag = "module-management", params(("operation_id" = String, Path)), responses((status = 200, description = "Hash-chained Module operation journal", body = Value), (status = 404, description = "Operation was not found", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn get_operation_journal(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let _actor = management_actor(admin);
    let operator = WorkspaceModuleOperator::new(
        management_root(&request_context)?,
        HostModuleEffectAdapter::new(&ctx),
    );
    value(
        operator
            .journal(&operation_id)
            .map_err(|error| operator_error(error, &request_context))?,
        &request_context,
    )
}

#[utoipa::path(post, path = "/admin/modules/operations/{operation_id}/apply", operation_id = "admin_modules_apply_operation", tag = "module-management", params(("operation_id" = String, Path)), responses((status = 200, description = "Applied, blocked, or completed Module operation", body = Value), (status = 409, description = "Operation cannot currently be applied", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn apply_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let actor = management_actor(admin);
    run_effectful(ctx, request_context, move |operator| {
        operator.apply(&operation_id, &actor, chrono::Utc::now())
    })
    .await
}

#[utoipa::path(post, path = "/admin/modules/operations/{operation_id}/approvals", operation_id = "admin_modules_approve_operation", tag = "module-management", params(("operation_id" = String, Path)), request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Operation with plan-bound approval", body = Value), (status = 400, description = "Approval is invalid", body = ErrorResponse), (status = 409, description = "Approval is stale", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn approve_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: ApprovalBody = decode(body, &request_context)?;
    let actor = management_actor(admin);
    let operator = WorkspaceModuleOperator::new(
        management_root(&request_context)?,
        HostModuleEffectAdapter::new(&ctx),
    );
    let operation = operator
        .approve(
            &operation_id,
            ApproveModuleOperation {
                expected_revision: body.expected_revision,
                boundary_id: body.boundary_id,
                reason: body.reason,
                nonce: body.nonce,
            },
            &actor,
            MANAGEMENT_HOLDER,
            chrono::Utc::now(),
        )
        .map_err(|error| operator_error(error, &request_context))?;
    value(operation, &request_context)
}

#[utoipa::path(post, path = "/admin/modules/operations/{operation_id}/cancel", operation_id = "admin_modules_cancel_operation", tag = "module-management", params(("operation_id" = String, Path)), request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Cancelled pre-mutation Module operation", body = Value), (status = 409, description = "Cancellation is unsafe", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn cancel_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: RevisionBody = decode(body, &request_context)?;
    let actor = management_actor(admin);
    let operator = WorkspaceModuleOperator::new(
        management_root(&request_context)?,
        HostModuleEffectAdapter::new(&ctx),
    );
    value(
        operator
            .cancel(
                &operation_id,
                body.expected_revision,
                &actor,
                chrono::Utc::now(),
            )
            .map_err(|error| operator_error(error, &request_context))?,
        &request_context,
    )
}

#[utoipa::path(post, path = "/admin/modules/operations/{operation_id}/retry", operation_id = "admin_modules_retry_operation", tag = "module-management", params(("operation_id" = String, Path)), request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Retried Module operation", body = Value), (status = 409, description = "Operation cannot be retried", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn retry_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: RevisionBody = decode(body, &request_context)?;
    let actor = management_actor(admin);
    run_effectful(ctx, request_context, move |operator| {
        operator.retry(
            &operation_id,
            body.expected_revision,
            &actor,
            chrono::Utc::now(),
        )
    })
    .await
}

#[utoipa::path(post, path = "/admin/modules/operations/{operation_id}/resume", operation_id = "admin_modules_resume_operation", tag = "module-management", params(("operation_id" = String, Path)), request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Crash-resumed Module operation", body = Value), (status = 409, description = "Safe continuation cannot be proven", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn resume_operation(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(operation_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: RevisionBody = decode(body, &request_context)?;
    let actor = management_actor(admin);
    run_effectful(ctx, request_context, move |operator| {
        operator.resume(
            &operation_id,
            body.expected_revision,
            &actor,
            chrono::Utc::now(),
        )
    })
    .await
}

#[utoipa::path(get, path = "/admin/services/installations/{environment_id}", operation_id = "admin_services_installation_snapshot", tag = "service-management", params(("environment_id" = String, Path), ("system_id" = String, Query), ("authorization" = String, Header, description = "Service or system bearer token")), responses((status = 200, description = "Target-owned desired Service Installation Set", body = Value), (status = 401, description = "Authentication is required", body = ErrorResponse), (status = 403, description = "Service or system authentication is required", body = ErrorResponse), (status = 500, description = "Installation state cannot be read", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn service_installation_snapshot(
    _admin: AdminActor,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(environment_id): AxumPath<String>,
    Query(scope): Query<ServiceInstallationScope>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let manager = WorkspaceServiceInstallationManager::new(
        management_root(&request_context)?,
        scope.system_id,
        environment_id,
    );
    value(
        manager
            .snapshot()
            .map_err(|error| service_installation_error(error, &request_context))?,
        &request_context,
    )
}

#[utoipa::path(post, path = "/admin/services/installations/plans/preview", operation_id = "admin_services_preview_installation", tag = "service-management", request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Immutable Service Installation Plan", body = Value), (status = 400, description = "Installation change is invalid", body = ErrorResponse), (status = 401, description = "Authentication is required", body = ErrorResponse), (status = 403, description = "Service or system authentication is required", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn preview_service_installation(
    _admin: AdminActor,
    HttpRequestContext(request_context): HttpRequestContext,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: PreviewServiceInstallationBody = decode(body, &request_context)?;
    let manager = WorkspaceServiceInstallationManager::new(
        management_root(&request_context)?,
        body.system_id,
        body.environment_id,
    );
    value(
        manager
            .preview(body.change, chrono::Utc::now())
            .map_err(|error| service_installation_error(error, &request_context))?,
        &request_context,
    )
}

#[utoipa::path(post, path = "/admin/services/installations/plans/{plan_id}/apply", operation_id = "admin_services_apply_installation", tag = "service-management", params(("plan_id" = String, Path)), request_body(content = Value, content_type = "application/json"), responses((status = 200, description = "Durable Service Installation Receipt", body = Value), (status = 400, description = "Installation plan is invalid", body = ErrorResponse), (status = 401, description = "Authentication is required", body = ErrorResponse), (status = 403, description = "service.manage authority is required", body = ErrorResponse), (status = 409, description = "Installation Set changed after preview", body = ErrorResponse)))]
#[allow(clippy::result_large_err)]
async fn apply_service_installation_plan(
    admin: AdminActor,
    HttpRequestContext(request_context): HttpRequestContext,
    AxumPath(plan_id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiErrorResponse> {
    let body: ApplyServiceInstallationBody = decode(body, &request_context)?;
    if body.plan.plan_id != plan_id {
        return Err(api_error(
            ErrorCode::Validation,
            "Service Installation plan identity differs from request path".to_owned(),
            &request_context,
        ));
    }
    let actor = management_actor(admin);
    let manager = WorkspaceServiceInstallationManager::new(
        management_root(&request_context)?,
        &body.plan.system_id,
        &body.plan.environment_id,
    );
    value(
        manager
            .apply(
                &body.operation_id,
                &body.plan,
                &actor.actor_id,
                &actor.verified_authorities,
                chrono::Utc::now(),
            )
            .map_err(|error| service_installation_error(error, &request_context))?,
        &request_context,
    )
}

#[derive(Debug, Clone)]
struct HostModuleEffectAdapter {
    shutdown: Shutdown,
    db: DbPool,
    runtime: tokio::runtime::Handle,
}

impl HostModuleEffectAdapter {
    fn new(ctx: &AppContext) -> Self {
        Self {
            shutdown: ctx.shutdown.clone(),
            db: ctx.db.clone(),
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl ModuleEffectAdapter for HostModuleEffectAdapter {
    fn execute(
        &self,
        workspace_root: &Path,
        operation: &lenso_module_management::ModuleOperation,
        effect: &ModulePlanEffect,
    ) -> Result<ModuleEffectExecution, ModuleEffectAdapterError> {
        let outcome = match effect {
            ModulePlanEffect::Validate {
                effect_id,
                command,
                expected_evidence,
            } if command == "cargo check --locked" => {
                let lock = fs::read(workspace_root.join("Cargo.lock"))
                    .map_err(|error| failed(effect_id, error))?;
                if sha256(&lock) != *expected_evidence {
                    return Err(failed(
                        effect_id,
                        "Cargo.lock no longer matches the reviewed candidate",
                    ));
                }
                let output = Command::new("cargo")
                    .args(["check", "--locked"])
                    .current_dir(workspace_root)
                    .output()
                    .map_err(|error| failed(effect_id, error))?;
                if !output.status.success() {
                    return Err(failed(effect_id, String::from_utf8_lossy(&output.stderr)));
                }
                ModuleEffectOutcome::Verified
            }
            ModulePlanEffect::Restart { target, .. } if target == "host" => {
                self.shutdown.signal();
                ModuleEffectOutcome::Applied
            }
            ModulePlanEffect::Activate {
                effect_id,
                target_lock_digest,
            } => {
                let bytes = fs::read(workspace_root.join("lenso.modules.lock.json"))
                    .map_err(|error| failed(effect_id, error))?;
                let lock =
                    serde_json::from_slice(&bytes).map_err(|error| failed(effect_id, error))?;
                if application_module_lock_digest(&lock)
                    .map_err(|error| failed(effect_id, error))?
                    != *target_lock_digest
                {
                    return Err(failed(
                        effect_id,
                        "application lock does not match reviewed target",
                    ));
                }
                ModuleEffectOutcome::Activated
            }
            ModulePlanEffect::Migration {
                effect_id,
                module_id,
                release_digest,
                migration_id,
                artifact_locator,
                artifact_digest,
                store_scope,
                execution,
                ..
            } if store_scope == "host" && *execution == MigrationExecutionMode::Transactional => {
                let artifact = verified_workspace_artifact(
                    workspace_root,
                    artifact_locator,
                    artifact_digest,
                    effect_id,
                )?;
                let sql = std::str::from_utf8(&artifact.bytes)
                    .map_err(|error| failed(effect_id, error))?;
                let name = format!("{module_id}/{release_digest}/{migration_id}");
                self.runtime
                    .block_on(apply_module_migration(
                        &self.db,
                        &name,
                        artifact_digest,
                        sql,
                    ))
                    .map_err(|error| failed(effect_id, error))?;
                return Ok(ModuleEffectExecution {
                    outcome: ModuleEffectOutcome::Applied,
                    evidence_references: vec![
                        artifact.reference,
                        write_effect_receipt(
                            workspace_root,
                            operation,
                            effect,
                            "module_migration_applied",
                        )?,
                    ],
                });
            }
            ModulePlanEffect::ServiceInstallation {
                installation_plan,
                action,
                ..
            } => {
                if let Some(receipt) = existing_effect_receipt(workspace_root, effect)? {
                    return Ok(ModuleEffectExecution {
                        outcome: ModuleEffectOutcome::Applied,
                        evidence_references: vec![receipt],
                    });
                }
                let mut references = Vec::new();
                if let Some(plan) = installation_plan {
                    references.push(apply_service_installation(
                        workspace_root,
                        operation,
                        effect,
                        plan,
                    )?);
                }
                let action =
                    action
                        .as_ref()
                        .ok_or_else(|| ModuleEffectAdapterError::Unsupported {
                            effect_id: effect.effect_id().to_owned(),
                            reason:
                                "desired Service installation was applied but no target-owned deployment action is available"
                                    .to_owned(),
                        })?;
                references.push(execute_service_action(workspace_root, effect, action)?);
                references.push(write_effect_receipt(
                    workspace_root,
                    operation,
                    effect,
                    "service_installation_and_deployment_applied",
                )?);
                return Ok(ModuleEffectExecution {
                    outcome: ModuleEffectOutcome::Applied,
                    evidence_references: references,
                });
            }
            ModulePlanEffect::ConsoleComposition { effect_id, .. } => {
                if let Some(receipt) = existing_effect_receipt(workspace_root, effect)? {
                    return Ok(ModuleEffectExecution {
                        outcome: ModuleEffectOutcome::Applied,
                        evidence_references: vec![receipt],
                    });
                }
                let management_url =
                    std::env::var("LENSO_CONSOLE_MANAGEMENT_URL").map_err(|_| {
                        ModuleEffectAdapterError::Unsupported {
                            effect_id: effect_id.clone(),
                            reason: "LENSO_CONSOLE_MANAGEMENT_URL is not configured".to_owned(),
                        }
                    })?;
                let management_token =
                    std::env::var("LENSO_CONSOLE_MANAGEMENT_TOKEN").map_err(|_| {
                        ModuleEffectAdapterError::Unsupported {
                            effect_id: effect_id.clone(),
                            reason: "LENSO_CONSOLE_MANAGEMENT_TOKEN is not configured".to_owned(),
                        }
                    })?;
                let management_url = reqwest::Url::parse(&management_url)
                    .map_err(|error| failed(effect_id, error))?;
                if management_url.scheme() != "https"
                    && !(management_url.scheme() == "http"
                        && management_url.host_str().is_some_and(|host| {
                            host == "localhost"
                                || host
                                    .parse::<std::net::IpAddr>()
                                    .is_ok_and(|ip| ip.is_loopback())
                        }))
                {
                    return Err(failed(
                        effect_id,
                        "Console management URL must use HTTPS or loopback HTTP",
                    ));
                }
                let endpoint = format!(
                    "{}/api/console/v1/artifacts/reconcile",
                    management_url.as_str().trim_end_matches('/')
                );
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|error| failed(effect_id, error))?;
                let response = self
                    .runtime
                    .block_on(
                        client
                            .post(endpoint)
                            .bearer_auth(management_token)
                            .json(effect)
                            .send(),
                    )
                    .map_err(|error| failed(effect_id, error))?;
                if !response.status().is_success() {
                    let status = response.status();
                    let body = self.runtime.block_on(response.text()).unwrap_or_default();
                    return Err(failed(
                        effect_id,
                        format!("Console composition request failed with {status}: {body}"),
                    ));
                }
                let receipt = write_effect_receipt(
                    workspace_root,
                    operation,
                    effect,
                    "console_composition_reconciled",
                )?;
                return Ok(ModuleEffectExecution {
                    outcome: ModuleEffectOutcome::Applied,
                    evidence_references: vec![receipt],
                });
            }
            ModulePlanEffect::ServiceRemoval { action, .. }
            | ModulePlanEffect::ServiceRestart { action, .. } => {
                if let Some(receipt) = existing_effect_receipt(workspace_root, effect)? {
                    return Ok(ModuleEffectExecution {
                        outcome: ModuleEffectOutcome::Applied,
                        evidence_references: vec![receipt],
                    });
                }
                let action =
                    action
                        .as_ref()
                        .ok_or_else(|| ModuleEffectAdapterError::Unsupported {
                            effect_id: effect.effect_id().to_owned(),
                            reason: "reviewed plan has no target-owned Service deployment action"
                                .to_owned(),
                        })?;
                let evidence = execute_service_action(workspace_root, effect, action)?;
                let mut references = vec![evidence];
                references.push(write_effect_receipt(
                    workspace_root,
                    operation,
                    effect,
                    "service_deployment_action_applied",
                )?);
                return Ok(ModuleEffectExecution {
                    outcome: ModuleEffectOutcome::Applied,
                    evidence_references: references,
                });
            }
            _ => {
                return Err(ModuleEffectAdapterError::Unsupported {
                    effect_id: effect.effect_id().to_owned(),
                    reason: "no deployment or migration adapter is configured for this target"
                        .to_owned(),
                });
            }
        };
        Ok(ModuleEffectExecution {
            outcome,
            evidence_references: Vec::new(),
        })
    }
}

#[allow(clippy::result_large_err)]
async fn run_effectful(
    ctx: AppContext,
    request_context: platform_core::RequestContext,
    run: impl FnOnce(
        WorkspaceModuleOperator<HostModuleEffectAdapter>,
    )
        -> Result<lenso_module_management::ModuleOperation, WorkspaceModuleOperatorError>
    + Send
    + 'static,
) -> Result<Json<Value>, ApiErrorResponse> {
    let root = management_root(&request_context)?;
    let adapter = HostModuleEffectAdapter::new(&ctx);
    let result =
        tokio::task::spawn_blocking(move || run(WorkspaceModuleOperator::new(root, adapter)))
            .await
            .map_err(|error| {
                api_error(
                    ErrorCode::Internal,
                    format!("Module operation task failed: {error}"),
                    &request_context,
                )
            })?;
    value(
        result.map_err(|error| operator_error(error, &request_context))?,
        &request_context,
    )
}

struct VerifiedArtifact {
    bytes: Vec<u8>,
    reference: lenso_contracts::ArtifactReference,
}

fn verified_workspace_artifact(
    workspace_root: &Path,
    locator: &str,
    expected_digest: &str,
    effect_id: &str,
) -> Result<VerifiedArtifact, ModuleEffectAdapterError> {
    let root = workspace_root
        .canonicalize()
        .map_err(|error| failed(effect_id, error))?;
    let path = workspace_root
        .join(locator)
        .canonicalize()
        .map_err(|error| failed(effect_id, error))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(failed(effect_id, "artifact escapes the managed workspace"));
    }
    let bytes = fs::read(&path).map_err(|error| failed(effect_id, error))?;
    if sha256(&bytes) != expected_digest {
        return Err(failed(
            effect_id,
            "artifact digest differs from the reviewed plan",
        ));
    }
    Ok(VerifiedArtifact {
        bytes,
        reference: lenso_contracts::ArtifactReference {
            locator: locator.to_owned(),
            digest: expected_digest.to_owned(),
        },
    })
}

fn execute_service_action(
    workspace_root: &Path,
    effect: &ModulePlanEffect,
    action: &ServiceDeploymentAction,
) -> Result<lenso_contracts::ArtifactReference, ModuleEffectAdapterError> {
    let effect_id = effect.effect_id();
    match action {
        ServiceDeploymentAction::Evidence { receipt } => Ok(verified_workspace_artifact(
            workspace_root,
            &receipt.locator,
            &receipt.digest,
            effect_id,
        )?
        .reference),
        ServiceDeploymentAction::Command {
            program,
            args,
            working_directory,
        } => {
            let executable = program.rsplit(['/', '\\']).next().unwrap_or_default();
            if matches!(
                executable.to_ascii_lowercase().as_str(),
                "sh" | "bash"
                    | "dash"
                    | "zsh"
                    | "fish"
                    | "cmd"
                    | "cmd.exe"
                    | "powershell"
                    | "powershell.exe"
                    | "pwsh"
                    | "pwsh.exe"
            ) {
                return Err(failed(
                    effect_id,
                    "shell programs are not valid deployment adapters",
                ));
            }
            let directory =
                command_directory(workspace_root, working_directory.as_deref(), effect_id)?;
            let output = Command::new(program)
                .args(args)
                .current_dir(directory)
                .output()
                .map_err(|error| failed(effect_id, error))?;
            if !output.status.success() {
                return Err(failed(
                    effect_id,
                    format!("Service deployment command exited with {}", output.status),
                ));
            }
            let digest = lenso_contracts::digest_json(&json!({
                "effect": effect,
                "program": program,
                "args": args,
                "exitCode": output.status.code(),
            }))
            .map_err(|error| failed(effect_id, error))?;
            Ok(lenso_contracts::ArtifactReference {
                locator: format!("command:{program}"),
                digest,
            })
        }
    }
}

fn command_directory(
    workspace_root: &Path,
    relative: Option<&str>,
    effect_id: &str,
) -> Result<PathBuf, ModuleEffectAdapterError> {
    let root = workspace_root
        .canonicalize()
        .map_err(|error| failed(effect_id, error))?;
    let directory = relative.map_or_else(|| root.clone(), |path| workspace_root.join(path));
    let directory = directory
        .canonicalize()
        .map_err(|error| failed(effect_id, error))?;
    if !directory.starts_with(&root) || !directory.is_dir() {
        return Err(failed(
            effect_id,
            "command working directory escapes the managed workspace",
        ));
    }
    Ok(directory)
}

fn write_effect_receipt(
    workspace_root: &Path,
    operation: &lenso_module_management::ModuleOperation,
    effect: &ModulePlanEffect,
    outcome: &str,
) -> Result<lenso_contracts::ArtifactReference, ModuleEffectAdapterError> {
    let effect_id = effect.effect_id();
    let effect_digest =
        lenso_contracts::digest_json(effect).map_err(|error| failed(effect_id, error))?;
    let relative = format!(
        ".lenso/module-management/effect-evidence/{}.json",
        effect_digest.trim_start_matches("sha256:")
    );
    let path = workspace_root.join(&relative);
    if let Some(existing) = existing_effect_receipt(workspace_root, effect)? {
        return Ok(existing);
    }
    let document = serde_json::to_vec_pretty(&json!({
        "protocol": "lenso.module-effect-evidence.v1",
        "operationId": operation.operation_id,
        "attempt": operation.attempt,
        "effectId": effect_id,
        "effectDigest": effect_digest,
        "outcome": outcome,
    }))
    .map_err(|error| failed(effect_id, error))?;
    let parent = path
        .parent()
        .ok_or_else(|| failed(effect_id, "receipt path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| failed(effect_id, error))?;
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&temporary, &document).map_err(|error| failed(effect_id, error))?;
    fs::rename(&temporary, &path).map_err(|error| failed(effect_id, error))?;
    Ok(lenso_contracts::ArtifactReference {
        locator: relative,
        digest: sha256(&document),
    })
}

fn existing_effect_receipt(
    workspace_root: &Path,
    effect: &ModulePlanEffect,
) -> Result<Option<lenso_contracts::ArtifactReference>, ModuleEffectAdapterError> {
    let effect_id = effect.effect_id();
    let effect_digest =
        lenso_contracts::digest_json(effect).map_err(|error| failed(effect_id, error))?;
    let relative = format!(
        ".lenso/module-management/effect-evidence/{}.json",
        effect_digest.trim_start_matches("sha256:")
    );
    let path = workspace_root.join(&relative);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(effect_id, error)),
    };
    let document: Value =
        serde_json::from_slice(&bytes).map_err(|error| failed(effect_id, error))?;
    if document.get("protocol").and_then(Value::as_str) != Some("lenso.module-effect-evidence.v1")
        || document.get("effectDigest").and_then(Value::as_str) != Some(&effect_digest)
        || document.get("outcome").and_then(Value::as_str).is_none()
    {
        return Err(failed(
            effect_id,
            "persisted effect evidence does not match the reviewed effect",
        ));
    }
    Ok(Some(lenso_contracts::ArtifactReference {
        locator: relative,
        digest: sha256(&bytes),
    }))
}

fn management_actor(admin: AdminActor) -> ManagementActor {
    match admin {
        AdminActor::Service { service_id, scopes } => ManagementActor {
            actor_id: format!("service:{service_id}"),
            verified_authorities: scopes.into_iter().collect(),
        },
        AdminActor::User { user_id, scopes } => ManagementActor {
            actor_id: format!("user:{user_id}"),
            verified_authorities: scopes.into_iter().collect(),
        },
        AdminActor::System => ManagementActor {
            actor_id: "system".to_owned(),
            verified_authorities: BTreeSet::from([
                "module.manage".to_owned(),
                "module.migrate.destructive".to_owned(),
                "module.data.delete".to_owned(),
                "module.trust.override".to_owned(),
                "service.manage".to_owned(),
            ]),
        },
    }
}

fn apply_service_installation(
    workspace_root: &Path,
    operation: &lenso_module_management::ModuleOperation,
    effect: &ModulePlanEffect,
    plan: &ServiceInstallationPlan,
) -> Result<lenso_contracts::ArtifactReference, ModuleEffectAdapterError> {
    let effect_id = effect.effect_id();
    let effect_digest =
        lenso_contracts::digest_json(effect).map_err(|error| failed(effect_id, error))?;
    let service_operation_id = format!(
        "{}-{}",
        operation.operation_id,
        &effect_digest.trim_start_matches("sha256:")[..16]
    );
    let authorities = operation
        .verified_authorities
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    WorkspaceServiceInstallationManager::new(workspace_root, &plan.system_id, &plan.environment_id)
        .apply(
            &service_operation_id,
            plan,
            &operation.actor_id,
            &authorities,
            chrono::Utc::now(),
        )
        .map_err(|error| failed(effect_id, error))?;
    let relative = format!(
        ".lenso/environments/{}/service-install-receipts/{service_operation_id}.json",
        plan.environment_id
    );
    let bytes =
        fs::read(workspace_root.join(&relative)).map_err(|error| failed(effect_id, error))?;
    Ok(lenso_contracts::ArtifactReference {
        locator: relative,
        digest: sha256(&bytes),
    })
}

#[allow(clippy::result_large_err)]
fn management_root(
    context: &platform_core::RequestContext,
) -> Result<std::path::PathBuf, ApiErrorResponse> {
    std::env::current_dir().map_err(|error| {
        api_error(
            ErrorCode::Internal,
            format!("Module management root is unavailable: {error}"),
            context,
        )
    })
}
#[allow(clippy::result_large_err)]
fn decode<T: serde::de::DeserializeOwned>(
    body: Value,
    context: &platform_core::RequestContext,
) -> Result<T, ApiErrorResponse> {
    serde_json::from_value(body).map_err(|error| {
        api_error(
            ErrorCode::Validation,
            format!("Module operation request is invalid: {error}"),
            context,
        )
    })
}
#[allow(clippy::result_large_err)]
fn value<T: serde::Serialize>(
    body: T,
    context: &platform_core::RequestContext,
) -> Result<Json<Value>, ApiErrorResponse> {
    serde_json::to_value(body).map(Json).map_err(|error| {
        api_error(
            ErrorCode::Internal,
            format!("Module operation serialization failed: {error}"),
            context,
        )
    })
}
#[allow(clippy::needless_pass_by_value)]
fn operator_error(
    error: WorkspaceModuleOperatorError,
    context: &platform_core::RequestContext,
) -> ApiErrorResponse {
    let code = match &error {
        WorkspaceModuleOperatorError::Store(
            lenso_module_management::ModuleOperationStoreError::NotFound(_),
        ) => ErrorCode::NotFound,
        WorkspaceModuleOperatorError::Management(
            lenso_module_management::ModuleManagementError::MissingAuthority(_),
        ) => ErrorCode::Forbidden,
        WorkspaceModuleOperatorError::StalePlan
        | WorkspaceModuleOperatorError::CancellationUnsafe
        | WorkspaceModuleOperatorError::PolicyUnavailable
        | WorkspaceModuleOperatorError::Management(_)
        | WorkspaceModuleOperatorError::Store(_) => ErrorCode::Conflict,
        _ => ErrorCode::Internal,
    };
    api_error(code, format!("Module operation failed: {error}"), context)
}

#[allow(clippy::needless_pass_by_value)]
fn service_installation_error(
    error: lenso_module_management::ServiceInstallationError,
    context: &platform_core::RequestContext,
) -> ApiErrorResponse {
    let code = match error {
        lenso_module_management::ServiceInstallationError::InvalidContract(_)
        | lenso_module_management::ServiceInstallationError::UnsafeOperationIdentity
        | lenso_module_management::ServiceInstallationError::Json(_) => ErrorCode::Validation,
        lenso_module_management::ServiceInstallationError::MissingAuthority(_) => {
            ErrorCode::Forbidden
        }
        lenso_module_management::ServiceInstallationError::StaleState => ErrorCode::Conflict,
        lenso_module_management::ServiceInstallationError::Io(_) => ErrorCode::Internal,
    };
    api_error(
        code,
        format!("Service Installation operation failed: {error}"),
        context,
    )
}
fn failed(effect_id: &str, reason: impl std::fmt::Display) -> ModuleEffectAdapterError {
    ModuleEffectAdapterError::Failed {
        effect_id: effect_id.to_owned(),
        reason: reason.to_string(),
    }
}
fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut value = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

fn api_error(
    code: ErrorCode,
    message: String,
    context: &platform_core::RequestContext,
) -> ApiErrorResponse {
    ApiErrorResponse::with_context(AppError::new(code, message), context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_module_management::ServiceDeploymentAdapterKind;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "lenso-module-effect-{name}-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn effect(action: ServiceDeploymentAction) -> ModulePlanEffect {
        ModulePlanEffect::ServiceInstallation {
            effect_id: "service-install:test".to_owned(),
            service_id: "acme/support".to_owned(),
            service_release_digest: format!("sha256:{}", "a".repeat(64)),
            installation_plan: None,
            adapter: Some(ServiceDeploymentAdapterKind::Local),
            action: Some(action),
        }
    }

    #[test]
    fn evidence_action_rejects_digest_drift() {
        let root = root("evidence");
        fs::write(root.join("deployment.json"), b"observed").unwrap();
        let action = ServiceDeploymentAction::Evidence {
            receipt: lenso_contracts::ArtifactReference {
                locator: "deployment.json".to_owned(),
                digest: format!("sha256:{}", "0".repeat(64)),
            },
        };
        let error = execute_service_action(&root, &effect(action.clone()), &action).unwrap_err();
        assert!(error.to_string().contains("digest differs"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_action_uses_argv_without_a_shell() {
        let root = root("command");
        let action = ServiceDeploymentAction::Command {
            program: "rustc".to_owned(),
            args: vec!["--version".to_owned()],
            working_directory: None,
        };
        let reference = execute_service_action(&root, &effect(action.clone()), &action).unwrap();
        assert_eq!(reference.locator, "command:rustc");
        assert!(reference.digest.starts_with("sha256:"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_action_rejects_shell_programs() {
        let root = root("shell");
        let action = ServiceDeploymentAction::Command {
            program: "/bin/sh".to_owned(),
            args: vec!["-c".to_owned(), "exit 0".to_owned()],
            working_directory: None,
        };
        let error = execute_service_action(&root, &effect(action.clone()), &action).unwrap_err();
        assert!(error.to_string().contains("shell programs"));
        fs::remove_dir_all(root).unwrap();
    }
}
