use crate::{RemoteHttpProxyMatch, RemoteHttpProxyRegistry};
use axum::Json;
use axum::extract::Path;
use axum::http::HeaderMap;
use platform_core::{AppError, ErrorCode};
use platform_http::{
    AdminActor, ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext,
    OpenApiRouter, routes,
};
use platform_module::ModuleHttpMethod;
use serde::{Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use utoipa::ToSchema;

static REMOTE_HTTP_PROXY_REGISTRY: OnceLock<RwLock<Arc<RemoteHttpProxyRegistry>>> = OnceLock::new();

#[derive(Debug, Serialize, ToSchema)]
pub struct RemoteHttpProxyResponse {
    pub status: RemoteHttpProxyStatus,
    pub module_name: String,
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub remote_path: String,
    pub capability: String,
    pub path_params: BTreeMap<String, String>,
    pub data: Value,
}

#[derive(Debug, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteHttpProxyStatus {
    Forwarded,
}

impl Serialize for RemoteHttpProxyStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Forwarded => serializer.serialize_str("forwarded"),
        }
    }
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
    operation_id = "remote_module_http_proxy_get",
    tag = "modules",
    params(
        ("module" = String, Path, description = "Configured remote module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the remote manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Remote route forwarded through the host.", body = RemoteHttpProxyResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "No configured remote route matched", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Remote module request failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
async fn proxy_get(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
) -> Result<Json<RemoteHttpProxyResponse>, ApiErrorResponse> {
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
    let data = forward_get(&matched, &headers, &request_ctx).await?;
    Ok(Json(RemoteHttpProxyResponse::from_match(matched, data)))
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

async fn forward_get(
    matched: &RemoteHttpProxyMatch,
    headers: &HeaderMap,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    let client = reqwest::Client::new();
    let mut request = client.get(format!(
        "{}/{}",
        matched.base_url.trim_end_matches('/'),
        matched.remote_path.trim_start_matches('/')
    ));

    request = forward_header(request, headers, "accept");
    if let Some(token) = matched.auth_token.as_deref() {
        request = request.bearer_auth(token);
    }
    request = request
        .header("x-request-id", request_ctx.request_id.0.as_str())
        .header("x-correlation-id", request_ctx.correlation_id.0.as_str());
    if let (Some(trace_id), Some(span_id)) = (
        request_ctx.trace.trace_id.as_deref(),
        request_ctx.trace.span_id.as_deref(),
    ) {
        request = request.header("traceparent", format!("00-{trace_id}-{span_id}-01"));
    }

    crate::response::decode_json_response::<Value>(
        request.send().await.map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote HTTP proxy request failed: {error}"),
                )
                .retryable(),
                request_ctx,
            )
        })?,
        "HTTP proxy",
        false,
    )
    .await
    .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::NotFound, "remote HTTP route not found"),
            request_ctx,
        )
    })
}

fn forward_header(
    request: reqwest::RequestBuilder,
    headers: &HeaderMap,
    name: &'static str,
) -> reqwest::RequestBuilder {
    match headers.get(name).and_then(|value| value.to_str().ok()) {
        Some(value) if !value.is_empty() => request.header(name, value),
        _ => request,
    }
}

impl RemoteHttpProxyResponse {
    fn from_match(matched: RemoteHttpProxyMatch, data: Value) -> Self {
        Self {
            status: RemoteHttpProxyStatus::Forwarded,
            module_name: matched.module_name,
            method: matched.method,
            declared_path: matched.declared_path,
            remote_path: matched.remote_path,
            capability: matched.capability.unwrap_or_default(),
            path_params: matched.path_params,
            data,
        }
    }
}
