use crate::dto::{
    AdminDataDetailResponse, AdminDataListResponse, AdminDataPageInfo, AdminModuleMetadataDto,
    AdminModuleMetadataListResponse, AdminModuleSchema, AdminModuleStatus, AdminSchemaListResponse,
    AdminSchemaRefreshResponse,
};
use crate::{
    AdminModule, AdminModuleMetadata, admin_metadata_refresher, admin_module_metadata_snapshot,
    admin_modules, admin_refresher, find_loaded_module, install_admin_module_metadata,
    install_admin_modules, record_admin_module_metadata_refresh_error,
};
use axum::Json;
use axum::extract::{Path, Query};
use platform_core::{AppError, ErrorCode, RequestContext};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};
use platform_module::{AdminListQuery, ModuleLoadStatus, lint_module_manifest_parts};
use serde::Deserialize;

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
    match refresher.refresh_admin_module_metadata().await {
        Ok(metadata) => {
            install_admin_module_metadata(metadata);
            Ok(Json(metadata_response(admin_module_metadata_snapshot())))
        }
        Err(error) => Ok(Json(metadata_response(
            record_admin_module_metadata_refresh_error(error.public_message),
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
        .map(|m| AdminModuleMetadataDto {
            module_name: m.module_name.clone(),
            source: m.source,
            status: admin_module_status(&m.load_status),
            error: load_error_message(&m.load_status),
            http_routes: m.http_routes.clone(),
            manifest_lints: lint_module_manifest_parts(
                m.source,
                &m.module_name,
                m.admin.as_ref(),
                &m.http_routes,
                &m.capabilities,
            ),
            story_display: m
                .story_display
                .clone()
                .into_iter()
                .map(Into::into)
                .collect(),
            capabilities: m.capabilities.clone(),
            admin: m.admin.clone(),
        })
        .collect()
}

fn metadata_response(
    snapshot: crate::AdminModuleMetadataSnapshot,
) -> AdminModuleMetadataListResponse {
    AdminModuleMetadataListResponse {
        modules: metadata_response_modules(snapshot.modules),
        refreshed_at: snapshot.refreshed_at,
        refresh_error: snapshot.refresh_error,
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

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleHttpMethod, ModuleHttpRoute, ModuleSource};

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
            story_display: Vec::new(),
            capabilities: Vec::new(),
            admin: None,
        }]);

        assert_eq!(modules[0].manifest_lints.len(), 1);
        assert_eq!(
            modules[0].manifest_lints[0].message,
            "Missing capability declaration for host proxy authorization."
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
