use crate::dto::{
    AdminActionInvocationDto, AdminActionInvokeRequest, AdminActionInvokeResponse,
    AdminCapabilityIssueDto, AdminCapabilitySummaryDto, AdminDataDetailResponse,
    AdminDataListResponse, AdminDataPageInfo, AdminModuleActivationState,
    AdminModuleCompatibilityDto, AdminModuleConsolePackagePlanPackageDto,
    AdminModuleConsolePackagePlanStateDto, AdminModuleGovernanceDto,
    AdminModuleHostCompatibilityDto, AdminModuleInstallStateDto, AdminModuleMetadataDto,
    AdminModuleMetadataListResponse, AdminModuleRefreshModuleResultDto,
    AdminModuleRefreshModuleStatusDto, AdminModuleRefreshRecordDto, AdminModuleRefreshStatusDto,
    AdminModuleRegistrySnapshotCatalogDto, AdminModuleRegistrySnapshotIssueDto,
    AdminModuleRegistrySnapshotManifestStatus, AdminModuleRegistrySnapshotModuleDto,
    AdminModuleRegistrySnapshotModuleStatus, AdminModuleRegistrySnapshotResponse,
    AdminModuleRegistrySnapshotStatus, AdminModuleRemoteSourceInstallStateDto, AdminModuleSchema,
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
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::time::Instant;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const HOST_CONSOLE_PACKAGE_API_VERSION: &str = "1";
const HOST_LENSO_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        (status = 200, description = "Legacy alias for available remote modules", body = AdminModuleRegistrySnapshotResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn module_registry_snapshot(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleRegistrySnapshotResponse>, ApiErrorResponse> {
    Ok(Json(available_modules_response()))
}

#[utoipa::path(
    get,
    path = "/admin/data/available-modules",
    operation_id = "admin_data_available_modules",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "Available remote modules for marketplace install", body = AdminModuleRegistrySnapshotResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn available_modules(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleRegistrySnapshotResponse>, ApiErrorResponse> {
    Ok(Json(available_modules_response()))
}

fn available_modules_response() -> AdminModuleRegistrySnapshotResponse {
    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        PathBuf::from(".env"),
        PathBuf::from(".lenso/console-package-install-plan.json"),
    );
    match module_catalog_response(PathBuf::from(".lenso/module-catalog.json"), &install_state) {
        Some(response) => response,
        None => module_registry_snapshot_response(metadata, &install_state),
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
                m.events.as_ref(),
                m.lifecycle.as_ref(),
                &m.console,
                &m.capabilities,
                &m.dependencies,
            );
            AdminModuleMetadataDto {
                module_name: m.module_name.clone(),
                source: m.source,
                status: admin_module_status(&m.load_status),
                error: load_error_message(&m.load_status),
                source_diagnostics: source_diagnostics_dto(m.source_diagnostics.clone()),
                http_routes: m.http_routes.clone(),
                runtime: m.runtime.clone(),
                events: m.events.clone(),
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
                dependencies: m.dependencies.clone(),
                admin: m.admin.clone(),
            }
        })
        .collect()
}

fn module_registry_snapshot_response(
    modules: Vec<AdminModuleMetadata>,
    install_state: &AvailableModuleInstallStateContext,
) -> AdminModuleRegistrySnapshotResponse {
    let modules = modules
        .into_iter()
        .filter(|module| matches!(module.source, ModuleSource::Remote))
        .map(|module| module_registry_snapshot_module(module, install_state))
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleCatalog {
    #[serde(default)]
    modules: Vec<LocalModuleCatalogEntry>,
    #[serde(default = "default_module_catalog_version")]
    version: u8,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleCatalogEntry {
    name: String,
    version: String,
    source: String,
    manifest_reference: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    console_packages: Vec<LocalModuleCatalogConsolePackage>,
    #[serde(default)]
    compatibility: Option<AdminModuleCompatibilityDto>,
    #[serde(default)]
    archived_at: Option<String>,
    #[serde(default)]
    archive_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleCatalogConsolePackage {
    #[serde(rename = "packageName")]
    _package_name: String,
    #[serde(rename = "exportName")]
    _export_name: String,
    #[serde(default)]
    #[serde(rename = "route")]
    _route: Option<String>,
}

#[derive(Debug)]
struct AvailableModuleInstallStateContext {
    console_plan: LocalConsolePackageInstallPlanState,
    registered_modules: HashSet<String>,
    remote_sources: LocalRemoteModulesEnvState,
    running_base_urls: HashMap<String, String>,
}

#[derive(Debug)]
struct LocalRemoteModulesEnvState {
    env_file: String,
    error: Option<String>,
    modules: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalConsolePackageInstallPlan {
    #[serde(default)]
    modules: Vec<LocalConsolePackageInstallPlanModule>,
    #[serde(default = "default_console_package_install_plan_version")]
    _version: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalConsolePackageInstallPlanModule {
    #[serde(default)]
    _base_url: Option<String>,
    #[serde(default)]
    console_packages: Vec<LocalConsolePackageInstallPlanPackage>,
    #[serde(default)]
    _manifest_reference: Option<String>,
    module_name: String,
    #[serde(default)]
    restart_required: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalConsolePackageInstallPlanPackage {
    #[serde(default)]
    command: Option<String>,
    export_name: String,
    #[serde(default)]
    key: Option<String>,
    package_name: String,
    #[serde(default)]
    _requested_by_module: Option<String>,
    #[serde(default)]
    route: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug)]
struct LocalConsolePackageInstallPlanState {
    error: Option<String>,
    exists: bool,
    modules: HashMap<String, LocalConsolePackageInstallPlanModule>,
    plan_file: String,
}

const fn default_module_catalog_version() -> u8 {
    1
}

const fn default_console_package_install_plan_version() -> u8 {
    1
}

impl AvailableModuleInstallStateContext {
    fn from_paths(
        metadata: &[AdminModuleMetadata],
        env_file_path: impl AsRef<FsPath>,
        console_plan_file_path: impl AsRef<FsPath>,
    ) -> Self {
        Self {
            console_plan: local_console_package_install_plan_state(console_plan_file_path),
            registered_modules: metadata
                .iter()
                .map(|module| module.module_name.clone())
                .collect(),
            remote_sources: local_remote_modules_env_state(env_file_path),
            running_base_urls: metadata
                .iter()
                .filter_map(|module| {
                    let Some(AdminModuleSourceDiagnostics::Remote(remote)) =
                        module.source_diagnostics.as_ref()
                    else {
                        return None;
                    };
                    Some((
                        module.module_name.clone(),
                        normalize_remote_base_url(&remote.base_url),
                    ))
                })
                .collect(),
        }
    }

    fn install_state(&self, module_name: &str) -> AdminModuleInstallStateDto {
        AdminModuleInstallStateDto {
            module_registered: self.registered_modules.contains(module_name),
            remote_source: self.remote_source_state(module_name),
            console_plan: self.console_plan_state(module_name),
        }
    }

    fn remote_source_state(&self, module_name: &str) -> AdminModuleRemoteSourceInstallStateDto {
        let desired_base_url = self.remote_sources.modules.get(module_name).cloned();
        let running_base_url = self.running_base_urls.get(module_name).cloned();
        let restart_reason =
            remote_source_restart_reason(desired_base_url.as_deref(), running_base_url.as_deref());

        AdminModuleRemoteSourceInstallStateDto {
            env_file: self.remote_sources.env_file.clone(),
            configured: desired_base_url.is_some(),
            desired_base_url,
            running_base_url,
            restart_pending: restart_reason.is_some(),
            restart_reason,
            error: self.remote_sources.error.clone(),
        }
    }

    fn console_plan_state(&self, module_name: &str) -> AdminModuleConsolePackagePlanStateDto {
        let module_plan = self.console_plan.modules.get(module_name);
        let packages = module_plan
            .map(|module_plan| {
                module_plan
                    .console_packages
                    .iter()
                    .map(|package| AdminModuleConsolePackagePlanPackageDto {
                        key: package.key.clone(),
                        package_name: package.package_name.clone(),
                        export_name: package.export_name.clone(),
                        command: package.command.clone(),
                        route: package.route.clone(),
                        status: package.status.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        AdminModuleConsolePackagePlanStateDto {
            plan_file: self.console_plan.plan_file.clone(),
            exists: self.console_plan.exists,
            readable: self.console_plan.exists && self.console_plan.error.is_none(),
            error: self.console_plan.error.clone(),
            module_entry_present: module_plan.is_some(),
            package_count: packages.len(),
            restart_required: module_plan.and_then(|module_plan| module_plan.restart_required),
            packages,
        }
    }
}

fn local_remote_modules_env_state(env_file_path: impl AsRef<FsPath>) -> LocalRemoteModulesEnvState {
    let env_file_path = env_file_path.as_ref();
    let env_file = env_file_path.display().to_string();
    let source = match fs::read_to_string(env_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LocalRemoteModulesEnvState {
                env_file,
                error: None,
                modules: HashMap::new(),
            };
        }
        Err(error) => {
            return LocalRemoteModulesEnvState {
                env_file,
                error: Some(format!("remote module env file could not be read: {error}")),
                modules: HashMap::new(),
            };
        }
    };
    LocalRemoteModulesEnvState {
        env_file,
        error: None,
        modules: parse_remote_modules_env_source(&source),
    }
}

fn parse_remote_modules_env_source(source: &str) -> HashMap<String, String> {
    source
        .lines()
        .filter_map(remote_modules_env_value)
        .last()
        .map(parse_remote_modules_value)
        .unwrap_or_default()
}

fn remote_modules_env_value(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, value) = line.split_once('=')?;
    (key.trim() == "REMOTE_MODULES").then(|| unquote_env_value(value.trim()).to_owned())
}

fn parse_remote_modules_value(value: String) -> HashMap<String, String> {
    value
        .split(',')
        .filter_map(|entry| {
            let (name, base_url) = entry.trim().split_once('=')?;
            let name = name.trim();
            let base_url = normalize_remote_base_url(base_url);
            if name.is_empty() || base_url.is_empty() {
                return None;
            }
            Some((name.to_owned(), base_url))
        })
        .collect()
}

fn unquote_env_value(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn local_console_package_install_plan_state(
    console_plan_file_path: impl AsRef<FsPath>,
) -> LocalConsolePackageInstallPlanState {
    let console_plan_file_path = console_plan_file_path.as_ref();
    let plan_file = console_plan_file_path.display().to_string();
    let source = match fs::read_to_string(console_plan_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LocalConsolePackageInstallPlanState {
                error: None,
                exists: false,
                modules: HashMap::new(),
                plan_file,
            };
        }
        Err(error) => {
            return LocalConsolePackageInstallPlanState {
                error: Some(format!(
                    "console package install plan could not be read: {error}"
                )),
                exists: true,
                modules: HashMap::new(),
                plan_file,
            };
        }
    };
    match serde_json::from_str::<LocalConsolePackageInstallPlan>(&source) {
        Ok(plan) => LocalConsolePackageInstallPlanState {
            error: None,
            exists: true,
            modules: plan
                .modules
                .into_iter()
                .map(|module| (module.module_name.clone(), module))
                .collect(),
            plan_file,
        },
        Err(error) => LocalConsolePackageInstallPlanState {
            error: Some(format!(
                "console package install plan could not be parsed: {error}"
            )),
            exists: true,
            modules: HashMap::new(),
            plan_file,
        },
    }
}

fn remote_source_restart_reason(
    desired_base_url: Option<&str>,
    running_base_url: Option<&str>,
) -> Option<String> {
    match (desired_base_url, running_base_url) {
        (Some(_), None) => Some("remote source configured in .env but not loaded".to_owned()),
        (Some(desired), Some(running)) if desired != running => {
            Some("REMOTE_MODULES base URL differs from running module metadata".to_owned())
        }
        _ => None,
    }
}

fn normalize_remote_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_owned()
}

fn module_catalog_response(
    catalog_file_path: impl AsRef<FsPath>,
    install_state: &AvailableModuleInstallStateContext,
) -> Option<AdminModuleRegistrySnapshotResponse> {
    let catalog_file_path = catalog_file_path.as_ref();
    let source = match fs::read_to_string(catalog_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            return Some(module_catalog_error_response(
                catalog_file_path,
                format!("module catalog could not be read: {error}"),
            ));
        }
    };
    let catalog = match serde_json::from_str::<LocalModuleCatalog>(&source) {
        Ok(catalog) => catalog,
        Err(error) => {
            return Some(module_catalog_error_response(
                catalog_file_path,
                format!("module catalog could not be parsed: {error}"),
            ));
        }
    };
    let mut issues = Vec::new();
    let modules = catalog
        .modules
        .into_iter()
        .map(|entry| {
            let module = module_catalog_entry_module(entry, install_state);
            issues.extend(module_catalog_entry_issues(&module));
            module
        })
        .collect::<Vec<_>>();

    Some(AdminModuleRegistrySnapshotResponse {
        version: 1,
        status: if issues.is_empty() {
            AdminModuleRegistrySnapshotStatus::Passed
        } else {
            AdminModuleRegistrySnapshotStatus::Failed
        },
        catalog: AdminModuleRegistrySnapshotCatalogDto {
            modules: modules.len(),
            registry_file: catalog_file_path.display().to_string(),
            version: catalog.version,
        },
        issues,
        modules,
    })
}

fn module_catalog_error_response(
    catalog_file_path: &FsPath,
    message: String,
) -> AdminModuleRegistrySnapshotResponse {
    AdminModuleRegistrySnapshotResponse {
        version: 1,
        status: AdminModuleRegistrySnapshotStatus::Failed,
        catalog: AdminModuleRegistrySnapshotCatalogDto {
            modules: 0,
            registry_file: catalog_file_path.display().to_string(),
            version: 1,
        },
        issues: vec![AdminModuleRegistrySnapshotIssueDto {
            group: "Catalog".to_owned(),
            message,
            fix: "fix .lenso/module-catalog.json or remove it to use loaded remote modules"
                .to_owned(),
        }],
        modules: vec![],
    }
}

fn module_catalog_entry_module(
    entry: LocalModuleCatalogEntry,
    install_state: &AvailableModuleInstallStateContext,
) -> AdminModuleRegistrySnapshotModuleDto {
    let _source = entry.source;
    let archived = entry.archived_at.is_some();
    let name = entry.name;
    let needs_attention = !archived
        && (module_needs_base_url(entry.base_url.as_deref(), &entry.manifest_reference)
            || module_compatibility_issue(&name, entry.compatibility.as_ref()).is_some());

    AdminModuleRegistrySnapshotModuleDto {
        name: name.clone(),
        source: ModuleSource::Remote,
        catalog_version: entry.version.clone(),
        manifest_reference: entry.manifest_reference,
        summary: entry.summary,
        base_url: entry.base_url,
        capabilities: entry.capabilities,
        console_package_hints: entry.console_packages.len(),
        compatibility: entry.compatibility,
        host_compatibility: host_module_compatibility(),
        archived_at: entry.archived_at,
        archive_reason: entry.archive_reason,
        manifest_name: if archived { None } else { Some(name.clone()) },
        manifest_status: if archived {
            AdminModuleRegistrySnapshotManifestStatus::Archived
        } else {
            AdminModuleRegistrySnapshotManifestStatus::Ok
        },
        manifest_version: if archived { None } else { Some(entry.version) },
        install_state: install_state.install_state(&name),
        status: if archived {
            AdminModuleRegistrySnapshotModuleStatus::Archived
        } else if needs_attention {
            AdminModuleRegistrySnapshotModuleStatus::NeedsAttention
        } else {
            AdminModuleRegistrySnapshotModuleStatus::Ready
        },
    }
}

fn module_catalog_entry_issues(
    module: &AdminModuleRegistrySnapshotModuleDto,
) -> Vec<AdminModuleRegistrySnapshotIssueDto> {
    if matches!(
        module.status,
        AdminModuleRegistrySnapshotModuleStatus::Archived
    ) {
        return vec![];
    }

    let mut issues = Vec::new();
    if let Some(issue) = module_compatibility_issue(&module.name, module.compatibility.as_ref()) {
        issues.push(issue);
    }
    if module_needs_base_url(module.base_url.as_deref(), &module.manifest_reference) {
        issues.push(AdminModuleRegistrySnapshotIssueDto {
            group: "Catalog".to_owned(),
            message: format!("{} baseUrl is missing", module.name),
            fix: "add baseUrl or use a manifest URL ending with /manifest".to_owned(),
        });
    }
    issues
}

fn module_needs_base_url(base_url: Option<&str>, manifest_reference: &str) -> bool {
    base_url.is_none() && !is_http_manifest_reference(manifest_reference)
}

fn is_http_manifest_reference(manifest_reference: &str) -> bool {
    (manifest_reference.starts_with("http://") || manifest_reference.starts_with("https://"))
        && manifest_reference.ends_with("/manifest")
}

fn module_compatibility_issue(
    module_name: &str,
    compatibility: Option<&AdminModuleCompatibilityDto>,
) -> Option<AdminModuleRegistrySnapshotIssueDto> {
    let compatibility = compatibility?;
    if let Some(lenso) = compatibility.lenso.as_ref() {
        if let Some(min_version) = lenso.min_version.as_ref() {
            if !matches!(
                compare_versions(HOST_LENSO_VERSION, min_version),
                Some(Ordering::Equal | Ordering::Greater)
            ) {
                return Some(AdminModuleRegistrySnapshotIssueDto {
                    group: "Compatibility".to_owned(),
                    message: format!(
                        "{module_name} requires Lenso >= {min_version}; host is {HOST_LENSO_VERSION}"
                    ),
                    fix: format!(
                        "upgrade Lenso to {min_version} or install a compatible {module_name} catalog entry"
                    ),
                });
            }
        }
        if let Some(max_version) = lenso.max_version.as_ref() {
            if !matches!(
                compare_versions(HOST_LENSO_VERSION, max_version),
                Some(Ordering::Equal | Ordering::Less)
            ) {
                return Some(AdminModuleRegistrySnapshotIssueDto {
                    group: "Compatibility".to_owned(),
                    message: format!(
                        "{module_name} supports Lenso <= {max_version}; host is {HOST_LENSO_VERSION}"
                    ),
                    fix: format!("install a compatible {module_name} catalog entry"),
                });
            }
        }
    }

    if let Some(console_package_api) = compatibility.console_package_api.as_ref() {
        if console_package_api != HOST_CONSOLE_PACKAGE_API_VERSION {
            return Some(AdminModuleRegistrySnapshotIssueDto {
                group: "Compatibility".to_owned(),
                message: format!(
                    "{module_name} requires console package API {console_package_api}; host supports {HOST_CONSOLE_PACKAGE_API_VERSION}"
                ),
                fix: format!(
                    "install a compatible {module_name} catalog entry or update Runtime Console package support"
                ),
            });
        }
    }

    None
}

fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    Some(parse_version(left)?.cmp(&parse_version(right)?))
}

fn parse_version(value: &str) -> Option<[u64; 3]> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some([major, minor, patch])
}

fn host_module_compatibility() -> AdminModuleHostCompatibilityDto {
    AdminModuleHostCompatibilityDto {
        console_package_api: HOST_CONSOLE_PACKAGE_API_VERSION.to_owned(),
        lenso_version: HOST_LENSO_VERSION.to_owned(),
    }
}

fn module_registry_snapshot_module(
    module: AdminModuleMetadata,
    install_state: &AvailableModuleInstallStateContext,
) -> AdminModuleRegistrySnapshotModuleDto {
    let module_name = module.module_name;
    let capabilities = module.capabilities;
    let console_package_hints = module.console.len();
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
        summary: None,
        base_url,
        capabilities,
        console_package_hints,
        compatibility: None,
        host_compatibility: host_module_compatibility(),
        archived_at: None,
        archive_reason: None,
        manifest_name: if has_error {
            None
        } else {
            Some(module_name.clone())
        },
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
        install_state: install_state.install_state(&module_name),
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
                transport: remote.transport,
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
            events: None,
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
            dependencies: vec!["auth".to_owned()],
            admin: None,
            source_diagnostics: None,
        }]);

        assert_eq!(modules[0].dependencies, vec!["auth"]);
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
            events: None,
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
            dependencies: Vec::new(),
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
