use crate::{RemoteHttpProxyMatch, RemoteHttpProxyRegistry};
use axum::Json;
use axum::extract::Path;
use platform_core::{AppError, ErrorCode};
use platform_http::{
    AdminActor, ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext,
    OpenApiRouter, routes,
};
use platform_module::ModuleHttpMethod;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use utoipa::ToSchema;

static REMOTE_HTTP_PROXY_REGISTRY: OnceLock<RwLock<Arc<RemoteHttpProxyRegistry>>> = OnceLock::new();

#[derive(Debug, Serialize, ToSchema)]
pub struct RemoteHttpProxySkeletonResponse {
    pub status: RemoteHttpProxySkeletonStatus,
    pub module_name: String,
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub remote_path: String,
    pub capability: String,
    pub path_params: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteHttpProxySkeletonStatus {
    Matched,
}

#[must_use]
pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new().routes(routes!(proxy_get))
}

pub fn install_remote_http_proxy_registry(registry: RemoteHttpProxyRegistry) {
    let storage = REMOTE_HTTP_PROXY_REGISTRY
        .get_or_init(|| RwLock::new(Arc::new(RemoteHttpProxyRegistry::from_modules(&[], &[]))));
    *storage
        .write()
        .expect("remote HTTP proxy registry lock poisoned") = Arc::new(registry);
}

fn remote_http_proxy_registry() -> Arc<RemoteHttpProxyRegistry> {
    REMOTE_HTTP_PROXY_REGISTRY
        .get()
        .map(|storage| {
            storage
                .read()
                .expect("remote HTTP proxy registry lock poisoned")
                .clone()
        })
        .unwrap_or_else(|| Arc::new(RemoteHttpProxyRegistry::from_modules(&[], &[])))
}

#[utoipa::path(
    get,
    path = "/modules/{module}/http/{*path}",
    operation_id = "remote_module_http_proxy_get_skeleton",
    tag = "modules",
    params(
        ("module" = String, Path, description = "Configured remote module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the remote manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Remote route matched by the host skeleton. No remote request is forwarded yet.", body = RemoteHttpProxySkeletonResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "No configured remote route matched", body = ErrorResponse, content_type = "application/json"),
    )
)]
async fn proxy_get(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, path)): Path<(String, String)>,
) -> Result<Json<RemoteHttpProxySkeletonResponse>, ApiErrorResponse> {
    let request_path = format!("/{path}");
    let matched = remote_http_proxy_registry()
        .match_route(&module, ModuleHttpMethod::Get, &request_path)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::NotFound,
                    format!("remote HTTP route not found: {module}{request_path}"),
                ),
                &request_ctx,
            )
        })?;

    ensure_capability(&admin, &matched, &request_ctx)?;
    Ok(Json(RemoteHttpProxySkeletonResponse::from_match(matched)))
}

fn ensure_capability(
    admin: &AdminActor,
    matched: &RemoteHttpProxyMatch,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    let Some(capability) = matched.capability.as_deref() else {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                "remote HTTP route has no declared capability",
            ),
            request_ctx,
        ));
    };

    match admin {
        AdminActor::System => Ok(()),
        AdminActor::Service { scopes, .. } if scopes.iter().any(|scope| scope == capability) => {
            Ok(())
        }
        AdminActor::Service { .. } => Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                format!("missing remote HTTP route capability: {capability}"),
            ),
            request_ctx,
        )),
    }
}

impl RemoteHttpProxySkeletonResponse {
    fn from_match(matched: RemoteHttpProxyMatch) -> Self {
        Self {
            status: RemoteHttpProxySkeletonStatus::Matched,
            module_name: matched.module_name,
            method: matched.method,
            declared_path: matched.declared_path,
            remote_path: matched.remote_path,
            capability: matched.capability.unwrap_or_default(),
            path_params: matched.path_params,
        }
    }
}
