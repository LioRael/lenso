use crate::dto::{
    AdminActionInvokeRequest, AdminActionInvokeResponse, AdminCapabilityIssueDto,
    AdminCapabilitySummaryDto, AdminDataDetailResponse, AdminDataListResponse, AdminDataPageInfo,
    AdminModuleActivationState, AdminModuleGovernanceDto, AdminModuleMetadataDto,
    AdminModuleMetadataListResponse, AdminModuleRefreshModuleResultDto,
    AdminModuleRefreshModuleStatusDto, AdminModuleRefreshRecordDto, AdminModuleRefreshStatusDto,
    AdminModuleSchema, AdminModuleSourceDiagnosticsDto, AdminModuleStatus,
    AdminRemoteModuleDiagnosticsDto, AdminSchemaListResponse, AdminSchemaRefreshResponse,
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
use axum::extract::{Path, Query};
use platform_core::{AppError, ErrorCode, RequestContext};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};
use platform_module::{
    AdminListQuery, AdminSurface, ModuleLoadStatus, ModuleManifestLint, ModuleManifestLintSeverity,
    lint_module_manifest_parts, module_capability_references,
};
use serde::Deserialize;
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
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module or undeclared action", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Action source is unavailable or failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn invoke_action(
    _admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, action)): Path<(String, String)>,
    Json(request): Json<AdminActionInvokeRequest>,
) -> Result<Json<AdminActionInvokeResponse>, ApiErrorResponse> {
    let admin_module = find_loaded_action_module(&module, &request_ctx)?;
    ensure_declared_action(&admin_module, &action, &request_ctx)?;
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
    let data = action_source
        .invoke(&action, request.input)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(Json(AdminActionInvokeResponse { data }))
}

fn ensure_declared_action(
    module: &AdminModule,
    action: &str,
    ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    if matches!(
        module.admin.as_ref(),
        Some(AdminSurface::DeclarativeCustom(surface))
            if surface.actions.iter().any(|declared| declared.name == action)
    ) {
        Ok(())
    } else {
        Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown action: {action}")),
            ctx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{
        AdminSchema, AdminSurface, EntitySchema, LifecycleActivationJobDeclaration,
        LifecycleActivationRunPolicy, LifecycleSurface, ModuleHttpMethod, ModuleHttpRoute,
        ModuleSource,
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
        assert_eq!(governance.capability_summary.missing_count, 2);
        assert_eq!(governance.capability_summary.unused_count, 1);
        assert_eq!(
            governance.capability_issues[0].capability,
            "remote_crm.contacts.read"
        );
        assert_eq!(
            governance.capability_issues[0].subject,
            "capability.reference.http_route.GET /contacts/{id}"
        );
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
