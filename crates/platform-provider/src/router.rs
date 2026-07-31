use crate::invocation::{self, InvocationContext};
use crate::protocol::{
    ProviderHttpProxyInvokeRequest, ProviderHttpProxyInvokeResponse, ProviderInvocationMode,
    ProviderOperationKind,
};
use crate::request::{ProxyRequestBody, apply_grpc_proxy_request_policy};
use crate::{ProviderHttpProxyMatch, ProviderHttpProxyRegistry};
use axum::Json;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Request};
use platform_core::{
    AppContext, AppError, ErrorCode, ProviderHttpCallRecord, insert_provider_http_call,
};
use platform_http::{
    AdminActor, ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext,
    OpenApiRouter, routes,
};
use platform_module::ModuleHttpMethod;
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};
use utoipa::ToSchema;

static PROVIDER_HTTP_PROXY_REGISTRY: OnceLock<RwLock<Arc<ProviderHttpProxyRegistry>>> =
    OnceLock::new();
const MAX_PROXY_DELETE_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderHttpProxyResponse {
    pub status: ProviderHttpProxyStatus,
    pub module_name: String,
    pub method: ModuleHttpMethod,
    pub declared_path: String,
    pub provider_path: String,
    pub capability: String,
    pub path_params: BTreeMap<String, String>,
    pub data: Value,
}

#[derive(Debug, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHttpProxyStatus {
    Forwarded,
}

impl Serialize for ProviderHttpProxyStatus {
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

pub fn install_provider_http_proxy_registry(registry: ProviderHttpProxyRegistry) {
    let storage = PROVIDER_HTTP_PROXY_REGISTRY
        .get_or_init(|| RwLock::new(Arc::new(ProviderHttpProxyRegistry::from_modules(&[], &[]))));
    *storage
        .write()
        .expect("provider HTTP proxy registry lock poisoned") = Arc::new(registry);
}

fn provider_http_proxy_registry() -> Arc<ProviderHttpProxyRegistry> {
    PROVIDER_HTTP_PROXY_REGISTRY
        .get()
        .map(|storage| {
            storage
                .read()
                .expect("provider HTTP proxy registry lock poisoned")
                .clone()
        })
        .unwrap_or_else(|| Arc::new(ProviderHttpProxyRegistry::from_modules(&[], &[])))
}

#[utoipa::path(
    get,
    path = "/modules/{module}/http/{*path}",
    operation_id = "service_module_http_proxy_get",
    tag = "modules",
    params(
        ("module" = String, Path, description = "Configured Service-provided Module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the Service-provided manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Provider route forwarded through the host.", body = ProviderHttpProxyResponse, content_type = "application/json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 404, description = "No configured Service-provided route matched", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 502, description = "Provider export request failed", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
async fn proxy_get(
    State(ctx): State<AppContext>,
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
) -> Result<Json<ProviderHttpProxyResponse>, ApiErrorResponse> {
    let request_path = format!("/{path}");
    let matched = provider_http_proxy_registry()
        .match_route(&module, ModuleHttpMethod::Get, &request_path)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::NotFound,
                    format!("provider HTTP route not found: {module}{request_path}"),
                ),
                &request_ctx,
            )
        })?;

    ensure_capability(&admin, &matched, &request_ctx)?;
    let data = forward_get(&ctx, &matched, &headers, &request_ctx).await?;
    Ok(Json(ProviderHttpProxyResponse::from_match(matched, data)))
}

#[utoipa::path(
    post,
    path = "/modules/{module}/http/{*path}",
    operation_id = "service_module_http_proxy_post",
    tag = "modules",
    request_body(
        content = Value,
        content_type = "application/json",
        description = "JSON request body forwarded to the matched Service-provided Module route"
    ),
    params(
        ("module" = String, Path, description = "Configured Service-provided Module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the Service-provided manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Provider route forwarded through the host.", body = ProviderHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 404, description = "No configured Service-provided route matched", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 502, description = "Provider export request failed", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
async fn proxy_post(
    State(ctx): State<AppContext>,
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<ProviderHttpProxyResponse>, ApiErrorResponse> {
    proxy_body_method(
        ModuleHttpMethod::Post,
        ctx,
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
    operation_id = "service_module_http_proxy_put",
    tag = "modules",
    request_body(
        content = Value,
        content_type = "application/json",
        description = "JSON request body forwarded to the matched Service-provided Module route"
    ),
    params(
        ("module" = String, Path, description = "Configured Service-provided Module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the Service-provided manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Provider route forwarded through the host.", body = ProviderHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 404, description = "No configured Service-provided route matched", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 502, description = "Provider export request failed", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
async fn proxy_put(
    State(ctx): State<AppContext>,
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<ProviderHttpProxyResponse>, ApiErrorResponse> {
    proxy_body_method(
        ModuleHttpMethod::Put,
        ctx,
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
    operation_id = "service_module_http_proxy_patch",
    tag = "modules",
    request_body(
        content = Value,
        content_type = "application/json",
        description = "JSON request body forwarded to the matched Service-provided Module route"
    ),
    params(
        ("module" = String, Path, description = "Configured Service-provided Module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the Service-provided manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Provider route forwarded through the host.", body = ProviderHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 404, description = "No configured Service-provided route matched", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 502, description = "Provider export request failed", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
async fn proxy_patch(
    State(ctx): State<AppContext>,
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    headers: HeaderMap,
    Path((module, path)): Path<(String, String)>,
    body: Bytes,
) -> Result<Json<ProviderHttpProxyResponse>, ApiErrorResponse> {
    proxy_body_method(
        ModuleHttpMethod::Patch,
        ctx,
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
    operation_id = "service_module_http_proxy_delete",
    tag = "modules",
    params(
        ("module" = String, Path, description = "Configured Service-provided Module name"),
        ("path" = String, Path, description = "Module-local HTTP path matched against the Service-provided manifest"),
        ("authorization" = String, Header, description = "Development service bearer token")
    ),
    responses(
        (status = 200, description = "Provider route forwarded through the host.", body = ProviderHttpProxyResponse, content_type = "application/json"),
        (status = 400, description = "Request body policy rejected the request", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "Service/system authentication or declared capability is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 404, description = "No configured Service-provided route matched", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 502, description = "Provider export request failed", body = ErrorResponse, content_type = "application/problem+json"),
    )
)]
async fn proxy_delete(
    State(ctx): State<AppContext>,
    admin: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module, path)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Json<ProviderHttpProxyResponse>, ApiErrorResponse> {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, MAX_PROXY_DELETE_REQUEST_BYTES)
        .await
        .map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Validation,
                    format!("provider HTTP proxy DELETE request body could not be read: {error}"),
                ),
                &request_ctx,
            )
        })?;
    if !body.is_empty() {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Validation,
                "provider HTTP proxy DELETE request body must be empty",
            ),
            &request_ctx,
        ));
    }

    let request_path = format!("/{path}");
    let matched = provider_http_proxy_registry()
        .match_route(&module, ModuleHttpMethod::Delete, &request_path)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::NotFound,
                    format!("provider HTTP route not found: {module}{request_path}"),
                ),
                &request_ctx,
            )
        })?;

    ensure_capability(&admin, &matched, &request_ctx)?;
    let data = forward_delete(&ctx, &matched, &parts.headers, &request_ctx).await?;
    Ok(Json(ProviderHttpProxyResponse::from_match(matched, data)))
}

async fn proxy_body_method(
    method: ModuleHttpMethod,
    ctx: AppContext,
    admin: AdminActor,
    request_ctx: platform_core::RequestContext,
    headers: HeaderMap,
    module: String,
    path: String,
    body: Bytes,
) -> Result<Json<ProviderHttpProxyResponse>, ApiErrorResponse> {
    let request_path = format!("/{path}");
    let matched = provider_http_proxy_registry()
        .match_route(&module, method, &request_path)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::NotFound,
                    format!("provider HTTP route not found: {module}{request_path}"),
                ),
                &request_ctx,
            )
        })?;

    ensure_capability(&admin, &matched, &request_ctx)?;
    let data = forward_body_method(method, &ctx, &matched, &headers, body, &request_ctx).await?;
    Ok(Json(ProviderHttpProxyResponse::from_match(matched, data)))
}

fn ensure_capability(
    admin: &AdminActor,
    matched: &ProviderHttpProxyMatch,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    let Some(capability) = matched.capability.as_deref() else {
        return Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::Forbidden,
                "provider HTTP route has no declared capability",
            ),
            request_ctx,
        ));
    };

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
                    format!("missing provider HTTP route capability: {capability}"),
                ),
                request_ctx,
            ))
        }
    }
}

#[derive(Debug, Clone)]
struct ProxyForwardRequest<'a> {
    ctx: &'a AppContext,
    matched: &'a ProviderHttpProxyMatch,
    method: ModuleHttpMethod,
    headers: &'a HeaderMap,
    request_ctx: &'a platform_core::RequestContext,
    body: ProxyRequestBody,
}

async fn forward_get(
    ctx: &AppContext,
    matched: &ProviderHttpProxyMatch,
    headers: &HeaderMap,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    forward_proxy_request(ProxyForwardRequest {
        ctx,
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
    ctx: &AppContext,
    matched: &ProviderHttpProxyMatch,
    headers: &HeaderMap,
    body: Bytes,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    forward_proxy_request(ProxyForwardRequest {
        ctx,
        matched,
        method,
        headers,
        request_ctx,
        body: ProxyRequestBody::Json(body),
    })
    .await
}

async fn forward_delete(
    ctx: &AppContext,
    matched: &ProviderHttpProxyMatch,
    headers: &HeaderMap,
    request_ctx: &platform_core::RequestContext,
) -> Result<Value, ApiErrorResponse> {
    forward_proxy_request(ProxyForwardRequest {
        ctx,
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
    forward_provider_invocation(request).await
}

async fn forward_provider_invocation(
    request: ProxyForwardRequest<'_>,
) -> Result<Value, ApiErrorResponse> {
    let started_at = Instant::now();
    let matched = request.matched;
    let request_ctx = request.request_ctx;
    let parts =
        apply_grpc_proxy_request_policy(request.method, request.headers, request_ctx, request.body)
            .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))?;
    let payload = ProviderHttpProxyInvokeRequest {
        request_id: request_ctx.request_id.0.clone(),
        correlation_id: request_ctx.correlation_id.0.clone(),
        module_name: matched.module_name.clone(),
        method: module_http_method_label(request.method).to_owned(),
        declared_path: matched.declared_path.clone(),
        provider_path: matched.provider_path.clone(),
        path_params: matched.path_params.clone(),
        headers: parts.headers,
        body: parts.body,
    };
    let invocation = invocation::build(
        &matched.config,
        ProviderOperationKind::HttpRoute,
        format!("{} {}", payload.method, payload.declared_path),
        "1",
        if matches!(request.method, ModuleHttpMethod::Get) {
            ProviderInvocationMode::ReadOnly
        } else {
            ProviderInvocationMode::Durable
        },
        InvocationContext {
            invocation_id: request_ctx.request_id.0.clone(),
            request_id: request_ctx.request_id.0.clone(),
            attempt: 1,
            actor: request_ctx.actor.clone(),
            correlation_id: request_ctx.correlation_id.0.clone(),
            causation_id: request_ctx.causation_id.clone(),
            trace: request_ctx.trace.clone(),
        },
        serde_json::to_value(payload).map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Internal,
                    format!("encode Provider payload: {error}"),
                ),
                request_ctx,
            )
        })?,
    )
    .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(matched.timeout_ms))
        .build()
        .map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Internal,
                    format!("build Provider client: {error}"),
                ),
                request_ctx,
            )
        })?;
    let outcome = invocation::send(&client, &matched.config, "http:invoke", &invocation)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))?;
    let value = invocation::result(&invocation, outcome)
        .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))?;
    let response: ProviderHttpProxyInvokeResponse =
        serde_json::from_value(value).map_err(|error| {
            ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider HTTP result violated its contract: {error}"),
                ),
                request_ctx,
            )
        })?;
    let provider_status = reqwest::StatusCode::from_u16(response.status_code).map_err(|error| {
        ApiErrorResponse::with_context(
            AppError::new(ErrorCode::ExternalDependency, error.to_string()),
            request_ctx,
        )
    })?;
    record_proxy_call(
        request.ctx,
        matched,
        request_ctx,
        started_at,
        Some(provider_status),
        None,
    )
    .await;
    Ok(response.body.unwrap_or(Value::Null))
}

async fn record_proxy_call(
    ctx: &AppContext,
    matched: &ProviderHttpProxyMatch,
    request_ctx: &platform_core::RequestContext,
    started_at: Instant,
    provider_status: Option<reqwest::StatusCode>,
    error: Option<&AppError>,
) {
    let duration_ms = started_at.elapsed().as_millis().min(i64::MAX as u128) as i64;
    match error {
        Some(error) => {
            tracing::warn!(
                module_name = %matched.module_name,
                declared_path = %matched.declared_path,
                provider_path = %matched.provider_path,
                http_method = %module_http_method_label(matched.method),
                provider_status = provider_status.map_or(0, |status| status.as_u16()),
                duration_ms,
                error_code = error.code.as_str(),
                retryable = error.retryable,
                request_id = %request_ctx.request_id.0,
                correlation_id = %request_ctx.correlation_id.0,
                "provider HTTP proxy call failed"
            );
        }
        None => {
            tracing::info!(
                module_name = %matched.module_name,
                declared_path = %matched.declared_path,
                provider_path = %matched.provider_path,
                http_method = %module_http_method_label(matched.method),
                provider_status = provider_status.map_or(0, |status| status.as_u16()),
                duration_ms,
                request_id = %request_ctx.request_id.0,
                correlation_id = %request_ctx.correlation_id.0,
                "provider HTTP proxy call completed"
            );
        }
    }

    let record = ProviderHttpCallRecord {
        module_name: matched.module_name.clone(),
        method: module_http_method_label(matched.method).to_owned(),
        declared_path: matched.declared_path.clone(),
        provider_path: matched.provider_path.clone(),
        capability: matched.capability.clone(),
        display_name: matched.display_name.clone(),
        story_title: matched.story_title.clone(),
        provider_status: provider_status.map(|status| status.as_u16()),
        duration_ms,
        success: error.is_none(),
        error_code: error.map(|error| error.code.as_str().to_owned()),
        retryable: error.is_some_and(|error| error.retryable),
        path_params: json!(matched.path_params),
        error_details: error
            .map(|error| json!(error.details))
            .unwrap_or_else(|| Value::Array(Vec::new())),
    };

    if let Err(error) =
        insert_provider_http_call(&ctx.db, ctx.ids.as_ref(), request_ctx, record).await
    {
        tracing::warn!(
            error = ?error,
            module_name = %matched.module_name,
            declared_path = %matched.declared_path,
            provider_path = %matched.provider_path,
            http_method = %module_http_method_label(matched.method),
            request_id = %request_ctx.request_id.0,
            correlation_id = %request_ctx.correlation_id.0,
            "failed to persist provider HTTP proxy call"
        );
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

impl ProviderHttpProxyResponse {
    fn from_match(matched: ProviderHttpProxyMatch, data: Value) -> Self {
        Self {
            status: ProviderHttpProxyStatus::Forwarded,
            module_name: matched.module_name,
            method: matched.method,
            declared_path: matched.declared_path,
            provider_path: matched.provider_path,
            capability: matched.capability.unwrap_or_default(),
            path_params: matched.path_params,
            data,
        }
    }
}
