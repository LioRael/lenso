use crate::dto::{
    AdminActionInvocationDto, AdminActionInvokeRequest, AdminActionInvokeResponse,
    AdminCapabilityIssueDto, AdminCapabilitySummaryDto, AdminDataDetailResponse,
    AdminDataListResponse, AdminDataPageInfo, AdminModuleActivationState,
    AdminModuleCompatibilityDto, AdminModuleConsolePackagePlanPackageDto,
    AdminModuleConsolePackagePlanStateDto, AdminModuleGovernanceDto,
    AdminModuleHostCompatibilityDto, AdminModuleInstallResponse, AdminModuleInstallStateDto,
    AdminModuleLinkedSourceInstallStateDto, AdminModuleMetadataDto,
    AdminModuleMetadataListResponse, AdminModuleRefreshModuleResultDto,
    AdminModuleRefreshModuleStatusDto, AdminModuleRefreshRecordDto, AdminModuleRefreshStatusDto,
    AdminModuleRegistrySnapshotCatalogDto, AdminModuleRegistrySnapshotIssueDto,
    AdminModuleRegistrySnapshotManifestStatus, AdminModuleRegistrySnapshotModuleDto,
    AdminModuleRegistrySnapshotModuleStatus, AdminModuleRegistrySnapshotResponse,
    AdminModuleRegistrySnapshotStatus, AdminModuleRemoteSourceInstallStateDto, AdminModuleSchema,
    AdminModuleSourceDiagnosticsDto, AdminModuleStatus, AdminQueryResponse,
    AdminRemoteModuleDiagnosticsDto, AdminSchemaListResponse, AdminSchemaRefreshResponse,
    AdminServiceModuleCompatibilityDto, AdminServiceModuleCompatibilityState,
    AdminServiceModuleDeploymentDto, AdminServiceModuleHealthCheckDto,
    AdminServiceModuleLifecycleModuleDto, AdminServiceModuleLifecycleModuleStatus,
    AdminServiceModuleLifecycleResponse, AdminServiceModuleLifecycleServiceDto,
    AdminServiceModuleLifecycleStatus, AdminServiceModuleManifestStatus,
    AdminServiceModuleServiceStatusCheckDto, AdminServiceModuleServiceStatusDto,
    AdminServiceModuleServiceStatusState,
};
use crate::{
    AdminModule, AdminModuleMetadata, AdminModuleMetadataRefreshModuleResult,
    AdminModuleMetadataRefreshModuleStatus, AdminModuleMetadataRefreshRecord,
    AdminModuleMetadataRefreshStatus, AdminModuleSourceDiagnostics, admin_metadata_refresher,
    admin_module_metadata_snapshot, admin_modules, admin_refresher, find_loaded_action_module,
    find_loaded_module, find_loaded_query_module, install_admin_module_metadata,
    install_admin_modules, record_admin_module_metadata_refresh_error,
    record_admin_module_metadata_refresh_success,
};
use axum::Json;
use axum::extract::{Path, Query, State};
use platform_core::{
    AdminActionStoryRecord, AppContext, AppError, ErrorCode, RequestContext,
    admin_action_story_event_id, insert_admin_action_story_event,
};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};
use platform_module::{
    AdminActionConfirmation, AdminActionInputField, AdminActionInputSchema,
    AdminDeclarativeComponent, AdminListQuery, AdminSurface, FieldType, ModuleLoadStatus,
    ModuleManifestLint, ModuleManifestLintSeverity, ModuleSource, lint_module_manifest_parts,
    module_capability_references,
};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path as FsPath, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const CONSOLE_EXTENSION_REGISTRY_PATH: &str = ".lenso/console/extensions/registry.json";
const CONSOLE_EXTENSION_ROUTE_PREFIX: &str = "/console/extensions";
const HOST_CONSOLE_PACKAGE_API_VERSION: &str = "1";
const HOST_LENSO_VERSION: &str = env!("CARGO_PKG_VERSION");
const HOST_REMOTE_PROTOCOL_VERSION: &str = "1";
const MODULE_INSTALL_LEDGER_PATH: &str = ".lenso/module-installs.json";
const MODULE_SERVICES_PATH: &str = ".lenso/module-services.json";
const SERVICE_MODULE_HEALTH_PATH: &str = ".lenso/service-health.json";
const OFFICIAL_MODULE_CATALOG_REGISTRY_FILE: &str = "builtin:lenso-official-module-catalog";
const OFFICIAL_MODULE_CATALOG_SOURCE: &str =
    include_str!("../catalogs/lenso-official-module-catalog.json");
const SUPPORTED_SERVICE_MODULE_FEATURES: &[&str] = &[
    "console.package-api.1",
    "service.lifecycle",
    "service.status",
];

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
        (status = 200, description = "Legacy alias for available catalog services", body = AdminModuleRegistrySnapshotResponse, content_type = "application/json"),
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
        (status = 200, description = "Available catalog services and linked modules", body = AdminModuleRegistrySnapshotResponse, content_type = "application/json"),
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

#[utoipa::path(
    get,
    path = "/admin/data/service-modules",
    operation_id = "admin_data_service_modules",
    tag = "admin-data",
    params(("authorization" = String, Header, description = "Development service bearer token")),
    responses(
        (status = 200, description = "Service provider lifecycle state", body = AdminServiceModuleLifecycleResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn service_modules(
    _admin: AdminActor,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<AdminServiceModuleLifecycleResponse>, ApiErrorResponse> {
    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        PathBuf::from(".env"),
        PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH),
    );
    Ok(Json(
        service_module_lifecycle_response(metadata, install_state).await,
    ))
}

#[utoipa::path(
    post,
    path = "/admin/data/available-modules/{module}/install",
    operation_id = "admin_data_install_available_module",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Available catalog entry name"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Catalog install state written to host-local files", body = AdminModuleInstallResponse, content_type = "application/json"),
        (status = 400, description = "Catalog entry cannot be installed", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown available module", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn install_available_module(
    _admin: AdminActor,
    Path(module): Path<String>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleInstallResponse>, ApiErrorResponse> {
    install_available_module_response(module, &request_ctx)
        .await
        .map(Json)
}

#[utoipa::path(
    delete,
    path = "/admin/data/available-modules/{module}/install",
    operation_id = "admin_data_uninstall_available_module",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Available module name"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Module install state removed from host-local files", body = AdminModuleInstallResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown available module", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn uninstall_available_module(
    _admin: AdminActor,
    Path(module): Path<String>,
    HttpRequestContext(request_ctx): HttpRequestContext,
) -> Result<Json<AdminModuleInstallResponse>, ApiErrorResponse> {
    uninstall_available_module_response(module, &request_ctx).map(Json)
}

fn available_modules_response() -> AdminModuleRegistrySnapshotResponse {
    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        PathBuf::from(".env"),
        PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH),
    );
    if let Some(response) =
        module_catalog_file_response(PathBuf::from(".lenso/module-catalog.json"), &install_state)
    {
        return response;
    }
    if metadata
        .iter()
        .any(|module| matches!(module.source, ModuleSource::Remote))
    {
        return module_registry_snapshot_response(metadata, &install_state);
    }

    module_catalog_source_response(
        OFFICIAL_MODULE_CATALOG_REGISTRY_FILE.to_owned(),
        OFFICIAL_MODULE_CATALOG_SOURCE,
        &install_state,
    )
}

async fn service_module_lifecycle_response(
    metadata: Vec<AdminModuleMetadata>,
    install_state: AvailableModuleInstallStateContext,
) -> AdminServiceModuleLifecycleResponse {
    let installed_modules = local_installed_remote_modules(MODULE_INSTALL_LEDGER_PATH);
    let services_by_module = local_module_services(MODULE_SERVICES_PATH);
    let metadata_by_name = metadata
        .iter()
        .filter(|module| matches!(module.source, ModuleSource::Remote))
        .map(|module| (module.module_name.clone(), module))
        .collect::<HashMap<_, _>>();
    let provider_names_by_module = installed_modules
        .iter()
        .filter_map(|(module_name, receipt)| {
            receipt
                .service
                .as_ref()
                .and_then(|service| service.name.as_deref())
                .filter(|provider_name| !provider_name.trim().is_empty())
                .map(|provider_name| (module_name.clone(), provider_name.trim().to_owned()))
        })
        .collect::<HashMap<_, _>>();
    let represented_provider_names = provider_names_by_module
        .values()
        .cloned()
        .collect::<HashSet<_>>();
    let mut module_names = HashSet::new();
    module_names.extend(installed_modules.keys().cloned());
    module_names.extend(
        install_state
            .remote_sources
            .modules
            .keys()
            .filter(|module_name| !represented_provider_names.contains(*module_name))
            .cloned(),
    );
    module_names.extend(
        install_state
            .running_base_urls
            .keys()
            .filter(|module_name| !represented_provider_names.contains(*module_name))
            .cloned(),
    );
    module_names.extend(
        services_by_module
            .keys()
            .filter(|module_name| !represented_provider_names.contains(*module_name))
            .cloned(),
    );
    module_names.extend(metadata_by_name.keys().cloned());
    let mut module_names = module_names.into_iter().collect::<Vec<_>>();
    module_names.sort();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .ok();
    let mut modules = Vec::new();
    for module_name in module_names {
        let provider_name = provider_names_by_module.get(&module_name);
        modules.push(
            service_module_lifecycle_module(
                &module_name,
                provider_name.map(String::as_str),
                &install_state,
                installed_modules.get(&module_name),
                services_by_module
                    .get(&module_name)
                    .or_else(|| provider_name.and_then(|name| services_by_module.get(name)))
                    .cloned()
                    .unwrap_or_default(),
                metadata_by_name.get(&module_name).copied(),
                client.as_ref(),
            )
            .await,
        );
    }
    let status = if modules.is_empty() {
        AdminServiceModuleLifecycleStatus::Empty
    } else if modules
        .iter()
        .all(|module| module.status == AdminServiceModuleLifecycleModuleStatus::Ready)
    {
        AdminServiceModuleLifecycleStatus::Ready
    } else {
        AdminServiceModuleLifecycleStatus::NeedsAttention
    };
    AdminServiceModuleLifecycleResponse {
        version: 1,
        status,
        modules,
    }
}

async fn service_module_lifecycle_module(
    module_name: &str,
    provider_name: Option<&str>,
    install_state: &AvailableModuleInstallStateContext,
    install_receipt: Option<&LocalModuleInstallLedgerModule>,
    service_specs: Vec<LocalModuleServiceSpec>,
    metadata: Option<&AdminModuleMetadata>,
    client: Option<&reqwest::Client>,
) -> AdminServiceModuleLifecycleModuleDto {
    let mut remote_source = install_state.remote_source_state(provider_name.unwrap_or(module_name));
    if provider_name.is_some() {
        reconcile_service_provider_remote_source(module_name, metadata, &mut remote_source);
    }
    let loaded =
        metadata.is_some_and(|module| matches!(module.load_status, ModuleLoadStatus::Loaded));
    let installed = install_receipt.is_some();
    let configured = remote_source.configured;
    let base_url = remote_source
        .desired_base_url
        .clone()
        .or_else(|| remote_source.running_base_url.clone())
        .or_else(|| {
            metadata.and_then(|module| match module.source_diagnostics.as_ref()? {
                AdminModuleSourceDiagnostics::Remote(remote) => {
                    Some(normalize_remote_base_url(&remote.base_url))
                }
            })
        });
    let manifest_url = base_url.as_deref().and_then(remote_module_manifest_url);
    let status_url = service_module_status_url(
        base_url.as_deref(),
        install_receipt.and_then(|receipt| receipt.service.as_ref()),
    );
    let manifest_status = if base_url.is_some() && manifest_url.is_none() {
        AdminServiceModuleManifestStatus::Skipped
    } else {
        service_module_manifest_status(metadata, manifest_url.as_deref(), client).await
    };
    let services = service_module_lifecycle_services(
        provider_name.unwrap_or(module_name),
        &service_specs,
        client,
    )
    .await;
    let service_status = service_module_status_summary(status_url.as_deref(), client).await;
    let health_history =
        record_service_module_health(module_name, status_url.as_deref(), &service_status);
    let compatibility = service_module_compatibility(module_name, install_receipt);
    let deployment = service_module_deployment(install_receipt);
    let has_stale_state = services.iter().any(|service| {
        !service.ready && (service.lock_file.is_some() || service.pid_file.is_some())
    });
    let has_service_not_ready = services.iter().any(|service| !service.ready);
    let mut fixes = Vec::new();
    let status = if has_stale_state {
        fixes
            .push("restart API/worker; remove stale lock/pid files if it remains stuck".to_owned());
        AdminServiceModuleLifecycleModuleStatus::StaleState
    } else if remote_source.restart_pending {
        fixes.push(
            remote_source
                .restart_reason
                .clone()
                .unwrap_or_else(|| "restart API and worker".to_owned()),
        );
        AdminServiceModuleLifecycleModuleStatus::RestartPending
    } else if configured && !loaded {
        fixes.push("restart API and worker, then inspect manifest errors".to_owned());
        AdminServiceModuleLifecycleModuleStatus::ConfiguredNotLoaded
    } else if manifest_status == AdminServiceModuleManifestStatus::Unreachable {
        fixes.push("start the service or fix REMOTE_MODULES".to_owned());
        AdminServiceModuleLifecycleModuleStatus::ManifestUnreachable
    } else if has_service_not_ready {
        fixes.push("start the declared service or inspect process logs".to_owned());
        AdminServiceModuleLifecycleModuleStatus::ServiceNotReady
    } else if loaded && (configured || installed || remote_source.running_base_url.is_some()) {
        AdminServiceModuleLifecycleModuleStatus::Ready
    } else {
        fixes.push("install the service or add its provider to REMOTE_MODULES".to_owned());
        AdminServiceModuleLifecycleModuleStatus::NotConfigured
    };

    AdminServiceModuleLifecycleModuleDto {
        module_name: module_name.to_owned(),
        provider_name: provider_name.map(str::to_owned),
        status,
        installed,
        configured,
        loaded,
        restart_pending: remote_source.restart_pending,
        base_url,
        manifest_url,
        manifest_status,
        status_url,
        service_status,
        health_history,
        compatibility,
        deployment,
        services,
        fixes,
    }
}

fn reconcile_service_provider_remote_source(
    module_name: &str,
    metadata: Option<&AdminModuleMetadata>,
    remote_source: &mut AdminModuleRemoteSourceInstallStateDto,
) {
    reconcile_service_provider_remote_source_base_url(
        module_name,
        metadata_remote_base_url(metadata).as_deref(),
        remote_source,
    );
}

fn reconcile_service_provider_remote_source_base_url(
    module_name: &str,
    running_module_base_url: Option<&str>,
    remote_source: &mut AdminModuleRemoteSourceInstallStateDto,
) {
    let Some(desired_base_url) = remote_source.desired_base_url.as_deref() else {
        return;
    };
    let Some(running_module_base_url) = running_module_base_url else {
        return;
    };
    if running_module_base_url == service_provider_module_base_url(desired_base_url, module_name) {
        remote_source.running_base_url = Some(normalize_remote_base_url(desired_base_url));
        remote_source.restart_pending = false;
        remote_source.restart_reason = None;
    }
}

fn metadata_remote_base_url(metadata: Option<&AdminModuleMetadata>) -> Option<String> {
    metadata.and_then(|module| match module.source_diagnostics.as_ref()? {
        AdminModuleSourceDiagnostics::Remote(remote) => {
            Some(normalize_remote_base_url(&remote.base_url))
        }
    })
}

fn service_provider_module_base_url(base_url: &str, module_name: &str) -> String {
    format!(
        "{}/modules/{module_name}",
        normalize_remote_base_url(base_url)
    )
}

fn service_module_status_url(
    base_url: Option<&str>,
    service: Option<&LocalServiceModuleMetadata>,
) -> Option<String> {
    let service = service?;
    if let Some(status_url) = service
        .status_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    {
        return Some(status_url.trim().to_owned());
    }
    let base_url = base_url?;
    let base_url = base_url.trim().trim_end_matches('/');
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return None;
    }
    let path = service
        .status_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or("status");
    if path.starts_with("http://") || path.starts_with("https://") {
        return Some(path.to_owned());
    }
    let base = reqwest::Url::parse(&format!("{base_url}/")).ok()?;
    base.join(path).ok().map(|url| url.to_string())
}

async fn service_module_status_summary(
    status_url: Option<&str>,
    client: Option<&reqwest::Client>,
) -> AdminServiceModuleServiceStatusDto {
    let Some(status_url) = status_url else {
        return service_module_status_unknown(false, None);
    };
    let Some(client) = client else {
        return service_module_status_unknown(false, None);
    };
    let response = match client.get(status_url).send().await {
        Ok(response) => response,
        Err(error) => {
            return AdminServiceModuleServiceStatusDto {
                checked: true,
                state: AdminServiceModuleServiceStatusState::Unreachable,
                error: Some(error.to_string()),
                checks: Vec::new(),
            };
        }
    };
    if !response.status().is_success() {
        return AdminServiceModuleServiceStatusDto {
            checked: true,
            state: AdminServiceModuleServiceStatusState::Unreachable,
            error: Some(format!(
                "status endpoint returned HTTP {}",
                response.status()
            )),
            checks: Vec::new(),
        };
    }
    let body = match response.json::<Value>().await {
        Ok(body) => body,
        Err(error) => {
            return AdminServiceModuleServiceStatusDto {
                checked: true,
                state: AdminServiceModuleServiceStatusState::Unreachable,
                error: Some(format!("status endpoint returned invalid JSON: {error}")),
                checks: Vec::new(),
            };
        }
    };
    AdminServiceModuleServiceStatusDto {
        checked: true,
        state: service_module_status_state(body.get("state").and_then(Value::as_str)),
        error: None,
        checks: service_module_status_checks(&body),
    }
}

fn service_module_status_unknown(
    checked: bool,
    error: Option<String>,
) -> AdminServiceModuleServiceStatusDto {
    AdminServiceModuleServiceStatusDto {
        checked,
        state: AdminServiceModuleServiceStatusState::Unknown,
        error,
        checks: Vec::new(),
    }
}

fn service_module_status_state(value: Option<&str>) -> AdminServiceModuleServiceStatusState {
    match value.unwrap_or("ready") {
        "ready" => AdminServiceModuleServiceStatusState::Ready,
        "degraded" => AdminServiceModuleServiceStatusState::Degraded,
        "starting" => AdminServiceModuleServiceStatusState::Starting,
        "unreachable" => AdminServiceModuleServiceStatusState::Unreachable,
        _ => AdminServiceModuleServiceStatusState::Unknown,
    }
}

fn service_module_status_checks(body: &Value) -> Vec<AdminServiceModuleServiceStatusCheckDto> {
    body.get("checks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|check| {
            Some(AdminServiceModuleServiceStatusCheckDto {
                name: check.get("name")?.as_str()?.to_owned(),
                status: check.get("status")?.as_str()?.to_owned(),
                detail: check
                    .get("detail")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })
        })
        .collect()
}

fn service_module_compatibility(
    module_name: &str,
    install_receipt: Option<&LocalModuleInstallLedgerModule>,
) -> AdminServiceModuleCompatibilityDto {
    let declared = install_receipt.and_then(|receipt| receipt.compatibility.clone());
    let issue = module_compatibility_issue(module_name, declared.as_ref());
    let override_allowed = issue.is_some();
    let issue_message = issue.as_ref().map(|issue| issue.message.clone());
    let issue_fix = issue.map(|issue| issue.fix);
    AdminServiceModuleCompatibilityDto {
        state: if override_allowed {
            AdminServiceModuleCompatibilityState::Blocked
        } else if declared.is_some() {
            AdminServiceModuleCompatibilityState::Compatible
        } else {
            AdminServiceModuleCompatibilityState::Unknown
        },
        declared,
        host: host_module_compatibility(),
        issue: issue_message,
        fix: issue_fix,
        override_allowed,
    }
}

fn service_module_deployment(
    install_receipt: Option<&LocalModuleInstallLedgerModule>,
) -> Option<AdminServiceModuleDeploymentDto> {
    install_receipt.and_then(|receipt| {
        receipt.deployment.clone().or_else(|| {
            receipt
                .service
                .as_ref()
                .and_then(|service| service.deployment.clone())
        })
    })
}

fn record_service_module_health(
    module_name: &str,
    status_url: Option<&str>,
    status: &AdminServiceModuleServiceStatusDto,
) -> Vec<AdminServiceModuleHealthCheckDto> {
    let path = FsPath::new(SERVICE_MODULE_HEALTH_PATH);
    let mut file = read_service_module_health_file(path);
    if status.checked
        && let Some(status_url) = status_url
    {
        file.records.push(AdminServiceModuleHealthCheckDto {
            module_name: module_name.to_owned(),
            checked_at_unix_ms: current_unix_ms(),
            status_url: status_url.to_owned(),
            state: service_module_status_state_label(&status.state).to_owned(),
            error: status.error.clone(),
        });
        if file.records.len() > 200 {
            let overflow = file.records.len() - 200;
            file.records.drain(0..overflow);
        }
        let _ = write_service_module_health_file(path, &file);
    }
    file.records
        .into_iter()
        .rev()
        .filter(|record| record.module_name == module_name)
        .take(20)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn read_service_module_health_file(path: &FsPath) -> LocalServiceModuleHealthFile {
    fs::read_to_string(path)
        .ok()
        .and_then(|source| serde_json::from_str::<LocalServiceModuleHealthFile>(&source).ok())
        .unwrap_or(LocalServiceModuleHealthFile {
            records: Vec::new(),
            version: 1,
        })
}

fn write_service_module_health_file(
    path: &FsPath,
    file: &LocalServiceModuleHealthFile,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
    fs::write(path, bytes)
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn service_module_status_state_label(state: &AdminServiceModuleServiceStatusState) -> &'static str {
    match state {
        AdminServiceModuleServiceStatusState::Ready => "ready",
        AdminServiceModuleServiceStatusState::Degraded => "degraded",
        AdminServiceModuleServiceStatusState::Starting => "starting",
        AdminServiceModuleServiceStatusState::Unreachable => "unreachable",
        AdminServiceModuleServiceStatusState::Unknown => "unknown",
    }
}

async fn service_module_manifest_status(
    metadata: Option<&AdminModuleMetadata>,
    manifest_url: Option<&str>,
    client: Option<&reqwest::Client>,
) -> AdminServiceModuleManifestStatus {
    if let Some(module) = metadata
        && let Some(AdminModuleSourceDiagnostics::Remote(remote)) =
            module.source_diagnostics.as_ref()
        && matches!(module.load_status, ModuleLoadStatus::Loaded)
    {
        return if remote.last_load_error.is_none() {
            AdminServiceModuleManifestStatus::Reachable
        } else {
            AdminServiceModuleManifestStatus::Unreachable
        };
    }
    let Some(manifest_url) = manifest_url else {
        return AdminServiceModuleManifestStatus::NotConfigured;
    };
    let Some(client) = client else {
        return AdminServiceModuleManifestStatus::Skipped;
    };
    if remote_service_ready(client, manifest_url).await {
        AdminServiceModuleManifestStatus::Reachable
    } else {
        AdminServiceModuleManifestStatus::Unreachable
    }
}

async fn service_module_lifecycle_services(
    module_name: &str,
    services: &[LocalModuleServiceSpec],
    client: Option<&reqwest::Client>,
) -> Vec<AdminServiceModuleLifecycleServiceDto> {
    let state_dir = FsPath::new(MODULE_SERVICES_PATH)
        .parent()
        .unwrap_or_else(|| FsPath::new("."));
    let mut items = Vec::new();
    for service in services {
        let ready = match client {
            Some(client) => remote_service_ready(client, &service.ready_url).await,
            None => false,
        };
        let lock_file_path =
            remote_module_service_state_path(state_dir, module_name, service, "lock");
        let pid_file_path =
            remote_module_service_state_path(state_dir, module_name, service, "pid");
        items.push(AdminServiceModuleLifecycleServiceDto {
            name: service.name.clone(),
            ready_url: service.ready_url.clone(),
            ready,
            auto_start: service.auto_start,
            lock_file: lock_file_path
                .exists()
                .then(|| lock_file_path.display().to_string()),
            pid_file: pid_file_path
                .exists()
                .then(|| pid_file_path.display().to_string()),
        });
    }
    items
}

fn local_installed_remote_modules(
    path: impl AsRef<FsPath>,
) -> HashMap<String, LocalModuleInstallLedgerModule> {
    let Ok(source) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(ledger) = serde_json::from_str::<LocalModuleInstallLedger>(&source) else {
        return HashMap::new();
    };
    ledger
        .modules
        .into_iter()
        .filter(|module| module.source.as_deref().unwrap_or("remote") == "remote")
        .map(|module| (module.module_name.clone(), module))
        .collect()
}

fn local_module_services(path: impl AsRef<FsPath>) -> HashMap<String, Vec<LocalModuleServiceSpec>> {
    let Ok(source) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(file) = serde_json::from_str::<LocalModuleServicesFile>(&source) else {
        return HashMap::new();
    };
    file.modules
        .into_iter()
        .map(|module| (module.module_name, module.services))
        .collect()
}

fn remote_module_manifest_url(base_url: &str) -> Option<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return None;
    }
    Some(if base_url.ends_with("/manifest") {
        base_url.to_owned()
    } else {
        format!("{base_url}/manifest")
    })
}

async fn remote_service_ready(client: &reqwest::Client, url: &str) -> bool {
    client
        .get(url)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

fn remote_module_service_state_path(
    services_state_dir: &FsPath,
    module_name: &str,
    service: &LocalModuleServiceSpec,
    extension: &str,
) -> PathBuf {
    services_state_dir.join(format!(
        "remote-{}-{}.{}",
        remote_module_service_state_segment(module_name),
        remote_module_service_state_segment(&service.name),
        extension
    ))
}

fn remote_module_service_state_segment(value: &str) -> String {
    let mut segment = String::new();
    let mut previous_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            segment.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !segment.is_empty() && !previous_dash {
            segment.push('-');
            previous_dash = true;
        }
    }
    while segment.ends_with('-') {
        segment.pop();
    }
    if segment.is_empty() {
        "service".to_owned()
    } else {
        segment
    }
}

async fn install_available_module_response(
    module_name: String,
    ctx: &RequestContext,
) -> Result<AdminModuleInstallResponse, ApiErrorResponse> {
    let catalog_entry =
        find_installable_catalog_entry(&module_name).map_err(|error| install_error(error, ctx))?;
    if catalog_entry.source == "linked" {
        return install_linked_available_module_response(catalog_entry, ctx).await;
    }
    let base_url = install_base_url(&catalog_entry).map_err(|error| install_error(error, ctx))?;
    validate_installable_catalog_entry(&catalog_entry, &base_url)
        .map_err(|error| install_error(error, ctx))?;

    let env_file_path = PathBuf::from(".env");
    let console_registry_file_path = PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH);
    write_runtime_console_extension_registry(
        &console_registry_file_path,
        &catalog_entry,
        &base_url,
    )
    .await
    .map_err(|error| install_error(error, ctx))?;
    let remote_source_name = catalog_entry_remote_source_name(&catalog_entry).to_owned();
    write_remote_modules_env(&env_file_path, &remote_source_name, &base_url)
        .map_err(|error| install_error(error, ctx))?;

    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        &env_file_path,
        &console_registry_file_path,
    );
    let state = catalog_entry_install_state(&catalog_entry, &install_state);
    Ok(AdminModuleInstallResponse {
        module_name: catalog_entry.name,
        manifest_reference: catalog_entry.manifest_reference,
        linked_source: state.linked_source,
        remote_source: state.remote_source,
        console_plan: state.console_plan,
        restart_required: true,
    })
}

fn uninstall_available_module_response(
    module_name: String,
    ctx: &RequestContext,
) -> Result<AdminModuleInstallResponse, ApiErrorResponse> {
    let catalog_entry =
        find_installable_catalog_entry(&module_name).map_err(|error| install_error(error, ctx))?;
    if catalog_entry.source == "linked" {
        return uninstall_linked_available_module_response(catalog_entry, ctx);
    }

    let env_file_path = PathBuf::from(".env");
    let console_registry_file_path = PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH);
    let legacy_console_plan_file_path = PathBuf::from(".lenso/console-package-install-plan.json");
    let remote_source_name = catalog_entry_remote_source_name(&catalog_entry).to_owned();
    remove_remote_modules_env(&env_file_path, &remote_source_name)
        .map_err(|error| install_error(error, ctx))?;
    remove_runtime_console_extension_registry_module(
        &console_registry_file_path,
        &catalog_entry.name,
    )
    .map_err(|error| install_error(error, ctx))?;
    remove_console_extension_module_dir(&catalog_entry.name)
        .map_err(|error| install_error(error, ctx))?;
    remove_console_package_install_plan_module(&legacy_console_plan_file_path, &catalog_entry.name)
        .map_err(|error| install_error(error, ctx))?;

    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        &env_file_path,
        &console_registry_file_path,
    );
    let state = catalog_entry_install_state(&catalog_entry, &install_state);
    Ok(AdminModuleInstallResponse {
        module_name: catalog_entry.name,
        manifest_reference: catalog_entry.manifest_reference,
        linked_source: state.linked_source,
        remote_source: state.remote_source,
        console_plan: state.console_plan,
        restart_required: true,
    })
}

async fn install_linked_available_module_response(
    catalog_entry: LocalModuleCatalogEntry,
    ctx: &RequestContext,
) -> Result<AdminModuleInstallResponse, ApiErrorResponse> {
    if catalog_entry.archived_at.is_some() {
        return Err(install_error(
            AppError::new(
                ErrorCode::Validation,
                format!("available module {} is archived", catalog_entry.name),
            ),
            ctx,
        ));
    }
    write_linked_module_profile_env(PathBuf::from(".env"))
        .map_err(|error| install_error(error, ctx))?;
    write_linked_module_enabled_env(PathBuf::from(".env"), &catalog_entry.name, true)
        .map_err(|error| install_error(error, ctx))?;
    write_linked_runtime_console_extensions(
        PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH),
        &catalog_entry,
    )
    .await
    .map_err(|error| install_error(error, ctx))?;
    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        PathBuf::from(".env"),
        PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH),
    );
    let state = install_state.install_state_for_source(&catalog_entry.name, ModuleSource::Linked);
    Ok(AdminModuleInstallResponse {
        module_name: catalog_entry.name,
        manifest_reference: catalog_entry.manifest_reference,
        linked_source: state.linked_source,
        remote_source: state.remote_source,
        console_plan: state.console_plan,
        restart_required: true,
    })
}

fn uninstall_linked_available_module_response(
    catalog_entry: LocalModuleCatalogEntry,
    ctx: &RequestContext,
) -> Result<AdminModuleInstallResponse, ApiErrorResponse> {
    write_linked_module_enabled_env(PathBuf::from(".env"), &catalog_entry.name, false)
        .map_err(|error| install_error(error, ctx))?;
    remove_runtime_console_extension_registry_module(
        PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH),
        &catalog_entry.name,
    )
    .map_err(|error| install_error(error, ctx))?;
    let metadata = admin_module_metadata_snapshot().modules;
    let install_state = AvailableModuleInstallStateContext::from_paths(
        &metadata,
        PathBuf::from(".env"),
        PathBuf::from(CONSOLE_EXTENSION_REGISTRY_PATH),
    );
    let state = install_state.install_state_for_source(&catalog_entry.name, ModuleSource::Linked);
    Ok(AdminModuleInstallResponse {
        module_name: catalog_entry.name,
        manifest_reference: catalog_entry.manifest_reference,
        linked_source: state.linked_source,
        remote_source: state.remote_source,
        console_plan: state.console_plan,
        restart_required: true,
    })
}

fn install_error(error: AppError, ctx: &RequestContext) -> ApiErrorResponse {
    ApiErrorResponse::with_context(error, ctx)
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
            message: format!("{} service metadata needs attention", module.name),
            fix: "refresh module metadata and verify service manifest configuration".to_owned(),
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleCatalog {
    #[serde(default)]
    modules: Vec<LocalModuleCatalogEntry>,
    #[serde(default = "default_module_catalog_version")]
    version: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleCatalogEntry {
    name: String,
    version: String,
    source: String,
    manifest_reference: String,
    #[serde(default, rename = "providedBy")]
    provided_by: Option<String>,
    #[serde(default, rename = "serviceManifest")]
    service_manifest: Option<String>,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleCatalogConsolePackage {
    package_name: String,
    export_name: String,
    #[serde(default, alias = "bundle_url")]
    bundle_url: Option<String>,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default, alias = "host_api")]
    host_api: Option<String>,
    #[serde(default)]
    route: Option<String>,
    #[serde(default)]
    required_capabilities: Vec<String>,
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug)]
struct AvailableModuleInstallStateContext {
    console_plan: LocalConsolePackageInstallPlanState,
    linked_modules: LocalLinkedModulesEnvState,
    registered_modules: HashSet<String>,
    remote_sources: LocalRemoteModulesEnvState,
    running_base_urls: HashMap<String, String>,
}

#[derive(Debug)]
struct LocalLinkedModulesEnvState {
    env_file: String,
    error: Option<String>,
    enabled: HashMap<String, bool>,
}

#[derive(Debug)]
struct LocalRemoteModulesEnvState {
    env_file: String,
    error: Option<String>,
    modules: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleInstallLedger {
    #[serde(default)]
    modules: Vec<LocalModuleInstallLedgerModule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleInstallLedgerModule {
    module_name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    compatibility: Option<AdminModuleCompatibilityDto>,
    #[serde(default)]
    deployment: Option<AdminServiceModuleDeploymentDto>,
    #[serde(default)]
    service: Option<LocalServiceModuleMetadata>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalServiceModuleMetadata {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    deployment: Option<AdminServiceModuleDeploymentDto>,
    #[serde(default, alias = "status_path")]
    status_path: Option<String>,
    #[serde(default, alias = "status_url")]
    status_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleServicesFile {
    #[serde(default)]
    modules: Vec<LocalModuleServiceState>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalServiceModuleHealthFile {
    #[serde(default)]
    records: Vec<AdminServiceModuleHealthCheckDto>,
    #[serde(default = "default_service_module_health_version")]
    version: u8,
}

const fn default_service_module_health_version() -> u8 {
    1
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleServiceState {
    module_name: String,
    #[serde(default)]
    services: Vec<LocalModuleServiceSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleServiceSpec {
    name: String,
    ready_url: String,
    #[serde(default = "default_service_auto_start")]
    auto_start: bool,
}

const fn default_service_auto_start() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalConsolePackageInstallPlan {
    #[serde(default)]
    modules: Vec<LocalConsolePackageInstallPlanModule>,
    #[serde(default = "default_console_package_install_plan_version")]
    version: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalConsolePackageInstallPlanModule {
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    console_packages: Vec<LocalConsolePackageInstallPlanPackage>,
    #[serde(default)]
    manifest_reference: Option<String>,
    module_name: String,
    #[serde(default)]
    restart_required: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalConsolePackageInstallPlanPackage {
    #[serde(default)]
    command: Option<String>,
    export_name: String,
    #[serde(default)]
    key: Option<String>,
    package_name: String,
    #[serde(default)]
    requested_by_module: Option<String>,
    #[serde(default)]
    route: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalRuntimeConsoleBundleRegistry {
    #[serde(default)]
    bundles: Vec<LocalRuntimeConsoleBundle>,
    #[serde(default = "default_console_package_install_plan_version")]
    version: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalRuntimeConsoleBundle {
    entry: String,
    export_name: String,
    host_api: String,
    #[serde(default)]
    module_name: Option<String>,
    package_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    required_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    styles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

#[derive(Clone, Debug)]
struct RuntimeConsoleBundleSpec {
    bundle_url: String,
    entry: String,
    export_name: String,
    host_api: String,
    module_name: String,
    package_name: String,
    required_capabilities: Vec<String>,
    route: Option<String>,
    styles: Vec<RuntimeConsoleBundleStyleSpec>,
    target_path: PathBuf,
    version: Option<String>,
}

#[derive(Clone, Debug)]
struct RuntimeConsoleBundleStyleSpec {
    entry: String,
    source_url: String,
    target_path: PathBuf,
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
            linked_modules: local_linked_modules_env_state(&env_file_path),
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
        self.install_state_for_source(module_name, ModuleSource::Remote)
    }

    fn install_state_for_source(
        &self,
        module_name: &str,
        source: ModuleSource,
    ) -> AdminModuleInstallStateDto {
        self.install_state_for_remote_source(module_name, source, module_name)
    }

    fn install_state_for_remote_source(
        &self,
        module_name: &str,
        source: ModuleSource,
        remote_source_name: &str,
    ) -> AdminModuleInstallStateDto {
        AdminModuleInstallStateDto {
            module_registered: self.registered_modules.contains(module_name),
            linked_source: match source {
                ModuleSource::Linked => Some(self.linked_source_state(module_name)),
                _ => None,
            },
            remote_source: match source {
                ModuleSource::Linked => None,
                ModuleSource::Remote => Some(self.remote_source_state(remote_source_name)),
                _ => Some(self.remote_source_state(remote_source_name)),
            },
            console_plan: self.console_plan_state(module_name),
        }
    }

    fn linked_source_state(&self, module_name: &str) -> AdminModuleLinkedSourceInstallStateDto {
        let desired_enabled = self.linked_modules.enabled.get(module_name).copied();
        let running_enabled = self.registered_modules.contains(module_name);
        let restart_reason = linked_source_restart_reason(desired_enabled, running_enabled);

        AdminModuleLinkedSourceInstallStateDto {
            env_file: self.linked_modules.env_file.clone(),
            configured: desired_enabled.is_some(),
            desired_enabled,
            running_enabled,
            restart_pending: restart_reason.is_some(),
            restart_reason,
            error: self.linked_modules.error.clone(),
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

    fn reconcile_service_provider_remote_source(
        &self,
        module_name: &str,
        remote_source: &mut AdminModuleRemoteSourceInstallStateDto,
    ) {
        reconcile_service_provider_remote_source_base_url(
            module_name,
            self.running_base_urls.get(module_name).map(String::as_str),
            remote_source,
        );
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

fn local_linked_modules_env_state(env_file_path: impl AsRef<FsPath>) -> LocalLinkedModulesEnvState {
    let env_file_path = env_file_path.as_ref();
    let env_file = env_file_path.display().to_string();
    let source = match fs::read_to_string(env_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LocalLinkedModulesEnvState {
                env_file,
                error: None,
                enabled: HashMap::new(),
            };
        }
        Err(error) => {
            return LocalLinkedModulesEnvState {
                env_file,
                error: Some(format!("linked module env file could not be read: {error}")),
                enabled: HashMap::new(),
            };
        }
    };
    LocalLinkedModulesEnvState {
        env_file,
        error: None,
        enabled: parse_linked_modules_env_source(&source),
    }
}

fn parse_linked_modules_env_source(source: &str) -> HashMap<String, bool> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            let module_name = module_name_from_linked_enabled_env_key(key.trim())?;
            Some((
                module_name,
                parse_bool_env_value(&unquote_env_value(value.trim()))?,
            ))
        })
        .collect()
}

fn module_name_from_linked_enabled_env_key(key: &str) -> Option<String> {
    Some(
        key.strip_prefix("LENSO_MODULE_")?
            .strip_suffix("_ENABLED")?
            .to_ascii_lowercase()
            .replace('_', "-"),
    )
    .filter(|name| !name.is_empty())
}

fn parse_bool_env_value(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
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
    console_registry_file_path: impl AsRef<FsPath>,
) -> LocalConsolePackageInstallPlanState {
    let console_registry_file_path = console_registry_file_path.as_ref();
    let plan_file = console_registry_file_path.display().to_string();
    let source = match fs::read_to_string(console_registry_file_path) {
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
                    "runtime console extension registry could not be read: {error}"
                )),
                exists: true,
                modules: HashMap::new(),
                plan_file,
            };
        }
    };
    match serde_json::from_str::<LocalRuntimeConsoleBundleRegistry>(&source) {
        Ok(registry) => {
            let mut modules: HashMap<String, LocalConsolePackageInstallPlanModule> = HashMap::new();
            for bundle in registry.bundles {
                let Some(module_name) = bundle.module_name else {
                    continue;
                };
                let key = format!("{}#{}", bundle.package_name, bundle.export_name);
                let package = LocalConsolePackageInstallPlanPackage {
                    command: None,
                    export_name: bundle.export_name,
                    key: Some(key),
                    package_name: bundle.package_name,
                    requested_by_module: Some(module_name.clone()),
                    route: bundle.route,
                    status: Some("installed".to_owned()),
                };
                modules
                    .entry(module_name.clone())
                    .or_insert_with(|| LocalConsolePackageInstallPlanModule {
                        base_url: None,
                        console_packages: Vec::new(),
                        manifest_reference: None,
                        module_name,
                        restart_required: Some(true),
                    })
                    .console_packages
                    .push(package);
            }
            LocalConsolePackageInstallPlanState {
                error: None,
                exists: true,
                modules,
                plan_file,
            }
        }
        Err(error) => LocalConsolePackageInstallPlanState {
            error: Some(format!(
                "runtime console extension registry could not be parsed: {error}"
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
        (Some(_), None) => {
            Some("service provider source configured in .env but not loaded".to_owned())
        }
        (None, Some(_)) => {
            Some("service provider source removed from .env but still loaded".to_owned())
        }
        (Some(desired), Some(running)) if desired != running => {
            Some("REMOTE_MODULES base URL differs from loaded service provider metadata".to_owned())
        }
        _ => None,
    }
}

fn linked_source_restart_reason(
    desired_enabled: Option<bool>,
    running_enabled: bool,
) -> Option<String> {
    match (desired_enabled, running_enabled) {
        (Some(true), false) => {
            Some("linked module enabled by env override; restart API and worker".to_owned())
        }
        (Some(false), true) => {
            Some("linked module disabled by env override; restart API and worker".to_owned())
        }
        _ => None,
    }
}

fn normalize_remote_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_owned()
}

fn module_catalog_file_response(
    catalog_file_path: impl AsRef<FsPath>,
    install_state: &AvailableModuleInstallStateContext,
) -> Option<AdminModuleRegistrySnapshotResponse> {
    let catalog_file_path = catalog_file_path.as_ref();
    let source = match fs::read_to_string(catalog_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            return Some(module_catalog_error_response(
                catalog_file_path.display().to_string(),
                format!("module catalog could not be read: {error}"),
            ));
        }
    };
    Some(module_catalog_source_response(
        catalog_file_path.display().to_string(),
        &source,
        install_state,
    ))
}

fn find_installable_catalog_entry(module_name: &str) -> Result<LocalModuleCatalogEntry, AppError> {
    let catalog = read_install_catalog(PathBuf::from(".lenso/module-catalog.json"))?
        .unwrap_or_else(official_module_catalog);
    catalog
        .modules
        .into_iter()
        .find(|entry| entry.name == module_name)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("available module {module_name} was not found"),
            )
        })
}

fn read_install_catalog(
    catalog_file_path: impl AsRef<FsPath>,
) -> Result<Option<LocalModuleCatalog>, AppError> {
    let catalog_file_path = catalog_file_path.as_ref();
    let source = match fs::read_to_string(catalog_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("module catalog could not be read: {error}"),
            ));
        }
    };
    serde_json::from_str::<LocalModuleCatalog>(&source)
        .map(Some)
        .map_err(|error| {
            AppError::new(
                ErrorCode::Validation,
                format!("module catalog could not be parsed: {error}"),
            )
        })
}

fn official_module_catalog() -> LocalModuleCatalog {
    serde_json::from_str::<LocalModuleCatalog>(OFFICIAL_MODULE_CATALOG_SOURCE)
        .expect("official module catalog is valid")
}

fn module_catalog_source_response(
    registry_file: String,
    source: &str,
    install_state: &AvailableModuleInstallStateContext,
) -> AdminModuleRegistrySnapshotResponse {
    let catalog = match serde_json::from_str::<LocalModuleCatalog>(&source) {
        Ok(catalog) => catalog,
        Err(error) => {
            return module_catalog_error_response(
                registry_file,
                format!("module catalog could not be parsed: {error}"),
            );
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

    AdminModuleRegistrySnapshotResponse {
        version: 1,
        status: if issues.is_empty() {
            AdminModuleRegistrySnapshotStatus::Passed
        } else {
            AdminModuleRegistrySnapshotStatus::Failed
        },
        catalog: AdminModuleRegistrySnapshotCatalogDto {
            modules: modules.len(),
            registry_file,
            version: catalog.version,
        },
        issues,
        modules,
    }
}

fn validate_installable_catalog_entry(
    entry: &LocalModuleCatalogEntry,
    base_url: &str,
) -> Result<(), AppError> {
    if entry.archived_at.is_some() {
        return Err(AppError::new(
            ErrorCode::Validation,
            format!("available module {} is archived", entry.name),
        ));
    }
    if !matches!(entry.source.as_str(), "remote" | "service") {
        return Err(AppError::new(
            ErrorCode::Validation,
            "only service catalog entries can be installed visually",
        ));
    }
    if base_url.is_empty() {
        return Err(AppError::new(
            ErrorCode::Validation,
            format!("{} baseUrl is missing", entry.name),
        ));
    }
    if module_compatibility_issue(&entry.name, entry.compatibility.as_ref()).is_some() {
        return Err(AppError::new(
            ErrorCode::Validation,
            format!("{} is not compatible with this Lenso host", entry.name),
        ));
    }
    Ok(())
}

fn install_base_url(entry: &LocalModuleCatalogEntry) -> Result<String, AppError> {
    if let Some(base_url) = entry.base_url.as_ref() {
        return Ok(normalize_remote_base_url(base_url));
    }
    if is_http_manifest_reference(&entry.manifest_reference) {
        return Ok(entry
            .manifest_reference
            .strip_suffix("/manifest")
            .unwrap_or(&entry.manifest_reference)
            .to_owned());
    }
    Err(AppError::new(
        ErrorCode::Validation,
        format!("{} baseUrl is missing", entry.name),
    ))
}

fn catalog_entry_remote_source_name(entry: &LocalModuleCatalogEntry) -> &str {
    if entry.source == "service" {
        return entry
            .provided_by
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&entry.name);
    }
    &entry.name
}

fn catalog_entry_install_state(
    entry: &LocalModuleCatalogEntry,
    install_state: &AvailableModuleInstallStateContext,
) -> AdminModuleInstallStateDto {
    let source = catalog_entry_source(&entry.source);
    let remote_source_name = catalog_entry_remote_source_name(entry);
    let mut state =
        install_state.install_state_for_remote_source(&entry.name, source, remote_source_name);
    if entry.source == "service"
        && let Some(remote_source) = state.remote_source.as_mut()
    {
        install_state.reconcile_service_provider_remote_source(&entry.name, remote_source);
    }
    state
}

fn write_remote_modules_env(
    env_file_path: impl AsRef<FsPath>,
    module_name: &str,
    base_url: &str,
) -> Result<(), AppError> {
    let env_file_path = env_file_path.as_ref();
    let source = match fs::read_to_string(env_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote module env file could not be read: {error}"),
            ));
        }
    };
    let current_value = source
        .lines()
        .filter_map(remote_modules_env_value)
        .last()
        .unwrap_or_default();
    let mut entries = parse_remote_modules_entries(&current_value);
    entries.retain(|entry| entry.name != module_name);
    entries.push(RemoteModuleEnvEntry {
        name: module_name.to_owned(),
        base_url: normalize_remote_base_url(base_url),
    });
    let next_source = upsert_env_source(
        &source,
        "REMOTE_MODULES",
        &format_remote_modules_entries(&entries),
    );
    fs::write(env_file_path, next_source).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote module env file could not be written: {error}"),
        )
    })
}

fn remove_remote_modules_env(
    env_file_path: impl AsRef<FsPath>,
    module_name: &str,
) -> Result<(), AppError> {
    let env_file_path = env_file_path.as_ref();
    let source = match fs::read_to_string(env_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote module env file could not be read: {error}"),
            ));
        }
    };
    let current_value = source
        .lines()
        .filter_map(remote_modules_env_value)
        .last()
        .unwrap_or_default();
    let mut entries = parse_remote_modules_entries(&current_value);
    entries.retain(|entry| entry.name != module_name);
    let next_source = if entries.is_empty() {
        remove_env_key_source(&source, "REMOTE_MODULES")
    } else {
        upsert_env_source(
            &source,
            "REMOTE_MODULES",
            &format_remote_modules_entries(&entries),
        )
    };
    fs::write(env_file_path, next_source).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote module env file could not be written: {error}"),
        )
    })
}

fn write_linked_module_profile_env(env_file_path: impl AsRef<FsPath>) -> Result<(), AppError> {
    let env_file_path = env_file_path.as_ref();
    let source = match fs::read_to_string(env_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("linked module env file could not be read: {error}"),
            ));
        }
    };
    let next_source = upsert_env_source(&source, "LENSO_COMPOSITION_PROFILE", "demo");
    fs::write(env_file_path, next_source).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("linked module env file could not be written: {error}"),
        )
    })
}

fn write_linked_module_enabled_env(
    env_file_path: impl AsRef<FsPath>,
    module_name: &str,
    enabled: bool,
) -> Result<(), AppError> {
    let env_file_path = env_file_path.as_ref();
    let source = match fs::read_to_string(env_file_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("linked module env file could not be read: {error}"),
            ));
        }
    };
    let key = linked_module_enabled_env_key(module_name);
    let next_source = upsert_env_source(&source, &key, if enabled { "true" } else { "false" });
    fs::write(env_file_path, next_source).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("linked module env file could not be written: {error}"),
        )
    })
}

fn write_linked_runtime_console_extension_registry(
    console_registry_file_path: impl AsRef<FsPath>,
    entry: &LocalModuleCatalogEntry,
) -> Result<(), AppError> {
    let console_registry_file_path = console_registry_file_path.as_ref();
    if entry.console_packages.is_empty() {
        return Ok(());
    }

    let mut registry = read_runtime_console_extension_registry(console_registry_file_path)?;
    registry.version = 1;
    registry.bundles.retain(|bundle| {
        bundle.module_name.as_deref() != Some(entry.name.as_str())
            && !entry.console_packages.iter().any(|package| {
                bundle.package_name == package.package_name
                    && bundle.export_name == package.export_name
            })
    });
    registry
        .bundles
        .extend(entry.console_packages.iter().map(|package| {
            LocalRuntimeConsoleBundle {
                entry: package
                    .entry
                    .clone()
                    .unwrap_or_else(|| linked_console_package_entry(&entry.name, package)),
                export_name: package.export_name.clone(),
                host_api: package
                    .host_api
                    .clone()
                    .unwrap_or_else(|| HOST_CONSOLE_PACKAGE_API_VERSION.to_owned()),
                module_name: Some(entry.name.clone()),
                package_name: package.package_name.clone(),
                required_capabilities: package.required_capabilities.clone(),
                route: package.route.clone(),
                styles: package.styles.clone(),
                version: package.version.clone(),
            }
        }));
    write_runtime_console_extension_registry_file(console_registry_file_path, &registry)
}

async fn write_linked_runtime_console_extensions(
    console_registry_file_path: impl AsRef<FsPath>,
    entry: &LocalModuleCatalogEntry,
) -> Result<(), AppError> {
    if entry.console_packages.is_empty() {
        return Ok(());
    }
    if entry
        .console_packages
        .iter()
        .all(|package| package.bundle_url.is_some())
    {
        return write_runtime_console_extension_registry(
            console_registry_file_path,
            entry,
            entry.base_url.as_deref().unwrap_or(""),
        )
        .await;
    }
    write_linked_runtime_console_extension_registry(console_registry_file_path, entry)
}

fn linked_console_package_entry(
    module_name: &str,
    package: &LocalModuleCatalogConsolePackage,
) -> String {
    format!(
        "{CONSOLE_EXTENSION_ROUTE_PREFIX}/{}/{}.js",
        slugify(module_name),
        slugify(&package.export_name)
    )
}

async fn write_runtime_console_extension_registry(
    console_registry_file_path: impl AsRef<FsPath>,
    entry: &LocalModuleCatalogEntry,
    base_url: &str,
) -> Result<(), AppError> {
    let console_registry_file_path = console_registry_file_path.as_ref();
    let specs = runtime_console_bundle_specs(console_registry_file_path, entry, base_url)?;
    if specs.is_empty() {
        return Ok(());
    }

    for spec in &specs {
        let bytes = read_console_bundle_reference(&spec.bundle_url).await?;
        if let Some(parent) = spec.target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("console extension bundle directory could not be created: {error}"),
                )
            })?;
        }
        fs::write(&spec.target_path, bytes).map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("console extension bundle could not be written: {error}"),
            )
        })?;
        for style in &spec.styles {
            let bytes = read_console_bundle_reference(&style.source_url).await?;
            if let Some(parent) = style.target_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    AppError::new(
                        ErrorCode::ExternalDependency,
                        format!("console extension style directory could not be created: {error}"),
                    )
                })?;
            }
            fs::write(&style.target_path, bytes).map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("console extension style could not be written: {error}"),
                )
            })?;
        }
    }

    let mut registry = read_runtime_console_extension_registry(console_registry_file_path)?;
    registry.version = 1;
    registry.bundles.retain(|bundle| {
        bundle.module_name.as_deref() != Some(entry.name.as_str())
            && !specs.iter().any(|spec| {
                bundle.package_name == spec.package_name && bundle.export_name == spec.export_name
            })
    });
    registry
        .bundles
        .extend(specs.into_iter().map(|spec| LocalRuntimeConsoleBundle {
            entry: spec.entry,
            export_name: spec.export_name,
            host_api: spec.host_api,
            module_name: Some(spec.module_name),
            package_name: spec.package_name,
            required_capabilities: spec.required_capabilities,
            route: spec.route,
            styles: spec.styles.into_iter().map(|style| style.entry).collect(),
            version: spec.version,
        }));
    write_runtime_console_extension_registry_file(console_registry_file_path, &registry)
}

fn runtime_console_bundle_specs(
    console_registry_file_path: &FsPath,
    entry: &LocalModuleCatalogEntry,
    base_url: &str,
) -> Result<Vec<RuntimeConsoleBundleSpec>, AppError> {
    let module_slug = slugify(&entry.name);
    entry
        .console_packages
        .iter()
        .map(|package| {
            let bundle_url = package.bundle_url.as_deref().ok_or_else(|| {
                AppError::new(
                    ErrorCode::Validation,
                    format!(
                        "{} console package {}#{} bundleUrl is missing",
                        entry.name, package.package_name, package.export_name
                    ),
                )
            })?;
            let bundle_url = resolve_console_bundle_reference(bundle_url, base_url)?;
            let file_name = console_bundle_file_name(&bundle_url, &package.export_name);
            let target_path = console_registry_file_path
                .parent()
                .unwrap_or_else(|| FsPath::new(".lenso/console/extensions"))
                .join(&module_slug)
                .join(&file_name);
            let styles = package
                .styles
                .iter()
                .map(|style_reference| {
                    let source_url = resolve_console_bundle_reference(style_reference, base_url)?;
                    let file_name = console_style_file_name(&source_url, &package.export_name);
                    Ok(RuntimeConsoleBundleStyleSpec {
                        entry: format!(
                            "{CONSOLE_EXTENSION_ROUTE_PREFIX}/{module_slug}/{file_name}"
                        ),
                        source_url,
                        target_path: console_registry_file_path
                            .parent()
                            .unwrap_or_else(|| FsPath::new(".lenso/console/extensions"))
                            .join(&module_slug)
                            .join(file_name),
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok(RuntimeConsoleBundleSpec {
                bundle_url,
                entry: format!("{CONSOLE_EXTENSION_ROUTE_PREFIX}/{module_slug}/{file_name}"),
                export_name: package.export_name.clone(),
                host_api: package
                    .host_api
                    .clone()
                    .unwrap_or_else(|| HOST_CONSOLE_PACKAGE_API_VERSION.to_owned()),
                module_name: entry.name.clone(),
                package_name: package.package_name.clone(),
                required_capabilities: package.required_capabilities.clone(),
                route: package.route.clone(),
                styles,
                target_path,
                version: package.version.clone(),
            })
        })
        .collect()
}

fn resolve_console_bundle_reference(reference: &str, base_url: &str) -> Result<String, AppError> {
    if reference.starts_with("http://")
        || reference.starts_with("https://")
        || reference.starts_with("file://")
    {
        return Ok(reference.to_owned());
    }
    let normalized_base = format!("{}/", base_url.trim_end_matches('/'));
    let base = reqwest::Url::parse(&normalized_base).map_err(|error| {
        AppError::new(
            ErrorCode::Validation,
            format!("console bundle base URL could not be parsed: {error}"),
        )
    })?;
    base.join(reference)
        .map(|url| url.to_string())
        .map_err(|error| {
            AppError::new(
                ErrorCode::Validation,
                format!("console bundle URL could not be resolved: {error}"),
            )
        })
}

fn console_bundle_file_name(bundle_url: &str, export_name: &str) -> String {
    console_asset_file_name(bundle_url, export_name, "js")
}

fn console_style_file_name(style_url: &str, export_name: &str) -> String {
    console_asset_file_name(style_url, export_name, "css")
}

fn console_asset_file_name(asset_url: &str, export_name: &str, extension: &str) -> String {
    reqwest::Url::parse(asset_url)
        .ok()
        .and_then(|url| {
            url.path_segments()
                .and_then(Iterator::last)
                .filter(|segment| !segment.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            FsPath::new(asset_url)
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("{}.{}", slugify(export_name), extension))
}

async fn read_console_bundle_reference(reference: &str) -> Result<Vec<u8>, AppError> {
    if reference.starts_with("http://") || reference.starts_with("https://") {
        let response = reqwest::get(reference).await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("console bundle could not be fetched: {error}"),
            )
        })?;
        if !response.status().is_success() {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "console bundle fetch failed: {} {}",
                    response.status().as_u16(),
                    response.status().canonical_reason().unwrap_or("")
                ),
            ));
        }
        return response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("console bundle bytes could not be read: {error}"),
                )
            });
    }
    let path = if let Some(file_path) = reference.strip_prefix("file://") {
        PathBuf::from(file_path)
    } else {
        PathBuf::from(reference)
    };
    fs::read(&path).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!(
                "console bundle {} could not be read: {error}",
                path.display()
            ),
        )
    })
}

fn read_runtime_console_extension_registry(
    console_registry_file_path: &FsPath,
) -> Result<LocalRuntimeConsoleBundleRegistry, AppError> {
    match fs::read_to_string(console_registry_file_path) {
        Ok(source) => {
            serde_json::from_str::<LocalRuntimeConsoleBundleRegistry>(&source).map_err(|error| {
                AppError::new(
                    ErrorCode::Validation,
                    format!("runtime console extension registry could not be parsed: {error}"),
                )
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(LocalRuntimeConsoleBundleRegistry {
                bundles: vec![],
                version: 1,
            })
        }
        Err(error) => Err(AppError::new(
            ErrorCode::ExternalDependency,
            format!("runtime console extension registry could not be read: {error}"),
        )),
    }
}

fn write_runtime_console_extension_registry_file(
    console_registry_file_path: &FsPath,
    registry: &LocalRuntimeConsoleBundleRegistry,
) -> Result<(), AppError> {
    if let Some(parent) = console_registry_file_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "runtime console extension registry directory could not be created: {error}"
                ),
            )
        })?;
    }
    let source = serde_json::to_string_pretty(registry)
        .map(|source| format!("{source}\n"))
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("runtime console extension registry could not be encoded: {error}"),
            )
        })?;
    fs::write(console_registry_file_path, source).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("runtime console extension registry could not be written: {error}"),
        )
    })
}

fn remove_console_package_install_plan_module(
    console_plan_file_path: impl AsRef<FsPath>,
    module_name: &str,
) -> Result<(), AppError> {
    let console_plan_file_path = console_plan_file_path.as_ref();
    let mut plan = match fs::read_to_string(console_plan_file_path) {
        Ok(source) => {
            serde_json::from_str::<LocalConsolePackageInstallPlan>(&source).map_err(|error| {
                AppError::new(
                    ErrorCode::Validation,
                    format!("console package install plan could not be parsed: {error}"),
                )
            })?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("console package install plan could not be read: {error}"),
            ));
        }
    };
    plan.modules
        .retain(|module| module.module_name != module_name);
    let source = serde_json::to_string_pretty(&plan)
        .map(|source| format!("{source}\n"))
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("console package install plan could not be encoded: {error}"),
            )
        })?;
    fs::write(console_plan_file_path, source).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("console package install plan could not be written: {error}"),
        )
    })
}

fn remove_runtime_console_extension_registry_module(
    console_registry_file_path: impl AsRef<FsPath>,
    module_name: &str,
) -> Result<(), AppError> {
    let console_registry_file_path = console_registry_file_path.as_ref();
    let mut registry = match fs::read_to_string(console_registry_file_path) {
        Ok(source) => {
            serde_json::from_str::<LocalRuntimeConsoleBundleRegistry>(&source).map_err(|error| {
                AppError::new(
                    ErrorCode::Validation,
                    format!("runtime console extension registry could not be parsed: {error}"),
                )
            })?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("runtime console extension registry could not be read: {error}"),
            ));
        }
    };
    registry
        .bundles
        .retain(|bundle| bundle.module_name.as_deref() != Some(module_name));
    write_runtime_console_extension_registry_file(console_registry_file_path, &registry)
}

fn remove_console_extension_module_dir(module_name: &str) -> Result<(), AppError> {
    let path = PathBuf::from(".lenso/console/extensions").join(slugify(module_name));
    match fs::remove_dir_all(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::new(
            ErrorCode::ExternalDependency,
            format!(
                "runtime console extension directory {} could not be removed: {error}",
                path.display()
            ),
        )),
    }
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    slug.trim_matches('-').to_owned()
}

#[derive(Debug)]
struct RemoteModuleEnvEntry {
    name: String,
    base_url: String,
}

fn parse_remote_modules_entries(value: &str) -> Vec<RemoteModuleEnvEntry> {
    value
        .split(',')
        .filter_map(|entry| {
            let (name, base_url) = entry.trim().split_once('=')?;
            let name = name.trim();
            let base_url = normalize_remote_base_url(base_url);
            if name.is_empty() || base_url.is_empty() {
                return None;
            }
            Some(RemoteModuleEnvEntry {
                name: name.to_owned(),
                base_url,
            })
        })
        .collect()
}

fn format_remote_modules_entries(entries: &[RemoteModuleEnvEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}={}", entry.name, entry.base_url))
        .collect::<Vec<_>>()
        .join(",")
}

fn upsert_env_source(source: &str, key: &str, value: &str) -> String {
    let mut replaced = false;
    let mut lines = Vec::new();
    for line in source.lines() {
        if env_value(line, key).is_some() {
            if !replaced {
                lines.push(format!("{key}={value}"));
                replaced = true;
            }
        } else {
            lines.push(line.to_owned());
        }
    }
    if !replaced {
        lines.push(format!("{key}={value}"));
    }
    format!("{}\n", lines.join("\n"))
}

fn remove_env_key_source(source: &str, key: &str) -> String {
    let lines = source
        .lines()
        .filter(|line| env_value(line, key).is_none())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn env_value(line: &str, expected_key: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, value) = line.split_once('=')?;
    (key.trim() == expected_key).then(|| unquote_env_value(value.trim()).to_owned())
}

fn linked_module_enabled_env_key(module_name: &str) -> String {
    format!(
        "LENSO_MODULE_{}_ENABLED",
        module_name.replace('-', "_").to_ascii_uppercase()
    )
}

fn module_catalog_error_response(
    registry_file: String,
    message: String,
) -> AdminModuleRegistrySnapshotResponse {
    AdminModuleRegistrySnapshotResponse {
        version: 1,
        status: AdminModuleRegistrySnapshotStatus::Failed,
        catalog: AdminModuleRegistrySnapshotCatalogDto {
            modules: 0,
            registry_file,
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
    let source = catalog_entry_source(&entry.source);
    let state = catalog_entry_install_state(&entry, install_state);
    let archived = entry.archived_at.is_some();
    let name = entry.name;
    let needs_attention = !archived
        && ((matches!(source, ModuleSource::Remote)
            && module_needs_base_url(entry.base_url.as_deref(), &entry.manifest_reference))
            || module_compatibility_issue(&name, entry.compatibility.as_ref()).is_some());

    AdminModuleRegistrySnapshotModuleDto {
        name: name.clone(),
        source,
        catalog_version: entry.version.clone(),
        manifest_reference: entry.manifest_reference,
        provided_by: entry.provided_by,
        service_manifest: entry.service_manifest,
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
        install_state: state,
        status: if archived {
            AdminModuleRegistrySnapshotModuleStatus::Archived
        } else if needs_attention {
            AdminModuleRegistrySnapshotModuleStatus::NeedsAttention
        } else {
            AdminModuleRegistrySnapshotModuleStatus::Ready
        },
    }
}

fn catalog_entry_source(source: &str) -> ModuleSource {
    if source == "linked" {
        ModuleSource::Linked
    } else {
        ModuleSource::Remote
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
    if matches!(module.source, ModuleSource::Remote)
        && module_needs_base_url(module.base_url.as_deref(), &module.manifest_reference)
    {
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

    if let Some(remote_protocol_version) = compatibility.remote_protocol_version.as_ref() {
        if remote_protocol_version != HOST_REMOTE_PROTOCOL_VERSION {
            return Some(AdminModuleRegistrySnapshotIssueDto {
                group: "Compatibility".to_owned(),
                message: format!(
                    "{module_name} requires remote protocol {remote_protocol_version}; host supports {HOST_REMOTE_PROTOCOL_VERSION}"
                ),
                fix: format!("install a compatible {module_name} service release"),
            });
        }
    }

    if let Some(feature) = compatibility
        .required_host_features
        .iter()
        .find(|feature| !SUPPORTED_SERVICE_MODULE_FEATURES.contains(&feature.as_str()))
    {
        return Some(AdminModuleRegistrySnapshotIssueDto {
            group: "Compatibility".to_owned(),
            message: format!("{module_name} requires unsupported host feature {feature}"),
            fix: format!("upgrade Lenso or install a compatible {module_name} service release"),
        });
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
        provided_by: None,
        service_manifest: None,
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

#[utoipa::path(
    get,
    path = "/admin/data/{module}/queries/{query}",
    operation_id = "admin_data_query_value",
    tag = "admin-data",
    params(
        ("module" = String, Path, description = "Module name, e.g. remote-crm"),
        ("query" = String, Path, description = "Declared admin query name"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Query result", body = AdminQueryResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown module or undeclared query", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Query source is unavailable or failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn query_value(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, query)): Path<(String, String)>,
) -> Result<Json<AdminQueryResponse>, ApiErrorResponse> {
    let admin_module = find_loaded_query_module(&module, &request_ctx)?;
    let declaration = declared_query(&admin_module, &query, &request_ctx)?;
    ensure_query_capability(&admin, &declaration.capability, &request_ctx)?;
    let query_source = admin_module.query_source.as_ref().ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("module {module} has no admin query source"),
            )
            .retryable(),
            &request_ctx,
        )
    })?;
    let data = query_source
        .query(&query)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    Ok(Json(AdminQueryResponse { data }))
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

#[derive(Debug, Clone)]
struct DeclaredQuery {
    capability: String,
}

fn declared_query(
    module: &AdminModule,
    query: &str,
    ctx: &RequestContext,
) -> Result<DeclaredQuery, ApiErrorResponse> {
    let Some(AdminSurface::DeclarativeCustom(surface)) = module.admin.as_ref() else {
        return Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown query: {query}")),
            ctx,
        ));
    };

    for page in &surface.pages {
        for section in &page.sections {
            let AdminDeclarativeComponent::QueryValue {
                capability,
                query: declared,
                ..
            } = &section.component
            else {
                continue;
            };
            if declared == query {
                return Ok(DeclaredQuery {
                    capability: capability.clone(),
                });
            }
        }
    }

    Err(ApiErrorResponse::with_context(
        AppError::new(ErrorCode::NotFound, format!("unknown query: {query}")),
        ctx,
    ))
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
        AdminActor::Service { scopes, .. } | AdminActor::User { scopes, .. }
            if scopes.iter().any(|scope| scope == capability) =>
        {
            Ok(())
        }
        AdminActor::Service { .. } | AdminActor::User { .. } => {
            Err(ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Forbidden,
                    format!("missing admin action capability: {capability}"),
                ),
                ctx,
            ))
        }
    }
}

fn ensure_query_capability(
    admin: &AdminActor,
    capability: &str,
    ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    match admin {
        AdminActor::System => Ok(()),
        AdminActor::Service { scopes, .. } | AdminActor::User { scopes, .. }
            if scopes.iter().any(|scope| scope == capability) =>
        {
            Ok(())
        }
        AdminActor::Service { .. } | AdminActor::User { .. } => {
            Err(ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Forbidden,
                    format!("missing admin query capability: {capability}"),
                ),
                ctx,
            ))
        }
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
