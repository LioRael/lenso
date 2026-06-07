use crate::dto::{
    AdminActionInvocationDto, AdminActionInvokeRequest, AdminActionInvokeResponse,
    AdminCapabilityIssueDto, AdminCapabilitySummaryDto, AdminDataDetailResponse,
    AdminDataListResponse, AdminDataPageInfo, AdminModuleActivationState, AdminModuleGovernanceDto,
    AdminModuleMetadataDto, AdminModuleMetadataListResponse, AdminModuleRefreshModuleResultDto,
    AdminModuleRefreshModuleStatusDto, AdminModuleRefreshRecordDto, AdminModuleRefreshStatusDto,
    AdminModuleRegistrySnapshotCatalogDto, AdminModuleRegistrySnapshotIssueDto,
    AdminModuleRegistrySnapshotInstallPolicy, AdminModuleRegistrySnapshotManifestStatus,
    AdminModuleRegistrySnapshotModuleDto, AdminModuleRegistrySnapshotModuleStatus,
    AdminModuleRegistrySnapshotResponse, AdminModuleRegistrySnapshotStatus, AdminModuleSchema,
    AdminModuleSourceDiagnosticsDto, AdminModuleStatus, AdminRemoteModuleDiagnosticsDto,
    AdminSchemaListResponse, AdminSchemaRefreshResponse,
};
use crate::{
    AdminModule, AdminModuleMetadata, AdminModuleMetadataRefreshModuleResult,
    AdminModuleMetadataRefreshModuleStatus, AdminModuleMetadataRefreshRecord,
    AdminModuleMetadataRefreshStatus, AdminModuleSourceDiagnostics, admin_metadata_refresher,
    admin_module_metadata_snapshot, admin_modules, admin_refresher, find_loaded_action_module,
    find_loaded_module, install_admin_module_metadata, install_admin_modules,
    record_admin_module_metadata_refresh_error, record_admin_module_metadata_refresh_success,
};
use axum::Json;
use axum::extract::{Path, Query, State};
use platform_core::{
    AdminActionStoryRecord, AppContext, AppError, ErrorCode, RequestContext,
    admin_action_story_event_id, insert_admin_action_story_event,
};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};
use platform_module::{
    AdminActionConfirmation, AdminActionInputField, AdminActionInputSchema, AdminListQuery,
    AdminSurface, FieldType, ModuleLoadStatus, ModuleManifestLint, ModuleManifestLintSeverity,
    ModuleSource, lint_module_manifest_parts, module_capability_references,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::time::Instant;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub(crate) struct DataListQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/admin/data/modules",
    operation_id = "admin_data_list_modules",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "All module registry metadata", body = AdminModuleMetadataListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_modules(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleMetadataListResponse>, ApiErrorResponse> {
    Ok(Json(metadata_response(admin_module_metadata_snapshot())))
}

#[utoipa::path(
    post,
    path = "/admin/data/modules/refresh",
    operation_id = "admin_data_refresh_modules",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "Module registry metadata snapshot after refresh", body = AdminModuleMetadataListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Module registry refresh is unavailable", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn refresh_modules(
    _admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleMetadataListResponse>, ApiErrorResponse> {
    let refresher = admin_metadata_refresher().ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                "module registry refresh is unavailable",
            )
            .retryable(),
            &request_ctx,
        )
    })?;
    let started_at = crate::current_timestamp();
    let started = Instant::now();
    match refresher.refresh_admin_module_metadata().await {
        Ok(metadata) => Ok(Json(metadata_response(
            record_admin_module_metadata_refresh_success(metadata, started_at, started),
        ))),
        Err(error) => Ok(Json(metadata_response(
            record_admin_module_metadata_refresh_error(error.public_message, started_at, started),
        ))),
    }
}

#[utoipa::path(
    get,
    path = "/admin/data/module-registry/snapshot",
    operation_id = "admin_data_module_registry_snapshot",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "Read-only module registry preflight snapshot", body = AdminModuleRegistrySnapshotResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn module_registry_snapshot(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleRegistrySnapshotResponse>, ApiErrorResponse> {
    Ok(Json(module_registry_snapshot_response(
        admin_module_metadata_snapshot().modules,
    )))
}

#[utoipa::path(
    get,
    path = "/admin/data/schema",
    operation_id = "admin_data_list_schemas",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "All admin-capable modules' schemas", body = AdminSchemaListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_schemas(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminSchemaListResponse>, ApiErrorResponse> {
    Ok(Json(AdminSchemaListResponse {
        modules: schema_response_modules(admin_modules()),
    }))
}

#[utoipa::path(
    post,
    path = "/admin/data/schema/refresh",
    operation_id = "admin_data_refresh_schemas",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "Refreshed admin-capable modules' schemas", body = AdminSchemaRefreshResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Admin module refresh is unavailable or failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn refresh_schemas(
    _admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminSchemaRefreshResponse>, ApiErrorResponse> {
    let refresher = admin_refresher().ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                "admin module refresh is unavailable",
            )
            .retryable(),
            &request_ctx,
        )
    })?;
    let modules = refresher
        .refresh_admin_modules()
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    install_admin_modules(modules.clone());
    if let Some(metadata_refresher) = admin_metadata_refresher() {
        let metadata = metadata_refresher
            .refresh_admin_module_metadata()
            .await
            .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
        install_admin_module_metadata(metadata);
    }
    Ok(Json(AdminSchemaRefreshResponse {
        modules: schema_response_modules(modules),
    }))
}

fn metadata_response_modules(modules: Vec<AdminModuleMetadata>) -> Vec<AdminModuleMetadataDto> {
    modules
        .iter()
        .map(|m| {
            let manifest_lints = lint_module_manifest_parts(
                m.source,
                &m.module_name,
                m.admin.as_ref(),
                &m.http_routes,
                m.runtime.as_ref(),
                m.lifecycle.as_ref(),
                &m.console,
                &m.capabilities,
            );
            AdminModuleMetadataDto {
                module_name: m.module_name.clone(),
                source: m.source,
                status: admin_module_status(&m.load_status),
                error: load_error_message(&m.load_status),
                source_diagnostics: source_diagnostics_dto(m.source_diagnostics.clone()),
                http_routes: m.http_routes.clone(),
                runtime: m.runtime.clone(),
                lifecycle: m.lifecycle.clone(),
                console: m.console.clone(),
                governance: module_governance(m, &manifest_lints),
                manifest_lints,
                story_display: m
                    .story_display
                    .clone()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                capabilities: m.capabilities.clone(),
                admin: m.admin.clone(),
            }
        })
        .collect()
}

fn module_registry_snapshot_response(
    modules: Vec<AdminModuleMetadata>,
) -> AdminModuleRegistrySnapshotResponse {
    let modules = modules
        .into_iter()
        .filter(|module| matches!(module.source, ModuleSource::Remote))
        .map(module_registry_snapshot_module)
        .collect::<Vec<_>>();
    let issue_count = modules
        .iter()
        .filter(|module| {
            matches!(
                module.status,
                AdminModuleRegistrySnapshotModuleStatus::NeedsAttention
            )
        })
        .count();
    let issues = modules
        .iter()
        .filter(|module| {
            matches!(
                module.status,
                AdminModuleRegistrySnapshotModuleStatus::NeedsAttention
            )
        })
        .map(|module| AdminModuleRegistrySnapshotIssueDto {
            group: "Manifest".to_owned(),
            message: format!("{} remote module metadata needs attention", module.name),
            fix: "refresh module metadata and verify remote manifest configuration".to_owned(),
        })
        .collect::<Vec<_>>();

    AdminModuleRegistrySnapshotResponse {
        version: 1,
        status: if issue_count == 0 {
            AdminModuleRegistrySnapshotStatus::Passed
        } else {
            AdminModuleRegistrySnapshotStatus::Failed
        },
        catalog: AdminModuleRegistrySnapshotCatalogDto {
            modules: modules.len(),
            registry_file: "host-admin-module-metadata".to_owned(),
            version: 1,
        },
        issues,
        modules,
    }
}

fn module_registry_snapshot_module(
    module: AdminModuleMetadata,
) -> AdminModuleRegistrySnapshotModuleDto {
    let module_name = module.module_name;
    let remote = match module.source_diagnostics {
        Some(AdminModuleSourceDiagnostics::Remote(remote)) => Some(remote),
        None => None,
    };
    let manifest_reference = remote
        .as_ref()
        .map(|diagnostics| diagnostics.manifest_url.clone())
        .unwrap_or_else(|| "-".to_owned());
    let base_url = remote
        .as_ref()
        .map(|diagnostics| diagnostics.base_url.clone());
    let has_error = !matches!(module.load_status, ModuleLoadStatus::Loaded)
        || remote
            .as_ref()
            .and_then(|diagnostics| diagnostics.last_load_error.as_ref())
            .is_some();

    AdminModuleRegistrySnapshotModuleDto {
        name: module_name.clone(),
        source: module.source,
        catalog_version: "unknown".to_owned(),
        manifest_reference,
        base_url,
        console_package_hints: module.console.len(),
        install_policy: AdminModuleRegistrySnapshotInstallPolicy::Trusted,
        manifest_name: if has_error { None } else { Some(module_name) },
        manifest_status: if has_error {
            AdminModuleRegistrySnapshotManifestStatus::Unreadable
        } else {
            AdminModuleRegistrySnapshotManifestStatus::Ok
        },
        manifest_version: if has_error {
            None
        } else {
            Some("unknown".to_owned())
        },
        status: if has_error {
            AdminModuleRegistrySnapshotModuleStatus::NeedsAttention
        } else {
            AdminModuleRegistrySnapshotModuleStatus::Ready
        },
    }
}

fn source_diagnostics_dto(
    diagnostics: Option<AdminModuleSourceDiagnostics>,
) -> Option<AdminModuleSourceDiagnosticsDto> {
    match diagnostics? {
        AdminModuleSourceDiagnostics::Remote(remote) => Some(
            AdminModuleSourceDiagnosticsDto::Remote(AdminRemoteModuleDiagnosticsDto {
                base_url: remote.base_url,
                manifest_url: remote.manifest_url,
                timeout_ms: remote.timeout_ms,
                auth_configured: remote.auth_configured,
                load_duration_ms: remote.load_duration_ms,
                last_checked_at: remote.last_checked_at,
                last_load_error: remote.last_load_error,
            }),
        ),
    }
}

fn module_governance(
    module: &AdminModuleMetadata,
    manifest_lints: &[ModuleManifestLint],
) -> AdminModuleGovernanceDto {
    let references = module_capability_references(
        module.admin.as_ref(),
        &module.http_routes,
        module.lifecycle.as_ref(),
        &module.console,
    );
    let declared = module
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let referenced = references
        .iter()
        .map(|reference| reference.capability.as_str())
        .collect::<HashSet<_>>();
    let capability_issues = references
        .iter()
        .filter(|reference| !declared.contains(reference.capability.as_str()))
        .map(|reference| AdminCapabilityIssueDto {
            capability: reference.capability.clone(),
            subject: format!("capability.reference.{}", reference.subject),
            message: "Capability reference is not declared by the module.".to_owned(),
            suggestion: format!(
                "Add `{}` to ModuleManifest.capabilities or update the reference.",
                reference.capability
            ),
        })
        .collect::<Vec<_>>();
    let unused_count = declared
        .iter()
        .filter(|capability| !referenced.contains(**capability))
        .count();

    AdminModuleGovernanceDto {
        activation_state: module_activation_state(&module.load_status, manifest_lints),
        activation_reasons: module_activation_reasons(&module.load_status, manifest_lints),
        capability_summary: AdminCapabilitySummaryDto {
            declared_count: declared.len(),
            referenced_count: referenced.len(),
            missing_count: capability_issues.len(),
            unused_count,
        },
        capability_issues,
    }
}

fn module_activation_state(
    load_status: &ModuleLoadStatus,
    manifest_lints: &[ModuleManifestLint],
) -> AdminModuleActivationState {
    if matches!(load_status, ModuleLoadStatus::Error { .. })
        || manifest_lints
            .iter()
            .any(|lint| lint.severity == ModuleManifestLintSeverity::Error)
    {
        return AdminModuleActivationState::Blocked;
    }
    if manifest_lints
        .iter()
        .any(|lint| lint.severity == ModuleManifestLintSeverity::Warning)
    {
        return AdminModuleActivationState::NeedsAttention;
    }
    AdminModuleActivationState::Active
}

fn module_activation_reasons(
    load_status: &ModuleLoadStatus,
    manifest_lints: &[ModuleManifestLint],
) -> Vec<String> {
    let state = module_activation_state(load_status, manifest_lints);
    let mut reasons = Vec::new();
    if let ModuleLoadStatus::Error { message } = load_status {
        reasons.push(format!("module failed to load: {message}"));
    }
    let severity = match state {
        AdminModuleActivationState::Blocked => ModuleManifestLintSeverity::Error,
        AdminModuleActivationState::NeedsAttention => ModuleManifestLintSeverity::Warning,
        AdminModuleActivationState::Active => return reasons,
    };
    reasons.extend(
        manifest_lints
            .iter()
            .filter(|lint| lint.severity == severity)
            .map(|lint| format!("{}: {}", lint.subject, lint.message)),
    );
    reasons
}

fn metadata_response(
    snapshot: crate::AdminModuleMetadataSnapshot,
) -> AdminModuleMetadataListResponse {
    AdminModuleMetadataListResponse {
        modules: metadata_response_modules(snapshot.modules),
        refreshed_at: snapshot.refreshed_at,
        refresh_error: snapshot.refresh_error,
        refresh_history: snapshot
            .refresh_history
            .into_iter()
            .map(refresh_record_dto)
            .collect(),
    }
}

fn refresh_record_dto(record: AdminModuleMetadataRefreshRecord) -> AdminModuleRefreshRecordDto {
    AdminModuleRefreshRecordDto {
        id: record.id,
        status: match record.status {
            AdminModuleMetadataRefreshStatus::Success => AdminModuleRefreshStatusDto::Success,
            AdminModuleMetadataRefreshStatus::Error => AdminModuleRefreshStatusDto::Error,
        },
        started_at: record.started_at,
        completed_at: record.completed_at,
        duration_ms: record.duration_ms,
        module_count: record.module_count,
        error: record.error,
        module_results: record
            .module_results
            .into_iter()
            .map(refresh_module_result_dto)
            .collect(),
    }
}

fn refresh_module_result_dto(
    result: AdminModuleMetadataRefreshModuleResult,
) -> AdminModuleRefreshModuleResultDto {
    AdminModuleRefreshModuleResultDto {
        module_name: result.module_name,
        source: result.source,
        status: match result.status {
            AdminModuleMetadataRefreshModuleStatus::Loaded => {
                AdminModuleRefreshModuleStatusDto::Loaded
            }
            AdminModuleMetadataRefreshModuleStatus::Error => {
                AdminModuleRefreshModuleStatusDto::Error
            }
        },
        duration_ms: result.duration_ms,
        endpoint: result.endpoint,
        error: result.error,
    }
}

fn schema_response_modules(modules: Vec<AdminModule>) -> Vec<AdminModuleSchema> {
    modules
        .iter()
        .filter(|m| m.listed_in_schema)
        .map(|m| AdminModuleSchema {
            module_name: m.module_name.clone(),
            source: m.source,
            status: admin_module_status(&m.load_status),
            error: load_error_message(&m.load_status),
            schema: m.schema.clone(),
        })
        .collect()
}

fn admin_module_status(status: &ModuleLoadStatus) -> AdminModuleStatus {
    match status {
        ModuleLoadStatus::Loaded => AdminModuleStatus::Loaded,
        ModuleLoadStatus::Error { .. } => AdminModuleStatus::Error,
    }
}

fn load_error_message(status: &ModuleLoadStatus) -> Option<String> {
    match status {
        ModuleLoadStatus::Loaded => None,
        ModuleLoadStatus::Error { message } => Some(message.clone()),
    }
}

#[utoipa::path(
    post,
    path = "/admin/data/{module}/actions/{action}",
    operation_id = "admin_data_invoke_action",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Module name, e.g. remote-crm"),
        ("action" = String, Path, description = "Declared admin action name"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    request_body = AdminActionInvokeRequest,
    responses(
        (status = 200, description = "Action result", body = AdminActionInvokeResponse, content_type = "application/json"),
        (status = 400, description = "Request validation failed", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module or undeclared action", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Action source is unavailable or failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn invoke_action(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, action)): Path<(String, String)>,
    Json(request): Json<AdminActionInvokeRequest>,
) -> Result<Json<AdminActionInvokeResponse>, ApiErrorResponse> {
    let started_at = ctx.clock.now();
    let started = Instant::now();
    let admin_module = find_loaded_action_module(&module, &request_ctx)?;
    let declaration = declared_action(&admin_module, &action, &request_ctx)?;
    ensure_action_capability(&admin, &declaration.capability, &request_ctx)?;
    ensure_action_confirmation(&request, &declaration, &request_ctx)?;
    ensure_action_input(&request, &declaration, &request_ctx)?;
    let action_source = admin_module.action_source.as_ref().ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("module {module} has no admin action source"),
            )
            .retryable(),
            &request_ctx,
        )
    })?;
    let input = request.input;
    let result = action_source.invoke(&action, input.clone()).await;
    match result {
        Ok(data) => {
            record_admin_action_story(
                &ctx,
                &request_ctx,
                AdminActionStoryRecord {
                    action_name: action,
                    capability: declaration.capability,
                    duration_ms: elapsed_ms(started),
                    error_code: None,
                    error_message: None,
                    input,
                    label: declaration.label,
                    module_name: module,
                    result: Some(data.clone()),
                    started_at,
                    success: true,
                },
            )
            .await;

            Ok(Json(AdminActionInvokeResponse {
                data,
                invocation: action_invocation_dto(&request_ctx),
            }))
        }
        Err(error) => {
            let error_code = error.code.as_str().to_owned();
            let error_message = error.public_message.clone();
            record_admin_action_story(
                &ctx,
                &request_ctx,
                AdminActionStoryRecord {
                    action_name: action,
                    capability: declaration.capability,
                    duration_ms: elapsed_ms(started),
                    error_code: Some(error_code),
                    error_message: Some(error_message),
                    input,
                    label: declaration.label,
                    module_name: module,
                    result: None,
                    started_at,
                    success: false,
                },
            )
            .await;
            Err(ApiErrorResponse::with_context(error, &request_ctx))
        }
    }
}

#[derive(Debug, Clone)]
struct DeclaredAction {
    label: String,
    capability: String,
    input_schema: Option<AdminActionInputSchema>,
    confirmation: Option<AdminActionConfirmation>,
}

fn declared_action(
    module: &AdminModule,
    action: &str,
    ctx: &RequestContext,
) -> Result<DeclaredAction, ApiErrorResponse> {
    let Some(AdminSurface::DeclarativeCustom(surface)) = module.admin.as_ref() else {
        return Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown action: {action}")),
            ctx,
        ));
    };

    surface
        .actions
        .iter()
        .find(|declared| declared.name == action)
        .map(|declared| DeclaredAction {
            capability: declared.capability.clone(),
            confirmation: declared.confirmation.clone(),
            input_schema: declared.input_schema.clone(),
            label: declared.label.clone(),
        })
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::NotFound, format!("unknown action: {action}")),
                ctx,
            )
        })
}

fn ensure_action_input(
    request: &AdminActionInvokeRequest,
    declaration: &DeclaredAction,
    ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    let Some(input_schema) = declaration.input_schema.as_ref() else {
        return Ok(());
    };
    if input_schema.fields.is_empty() {
        return Ok(());
    }

    let Some(input) = request.input.as_object() else {
        return Err(input_validation_error(
            "admin action input must be a JSON object",
            ctx,
        ));
    };

    let declared_fields = input_schema
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<HashSet<_>>();
    if let Some(field_name) = input
        .keys()
        .find(|field_name| !declared_fields.contains(field_name.as_str()))
    {
        return Err(input_validation_error(
            format!("admin action input field `{field_name}` is not declared"),
            ctx,
        ));
    }

    for field in &input_schema.fields {
        let value = input.get(&field.name);
        if field.required && matches!(value, None | Some(Value::Null)) {
            return Err(input_validation_error(
                format!("admin action input field `{}` is required", field.name),
                ctx,
            ));
        }
        let Some(value) = value.filter(|value| !value.is_null()) else {
            continue;
        };
        if !action_input_field_type_matches(field, value) {
            return Err(input_validation_error(
                format!(
                    "admin action input field `{}` must be {}",
                    field.name,
                    action_input_field_type_label(&field.field_type)
                ),
                ctx,
            ));
        }
    }

    Ok(())
}

fn action_input_field_type_matches(field: &AdminActionInputField, value: &Value) -> bool {
    match &field.field_type {
        FieldType::String | FieldType::Timestamp => value.is_string(),
        FieldType::Integer => value
            .as_number()
            .is_some_and(|number| number.as_i64().is_some() || number.as_u64().is_some()),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Json => true,
        _ => true,
    }
}

fn action_input_field_type_label(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::String => "a string",
        FieldType::Integer => "an integer",
        FieldType::Boolean => "a boolean",
        FieldType::Timestamp => "a timestamp string",
        FieldType::Json => "valid JSON",
        _ => "the declared type",
    }
}

fn input_validation_error(message: impl Into<String>, ctx: &RequestContext) -> ApiErrorResponse {
    ApiErrorResponse::with_context(AppError::new(ErrorCode::Validation, message), ctx)
}

fn action_invocation_dto(request_ctx: &RequestContext) -> AdminActionInvocationDto {
    AdminActionInvocationDto {
        request_id: request_ctx.request_id.0.clone(),
        correlation_id: request_ctx.correlation_id.0.clone(),
        story_node_id: admin_action_story_event_id(request_ctx),
    }
}

fn ensure_action_confirmation(
    request: &AdminActionInvokeRequest,
    declaration: &DeclaredAction,
    ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    let Some(required_phrase) = declaration
        .confirmation
        .as_ref()
        .and_then(|confirmation| confirmation.required_phrase.as_deref())
        .filter(|phrase| !phrase.is_empty())
    else {
        return Ok(());
    };

    if request.confirmation_phrase.as_deref() == Some(required_phrase) {
        return Ok(());
    }

    Err(ApiErrorResponse::with_context(
        AppError::new(
            ErrorCode::Validation,
            "admin action confirmation phrase did not match",
        ),
        ctx,
    ))
}

fn ensure_action_capability(
    admin: &AdminActor,
    capability: &str,
    ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    match admin {
        AdminActor::System => Ok(()),
        AdminActor::Service { scopes, .. } if scopes.iter().any(|scope| scope == capability) => {
            Ok(())
        }
        AdminActor::Service { .. } => Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                format!("missing admin action capability: {capability}"),
            ),
            ctx,
        )),
    }
}

async fn record_admin_action_story(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    record: AdminActionStoryRecord,
) {
    let module_name = record.module_name.clone();
    let action_name = record.action_name.clone();
    let success = record.success;
    if let Err(error) = insert_admin_action_story_event(&ctx.db, request_ctx, record).await {
        tracing::warn!(
            error = ?error,
            module_name = %module_name,
            action_name = %action_name,
            success,
            request_id = %request_ctx.request_id.0,
            correlation_id = %request_ctx.correlation_id.0,
            "failed to persist admin action story event"
        );
    }
}

fn elapsed_ms(started: Instant) -> i64 {
    started.elapsed().as_millis().min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{
        AdminSchema, AdminSurface, ConsoleArea, ConsolePackage, ConsoleSurface, EntitySchema,
        LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy, LifecycleSurface,
        ModuleHttpMethod, ModuleHttpRoute, ModuleSource,
    };

    #[test]
    fn metadata_response_includes_manifest_lints() {
        let modules = metadata_response_modules(vec![AdminModuleMetadata {
            module_name: "remote-crm".to_owned(),
            source: ModuleSource::Remote,
            load_status: ModuleLoadStatus::Loaded,
            http_routes: vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/contacts/{id}".to_owned(),
                capability: None,
                display_name: Some("Fetch Contact".to_owned()),
                story_title: Some("Fetch Contact".to_owned()),
            }],
            runtime: None,
            lifecycle: Some(LifecycleSurface {
                startup_checks: Vec::new(),
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "warm contact cache".to_owned(),
                    function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: serde_json::json!({}),
                    required: true,
                }],
            }),
            console: Vec::new(),
            story_display: Vec::new(),
            capabilities: Vec::new(),
            admin: None,
            source_diagnostics: None,
        }]);

        assert!(modules[0].lifecycle.is_some());
        assert!(modules[0].manifest_lints.iter().any(|lint| {
            lint.subject == "lifecycle.activation_job.warm contact cache"
                && lint.message
                    == "Lifecycle activation job references an unknown runtime function."
        }));
        assert!(modules[0].manifest_lints.iter().any(|lint| {
            lint.message == "Missing capability declaration for host proxy authorization."
        }));
    }

    #[test]
    fn metadata_response_includes_module_governance() {
        let modules = metadata_response_modules(vec![AdminModuleMetadata {
            module_name: "remote-crm".to_owned(),
            source: ModuleSource::Remote,
            load_status: ModuleLoadStatus::Loaded,
            http_routes: vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/contacts/{id}".to_owned(),
                capability: Some("remote_crm.contacts.read".to_owned()),
                display_name: Some("Fetch Contact".to_owned()),
                story_title: Some("Fetch Contact".to_owned()),
            }],
            runtime: None,
            lifecycle: None,
            console: vec![ConsoleSurface {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                area: ConsoleArea::Data,
                route: "/data/contacts".to_owned(),
                package: ConsolePackage {
                    name: "@lenso/contacts-console".to_owned(),
                    export: "contactsConsoleModule".to_owned(),
                },
                icon: None,
                required_capabilities: vec!["remote_crm.contacts.read".to_owned()],
                navigation: None,
            }],
            story_display: Vec::new(),
            capabilities: vec!["remote_crm.contacts.write".to_owned()],
            admin: Some(AdminSurface::Schema(AdminSchema {
                entities: vec![EntitySchema {
                    name: "contacts".to_owned(),
                    label: "Contacts".to_owned(),
                    fields: vec![],
                    read_capability: "remote_crm.contacts.read".to_owned(),
                }],
            })),
            source_diagnostics: None,
        }]);

        let governance = &modules[0].governance;

        assert_eq!(
            governance.activation_state,
            AdminModuleActivationState::NeedsAttention
        );
        assert_eq!(governance.capability_summary.declared_count, 1);
        assert_eq!(governance.capability_summary.referenced_count, 1);
        assert_eq!(governance.capability_summary.missing_count, 3);
        assert_eq!(governance.capability_summary.unused_count, 1);
        assert_eq!(
            governance.capability_issues[0].capability,
            "remote_crm.contacts.read"
        );
        assert_eq!(
            governance.capability_issues[0].subject,
            "capability.reference.http_route.GET /contacts/{id}"
        );
        assert!(
            governance
                .capability_issues
                .iter()
                .any(|issue| { issue.subject == "capability.reference.console.surface.contacts" })
        );
        assert_eq!(modules[0].console.len(), 1);
    }
}

#[utoipa::path(
    get,
    path = "/admin/data/{module}/{entity}",
    operation_id = "admin_data_list_records",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Module name, e.g. identity"),
        ("entity" = String, Path, description = "Entity name, e.g. users"),
        ("limit" = Option<i64>, Query, description = "Max records (default 50, max 200)"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "A page of records", body = AdminDataListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module or entity", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_records(
    _admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, entity)): Path<(String, String)>,
    Query(query): Query<DataListQuery>,
) -> Result<Json<AdminDataListResponse>, ApiErrorResponse> {
    let admin_module = find_loaded_module(&module, &request_ctx)?;
    ensure_entity(&admin_module, &entity, &request_ctx)?;

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let page = admin_module
        .data_source
        .as_ref()
        .expect("loaded admin module has data source")
        .list(&entity, &AdminListQuery::new(limit, query.cursor))
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(Json(AdminDataListResponse {
        data: page.records,
        page: AdminDataPageInfo {
            limit,
            next_cursor: page.next_cursor,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/admin/data/{module}/{entity}/{id}",
    operation_id = "admin_data_get_record",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Module name"),
        ("entity" = String, Path, description = "Entity name"),
        ("id" = String, Path, description = "Record id"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "One record", body = AdminDataDetailResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module/entity or record not found", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn get_record(
    _admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, entity, id)): Path<(String, String, String)>,
) -> Result<Json<AdminDataDetailResponse>, ApiErrorResponse> {
    let admin_module = find_loaded_module(&module, &request_ctx)?;
    ensure_entity(&admin_module, &entity, &request_ctx)?;

    match admin_module
        .data_source
        .as_ref()
        .expect("loaded admin module has data source")
        .get(&entity, &id)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?
    {
        Some(data) => Ok(Json(AdminDataDetailResponse { data })),
        None => Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, "record not found"),
            &request_ctx,
        )),
    }
}

fn ensure_entity(
    module: &AdminModule,
    entity: &str,
    ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    if module.schema.entities.iter().any(|e| e.name == entity) {
        Ok(())
    } else {
        Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown entity: {entity}")),
            ctx,
        ))
    }
}
