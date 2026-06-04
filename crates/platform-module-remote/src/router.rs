use crate::request::{ProxyRequestBody, apply_proxy_request_policy};
use crate::response::ResponseBodyPolicy;
use crate::{RemoteHttpProxyMatch, RemoteHttpProxyRegistry};
use axum::Json;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::Path;
use axum::http::{HeaderMap, Request};
use platform_core::error::ErrorDetail;
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
use std::time::{Duration, Instant};
use utoipa::ToSchema;

static REMOTE_HTTP_PROXY_REGISTRY: OnceLock<RwLock<Arc<RemoteHttpProxyRegistry>>> = OnceLock::new();
const MAX_PROXY_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PROXY_DELETE_REQUEST_BYTES: usize = 1024 * 1024;

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
    OpenApiRouter::new()
        .routes(routes!(proxy_get))
        .routes(routes!(proxy_post))
        .routes(routes!(proxy_put))
        .routes(routes!(proxy_patch))
        .routes(routes!(proxy_delete))
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

#[utoipa::path(
    post,
    path = "/modules/{module}/http/{*path}",
    operation_id = "remote_module_http_proxy_post",
    tag = "modules",
    request_body(
        content = Value,
        content_type = "application/json",
        description = "JSON request body forwarded to the matched remote module route"
    ),
    params(
        ("module" = String, Path, description = "Configured remote module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the remote manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Remote route forwarded through the host.", body = RemoteHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "No configured remote route matched", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Remote module request failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
async fn proxy_post(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<RemoteHttpProxyResponse>, ApiErrorResponse> {
    proxy_body_method(
        ModuleHttpMethod::Post,
        admin,
        request_ctx,
        headers,
        module,
        path,
        body,
    )
    .await
}

#[utoipa::path(
    put,
    path = "/modules/{module}/http/{*path}",
    operation_id = "remote_module_http_proxy_put",
    tag = "modules",
    request_body(
        content = Value,
        content_type = "application/json",
        description = "JSON request body forwarded to the matched remote module route"
    ),
    params(
        ("module" = String, Path, description = "Configured remote module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the remote manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Remote route forwarded through the host.", body = RemoteHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "No configured remote route matched", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Remote module request failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
async fn proxy_put(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<RemoteHttpProxyResponse>, ApiErrorResponse> {
    proxy_body_method(
        ModuleHttpMethod::Put,
        admin,
        request_ctx,
        headers,
        module,
        path,
        body,
    )
    .await
}

#[utoipa::path(
    patch,
    path = "/modules/{module}/http/{*path}",
    operation_id = "remote_module_http_proxy_patch",
    tag = "modules",
    request_body(
        content = Value,
        content_type = "application/json",
        description = "JSON request body forwarded to the matched remote module route"
    ),
    params(
        ("module" = String, Path, description = "Configured remote module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the remote manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Remote route forwarded through the host.", body = RemoteHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "No configured remote route matched", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Remote module request failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
async fn proxy_patch(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<RemoteHttpProxyResponse>, ApiErrorResponse> {
    proxy_body_method(
        ModuleHttpMethod::Patch,
        admin,
        request_ctx,
        headers,
        module,
        path,
        body,
    )
    .await
}

#[utoipa::path(
    delete,
    path = "/modules/{module}/http/{*path}",
    operation_id = "remote_module_http_proxy_delete",
    tag = "modules",
    params(
        ("module" = String, Path, description = "Configured remote module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the remote manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Remote route forwarded through the host.", body = RemoteHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/json"),
        (status = 404, description = "No configured remote route matched", body = ErrorResponse, content_type = "application/json"),
        (status = 502, description = "Remote module request failed", body = ErrorResponse, content_type = "application/json"),
    )
)]
async fn proxy_delete(
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, path)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Json<RemoteHttpProxyResponse>, ApiErrorResponse> {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, MAX_PROXY_DELETE_REQUEST_BYTES)
        .await
        .map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Validation,
                    format!("remote HTTP proxy DELETE request body could not be read: {error}"),
                ),
                &request_ctx,
            )
        })?;
    if !body.is_empty() {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Validation,
                "remote HTTP proxy DELETE request body must be empty",
            ),
            &request_ctx,
        ));
    }

    let request_path = format!("/{path}");
    let matched = remote_http_proxy_registry()
        .match_route(&module, ModuleHttpMethod::Delete, &request_path)
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
    let data = forward_delete(&matched, &parts.headers, &request_ctx).await?;
    Ok(Json(RemoteHttpProxyResponse::from_match(matched, data)))
}

async fn proxy_body_method(
    method: ModuleHttpMethod,
    admin: AdminActor,
    request_ctx: platform_core::RequestContext,
    headers: HeaderMap,
    module: String,
    path: String,
    body: Bytes,
) -> Result<Json<RemoteHttpProxyResponse>, ApiErrorResponse> {
    let request_path = format!("/{path}");
    let matched = remote_http_proxy_registry()
        .match_route(&module, method, &request_path)
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
    let data = forward_body_method(method, &matched, &headers, body, &request_ctx).await?;
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

#[derive(Debug, Clone)]
struct ProxyForwardRequest<'a> {
    matched: &'a RemoteHttpProxyMatch,
    method: ModuleHttpMethod,
    headers: &'a HeaderMap,
    request_ctx: &'a platform_core::RequestContext,
    body: ProxyRequestBody,
}

async fn forward_get(
    matched: &RemoteHttpProxyMatch,
    headers: &HeaderMap,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    forward_proxy_request(ProxyForwardRequest {
        matched,
        method: ModuleHttpMethod::Get,
        headers,
        request_ctx,
        body: ProxyRequestBody::Empty,
    })
    .await
}

async fn forward_body_method(
    method: ModuleHttpMethod,
    matched: &RemoteHttpProxyMatch,
    headers: &HeaderMap,
    body: Bytes,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    forward_proxy_request(ProxyForwardRequest {
        matched,
        method,
        headers,
        request_ctx,
        body: ProxyRequestBody::Json(body),
    })
    .await
}

async fn forward_delete(
    matched: &RemoteHttpProxyMatch,
    headers: &HeaderMap,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    forward_proxy_request(ProxyForwardRequest {
        matched,
        method: ModuleHttpMethod::Delete,
        headers,
        request_ctx,
        body: ProxyRequestBody::Empty,
    })
    .await
}

async fn forward_proxy_request(
    request: ProxyForwardRequest<'_>,
) -> Result<Value, ApiErrorResponse> {
    let matched = request.matched;
    let request_ctx = request.request_ctx;
    let started_at = Instant::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(matched.timeout_ms))
        .build()
        .map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build remote HTTP proxy client: {error}"),
                ),
                request_ctx,
            )
        })?;
    let outbound = client.request(reqwest_method(request.method), remote_url(matched));
    let outbound = apply_proxy_request_policy(
        outbound,
        request.method,
        request.headers,
        request_ctx,
        matched.auth_token.as_deref(),
        request.body,
    )
    .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))?;

    let response = outbound.send().await.map_err(|error| {
        let app_error = AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote HTTP proxy request failed: {error}"),
        )
        .retryable();
        let app_error = with_proxy_error_details(app_error, matched, request.method, None);
        record_proxy_call(matched, request_ctx, started_at, None, Some(&app_error));
        ApiErrorResponse::with_context(app_error, request_ctx)
    })?;
    let remote_status = response.status();

    match crate::response::decode_json_response_with_policy::<Value>(
        response,
        "HTTP proxy",
        false,
        ResponseBodyPolicy {
            max_bytes: Some(MAX_PROXY_RESPONSE_BYTES),
            require_json_content_type: true,
            allow_empty_success: request.method == ModuleHttpMethod::Delete,
        },
    )
    .await
    {
        Ok(Some(data)) => {
            record_proxy_call(matched, request_ctx, started_at, Some(remote_status), None);
            Ok(data)
        }
        Ok(None) => {
            if request.method == ModuleHttpMethod::Delete && remote_status.is_success() {
                record_proxy_call(matched, request_ctx, started_at, Some(remote_status), None);
                Ok(Value::Null)
            } else {
                let app_error = AppError::new(ErrorCode::NotFound, "remote HTTP route not found");
                let app_error = with_proxy_error_details(
                    app_error,
                    matched,
                    request.method,
                    Some(remote_status),
                );
                record_proxy_call(
                    matched,
                    request_ctx,
                    started_at,
                    Some(remote_status),
                    Some(&app_error),
                );
                Err(ApiErrorResponse::with_context(app_error, request_ctx))
            }
        }
        Err(error) => {
            let error =
                with_proxy_error_details(error, matched, request.method, Some(remote_status));
            record_proxy_call(
                matched,
                request_ctx,
                started_at,
                Some(remote_status),
                Some(&error),
            );
            Err(ApiErrorResponse::with_context(error, request_ctx))
        }
    }
}

fn with_proxy_error_details(
    mut error: AppError,
    matched: &RemoteHttpProxyMatch,
    method: ModuleHttpMethod,
    remote_status: Option<reqwest::StatusCode>,
) -> AppError {
    push_error_detail(&mut error, "remote_module", matched.module_name.clone());
    push_error_detail(
        &mut error,
        "remote_method",
        module_http_method_label(method),
    );
    push_error_detail(&mut error, "declared_path", matched.declared_path.clone());
    push_error_detail(&mut error, "remote_path", matched.remote_path.clone());
    if let Some(status) = remote_status {
        push_error_detail(&mut error, "remote_status", status.as_u16().to_string());
    }
    error
}

fn push_error_detail(error: &mut AppError, field: &'static str, reason: impl Into<String>) {
    if error
        .details
        .iter()
        .any(|detail| detail.field.as_deref() == Some(field))
    {
        return;
    }
    error.details.push(ErrorDetail {
        field: Some(field.to_owned()),
        reason: reason.into(),
    });
}

fn remote_url(matched: &RemoteHttpProxyMatch) -> String {
    format!(
        "{}/{}",
        matched.base_url.trim_end_matches('/'),
        matched.remote_path.trim_start_matches('/')
    )
}

fn reqwest_method(method: ModuleHttpMethod) -> reqwest::Method {
    match method {
        ModuleHttpMethod::Get => reqwest::Method::GET,
        ModuleHttpMethod::Post => reqwest::Method::POST,
        ModuleHttpMethod::Put => reqwest::Method::PUT,
        ModuleHttpMethod::Patch => reqwest::Method::PATCH,
        ModuleHttpMethod::Delete => reqwest::Method::DELETE,
        _ => reqwest::Method::GET,
    }
}

fn record_proxy_call(
    matched: &RemoteHttpProxyMatch,
    request_ctx: &platform_core::RequestContext,
    started_at: Instant,
    remote_status: Option<reqwest::StatusCode>,
    error: Option<&AppError>,
) {
    let duration_ms = started_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
    match error {
        Some(error) => {
            tracing::warn!(
                module_name = %matched.module_name,
                declared_path = %matched.declared_path,
                remote_path = %matched.remote_path,
                http_method = %module_http_method_label(matched.method),
                remote_status = remote_status.map_or(0, |status| status.as_u16()),
                duration_ms,
                error_code = error.code.as_str(),
                retryable = error.retryable,
                request_id = %request_ctx.request_id.0,
                correlation_id = %request_ctx.correlation_id.0,
                "remote HTTP proxy call failed"
            );
        }
        None => {
            tracing::info!(
                module_name = %matched.module_name,
                declared_path = %matched.declared_path,
                remote_path = %matched.remote_path,
                http_method = %module_http_method_label(matched.method),
                remote_status = remote_status.map_or(0, |status| status.as_u16()),
                duration_ms,
                request_id = %request_ctx.request_id.0,
                correlation_id = %request_ctx.correlation_id.0,
                "remote HTTP proxy call completed"
            );
        }
    }
}

fn module_http_method_label(method: ModuleHttpMethod) -> &'static str {
    match method {
        ModuleHttpMethod::Get => "GET",
        ModuleHttpMethod::Post => "POST",
        ModuleHttpMethod::Put => "PUT",
        ModuleHttpMethod::Patch => "PATCH",
        ModuleHttpMethod::Delete => "DELETE",
        _ => "UNKNOWN",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn matched(method: ModuleHttpMethod) -> RemoteHttpProxyMatch {
        RemoteHttpProxyMatch {
            module_name: "remote-crm".to_owned(),
            base_url: "http://127.0.0.1:4100/lenso/module/v1/".to_owned(),
            timeout_ms: 5_000,
            auth_token: None,
            method,
            declared_path: "/contacts/{id}".to_owned(),
            remote_path: "/contacts/contact_1".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            path_params: BTreeMap::new(),
        }
    }

    #[test]
    fn remote_url_joins_base_and_remote_path_once() {
        assert_eq!(
            remote_url(&matched(ModuleHttpMethod::Get)),
            "http://127.0.0.1:4100/lenso/module/v1/contacts/contact_1"
        );
    }

    #[test]
    fn reqwest_method_maps_declared_methods() {
        assert_eq!(reqwest_method(ModuleHttpMethod::Get), reqwest::Method::GET);
        assert_eq!(
            reqwest_method(ModuleHttpMethod::Post),
            reqwest::Method::POST
        );
        assert_eq!(reqwest_method(ModuleHttpMethod::Put), reqwest::Method::PUT);
        assert_eq!(
            reqwest_method(ModuleHttpMethod::Patch),
            reqwest::Method::PATCH
        );
        assert_eq!(
            reqwest_method(ModuleHttpMethod::Delete),
            reqwest::Method::DELETE
        );
    }
}
