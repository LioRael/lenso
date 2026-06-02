#[allow(clippy::wildcard_imports)]
use super::*;
use crate::config_dto::*;
use axum::Json;
use axum::extract::{Path, Query, State};
use platform_core::settings::store::{delete_value, load_audit, upsert_value};
use platform_core::{AppContext, AppError, ErrorCode};
use platform_http::{AdminActor, ApiErrorResponse, ErrorResponse, HttpRequestContext};

const AUDIT_DEFAULT_LIMIT: i64 = 50;
const AUDIT_MAX_LIMIT: i64 = 200;

fn actor_label(actor: &AdminActor) -> String {
    match actor {
        AdminActor::Service { service_id, .. } => format!("service:{service_id}"),
        AdminActor::System => "system".to_owned(),
    }
}

#[utoipa::path(
    get,
    path = "/admin/config/descriptors",
    operation_id = "admin_config_list_descriptors",
    tag = "admin-config",
    params(
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Registered setting descriptors", body = ConfigDescriptorListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_config_descriptors(
    _admin: AdminActor,
    State(_ctx): State<AppContext>,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<ConfigDescriptorListResponse>, ApiErrorResponse> {
    let data = settings_registry()
        .iter()
        .map(|d| ConfigDescriptorDto {
            key: d.key.to_owned(),
            service: d.scope.as_service_key().to_owned(),
            value_type: d.value_type.to_json(),
            default: d.default.clone(),
            editable: d.editable,
            restart_only: d.restart_only,
            description: d.description.to_owned(),
        })
        .collect();
    Ok(Json(ConfigDescriptorListResponse { data }))
}

#[utoipa::path(
    get,
    path = "/admin/config/values",
    operation_id = "admin_config_list_values",
    tag = "admin-config",
    params(
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Effective config values", body = ConfigValueListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service or system authentication is required", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn list_config_values(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(_request_ctx): HttpRequestContext,
) -> Result<Json<ConfigValueListResponse>, ApiErrorResponse> {
    let snapshot = ctx.settings.snapshot();
    let data = snapshot
        .entries()
        .map(|(key, value, source)| ConfigValueDto {
            key: key.to_owned(),
            value: value.clone(),
            source: serde_json::to_value(source)
                .ok()
                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "default".to_owned()),
        })
        .collect();
    Ok(Json(ConfigValueListResponse { data }))
}

#[utoipa::path(
    put,
    path = "/admin/config/{service}/{key}",
    operation_id = "admin_config_put_value",
    tag = "admin-config",
    params(
        ("service" = String, Path, description = "Service key: a service name or `*` for shared"),
        ("key" = String, Path, description = "Setting key"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    request_body = ConfigWriteRequest,
    responses(
        (status = 200, description = "Value written", body = ConfigWriteResponse, content_type = "application/json"),
        (status = 400, description = "Value failed validation", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Setting is not editable", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown setting key", body = ErrorResponse, content_type = "application/json"),
        (status = 500, description = "Internal server error", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn put_config_value(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((service, key)): Path<(String, String)>,
    Json(body): Json<ConfigWriteRequest>,
) -> Result<Json<ConfigWriteResponse>, ApiErrorResponse> {
    let descriptor = settings_registry().get_raw(&service, &key).ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown setting `{service}:{key}`")),
            &request_ctx,
        )
    })?;

    if !descriptor.editable {
        return Err(ApiErrorResponse::with_context(
            AppError::new(ErrorCode::Forbidden, format!("setting `{key}` is not editable")),
            &request_ctx,
        ));
    }

    descriptor
        .validate(&body.value)
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    let actor = actor_label(&admin);
    let stored = upsert_value(&ctx.db, &service, &key, &body.value, Some(&actor))
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;

    let applies_on_restart = descriptor.restart_only;

    notify_config_changed(&ctx, &service, &key, &request_ctx).await?;

    tracing::info!(
        actor = %actor,
        service = %service,
        key = %key,
        "config value updated"
    );

    Ok(Json(ConfigWriteResponse {
        key: stored.key,
        service: stored.service,
        value: stored.value,
        updated_at: stored.updated_at,
        updated_by: stored.updated_by,
        applies_on_restart,
    }))
}

#[utoipa::path(
    delete,
    path = "/admin/config/{service}/{key}",
    operation_id = "admin_config_delete_value",
    tag = "admin-config",
    params(
        ("service" = String, Path, description = "Service key: a service name or `*` for shared"),
        ("key" = String, Path, description = "Setting key"),
        ("authorization" = String, Header, description = "Development service bearer token"),
    ),
    responses(
        (status = 200, description = "Value reset to default", body = ConfigWriteResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "Unknown setting key", body = ErrorResponse, content_type = "application/json"),
        (status = 500, description = "Internal server error", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn delete_config_value(
    admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((service, key)): Path<(String, String)>,
) -> Result<Json<ConfigWriteResponse>, ApiErrorResponse> {
    let descriptor = settings_registry().get_raw(&service, &key).ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, format!("unknown setting `{service}:{key}`")),
            &request_ctx,
        )
    })?;
    let restart_only = descriptor.restart_only;
    let default_value = descriptor.default.clone();
    let actor = actor_label(&admin);
    delete_value(&ctx.db, &service, &key, Some(&actor))
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    notify_config_changed(&ctx, &service, &key, &request_ctx).await?;

    Ok(Json(ConfigWriteResponse {
        key,
        service,
        value: default_value,
        updated_at: chrono::Utc::now(),
        updated_by: Some(actor),
        applies_on_restart: restart_only,
    }))
}

#[utoipa::path(
    get,
    path = "/admin/config/{service}/{key}/audit",
    operation_id = "admin_config_get_audit",
    tag = "admin-config",
    params(
        ("service" = String, Path, description = "Service key"),
        ("key" = String, Path, description = "Setting key"),
        ("authorization" = String, Header, description = "Development service bearer token"),
        ConfigAuditQuery
    ),
    responses(
        (status = 200, description = "Audit history", body = ConfigAuditListResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 500, description = "Internal server error", body = ErrorResponse, content_type = "application/json"),
    )
)]
pub(crate) async fn get_config_audit(
    _admin: AdminActor,
    State(ctx): State<AppContext>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((service, key)): Path<(String, String)>,
    Query(query): Query<ConfigAuditQuery>,
) -> Result<Json<ConfigAuditListResponse>, ApiErrorResponse> {
    let limit = query.limit.unwrap_or(AUDIT_DEFAULT_LIMIT).clamp(1, AUDIT_MAX_LIMIT);
    let entries = load_audit(&ctx.db, &service, &key, limit)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
    let data = entries
        .into_iter()
        .map(|e| ConfigAuditDto {
            service: e.service,
            key: e.key,
            old_value: e.old_value,
            new_value: e.new_value,
            actor: e.actor,
            changed_at: e.changed_at,
        })
        .collect();
    Ok(Json(ConfigAuditListResponse { data }))
}

/// Emit a `config_changed` notification so every instance refreshes.
async fn notify_config_changed(
    ctx: &AppContext,
    service: &str,
    key: &str,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    let payload = format!("{service}:{key}");
    sqlx::query("select pg_notify('config_changed', $1)")
        .bind(payload)
        .execute(&ctx.db)
        .await
        .map_err(|source| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::Internal, "config notify failed").with_source(source),
                request_ctx,
            )
        })?;
    Ok(())
}
