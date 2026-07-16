use crate::{
    AuthenticatedServiceContext, AuthenticatedServicePrincipal, AuthenticatedTransportBinding,
    CallPolicyDeclaration, CallPolicyEvent, CallPolicyEvidence, CallPolicyFailure,
    CallPolicyRuntime, CallPolicyTerminalOutcome, DelegatedActorContext, DelegatedContextProvider,
    EndpointResolver, IdentityDecisionRecorder, ServiceContext, ServiceContextAdmission,
    ServiceContextPolicy, ServiceReference, TenantContext, WorkloadIdentityProvider,
    WorkloadIdentityVerification,
};
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DEADLINE_HEADER: &str = "x-lenso-deadline-unix-ms";
const IDEMPOTENCY_HEADER: &str = "idempotency-key";
const AUTHORIZATION_HEADER: &str = "authorization";
const DELEGATED_ACTOR_HEADER: &str = "x-lenso-delegated-actor";
const TENANT_CONTEXT_HEADER: &str = "x-lenso-tenant-context";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpIdempotency {
    Unknown,
    Idempotent,
    RequiresKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectHttpOperation {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub idempotency: HttpIdempotency,
    pub call_policy: CallPolicyDeclaration,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<Value>,
    pub response_schemas: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_error_schema: Option<Value>,
}

impl DirectHttpOperation {
    #[must_use]
    pub fn no_retry_reason(&self) -> Option<&'static str> {
        match self.idempotency {
            HttpIdempotency::Unknown => Some("operation_retry_safety_unknown"),
            HttpIdempotency::Idempotent | HttpIdempotency::RequiresKey => None,
        }
    }

    #[must_use]
    pub fn retry_decision(&self, status: StatusCode, attempt: u32) -> RetryDecision {
        self.retry_decision_for(status, attempt, None)
    }

    fn retry_decision_for(
        &self,
        status: StatusCode,
        attempt: u32,
        idempotency_key: Option<&str>,
    ) -> RetryDecision {
        if self.idempotency == HttpIdempotency::Unknown {
            return RetryDecision::no("operation_retry_safety_unknown");
        }
        if attempt >= self.call_policy.max_attempts {
            return RetryDecision::no("initial_policy_attempt_limit");
        }
        if !matches!(status.as_u16(), 429 | 502 | 503 | 504) {
            return RetryDecision::no("failure_not_retryable");
        }
        match self.idempotency {
            HttpIdempotency::Idempotent => RetryDecision::yes(),
            HttpIdempotency::RequiresKey if idempotency_key.is_some_and(|key| !key.is_empty()) => {
                RetryDecision::yes()
            }
            HttpIdempotency::RequiresKey => RetryDecision::no("idempotency_key_required"),
            HttpIdempotency::Unknown => unreachable!("unknown safety returns before matching"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectHttpBindings {
    pub contract_id: String,
    pub version: String,
    pub operations: Vec<DirectHttpOperation>,
}

impl DirectHttpBindings {
    #[must_use]
    pub fn operation(&self, operation_id: &str) -> Option<&DirectHttpOperation> {
        self.operations
            .iter()
            .find(|item| item.operation_id == operation_id)
    }

    fn match_request(&self, method: &Method, path: &str) -> Option<&DirectHttpOperation> {
        self.operations.iter().find(|operation| {
            operation.method == method.as_str() && path_matches(&operation.path, path)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingGenerationError(pub String);

impl std::fmt::Display for BindingGenerationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BindingGenerationError {}

pub fn generate_direct_http_bindings(
    contract_id: impl Into<String>,
    version: impl Into<String>,
    openapi: &Value,
) -> Result<DirectHttpBindings, BindingGenerationError> {
    let version = version.into();
    let document_version = openapi.pointer("/info/version").and_then(Value::as_str);
    if document_version != Some(version.as_str()) {
        return Err(BindingGenerationError(
            "OpenAPI info.version must match the Service Contract version".to_owned(),
        ));
    }
    let paths = openapi
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            BindingGenerationError("OpenAPI Service Contract requires paths".to_owned())
        })?;
    let mut operations = Vec::new();
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = item.get(method).and_then(Value::as_object) else {
                continue;
            };
            let operation_id = operation
                .get("operationId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    BindingGenerationError(format!("{method} {path} requires operationId"))
                })?;
            let idempotency = match operation.get("x-lenso-idempotency").and_then(Value::as_str) {
                Some("idempotent") => HttpIdempotency::Idempotent,
                Some("requires_key") => HttpIdempotency::RequiresKey,
                Some(value) => {
                    return Err(BindingGenerationError(format!(
                        "unsupported x-lenso-idempotency `{value}`"
                    )));
                }
                None => HttpIdempotency::Unknown,
            };
            let retry_safe = idempotency != HttpIdempotency::Unknown;
            let call_policy = operation
                .get("x-lenso-call-policy")
                .ok_or_else(|| {
                    BindingGenerationError(format!("{method} {path} requires x-lenso-call-policy"))
                })
                .and_then(|value| {
                    serde_json::from_value::<CallPolicyDeclaration>(value.clone()).map_err(
                        |error| {
                            BindingGenerationError(format!(
                                "{method} {path} has invalid x-lenso-call-policy: {error}"
                            ))
                        },
                    )
                })?;
            if let Some(issue) = call_policy.validate(retry_safe).into_iter().next() {
                return Err(BindingGenerationError(format!(
                    "{method} {path} x-lenso-call-policy.{}: {}",
                    issue.path, issue.code
                )));
            }
            operations.push(DirectHttpOperation {
                operation_id: operation_id.to_owned(),
                method: method.to_uppercase(),
                path: path.clone(),
                idempotency,
                call_policy,
                request_schema: operation
                    .get("requestBody")
                    .and_then(|value| value.pointer("/content/application~1json/schema"))
                    .map(|schema| resolve_local_schema(openapi, schema)),
                response_schemas: response_schemas(openapi, operation),
                standard_error_schema: operation
                    .get("responses")
                    .and_then(Value::as_object)
                    .and_then(|responses| {
                        responses.values().find_map(|response| {
                            response
                                .pointer("/content/application~1problem+json/schema")
                                .map(|schema| resolve_local_schema(openapi, schema))
                        })
                    }),
            });
        }
    }
    operations.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    Ok(DirectHttpBindings {
        contract_id: contract_id.into(),
        version,
        operations,
    })
}

#[derive(Debug, Clone)]
pub struct DirectHttpRequest {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub deadline_unix_ms: Option<u64>,
    pub idempotency_key: Option<String>,
    pub workload_credential: Option<String>,
    pub authenticated_transport_binding: Option<String>,
    pub authenticated_service_principal: Option<AuthenticatedServicePrincipal>,
    pub delegated_actor_context: Option<DelegatedActorContext>,
    pub tenant_context: Option<TenantContext>,
    service_context_decode_failed: bool,
    pub authenticated_service_context: Option<AuthenticatedServiceContext>,
}

impl DirectHttpRequest {
    #[must_use]
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
            deadline_unix_ms: None,
            idempotency_key: None,
            workload_credential: None,
            authenticated_transport_binding: None,
            authenticated_service_principal: None,
            delegated_actor_context: None,
            tenant_context: None,
            service_context_decode_failed: false,
            authenticated_service_context: None,
        }
    }

    #[must_use]
    pub fn with_deadline(mut self, deadline_unix_ms: u64) -> Self {
        self.deadline_unix_ms = Some(deadline_unix_ms);
        self
    }

    #[must_use]
    pub fn with_workload_credential(mut self, credential: impl Into<String>) -> Self {
        self.workload_credential = Some(credential.into());
        self
    }

    #[must_use]
    pub fn with_authenticated_transport_binding(mut self, binding: impl Into<String>) -> Self {
        self.authenticated_transport_binding = Some(binding.into());
        self
    }

    #[must_use]
    pub fn with_service_context(
        mut self,
        actor: DelegatedActorContext,
        tenant: Option<TenantContext>,
    ) -> Self {
        self.delegated_actor_context = Some(actor);
        self.tenant_context = tenant;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectHttpEvidence {
    pub operation_id: Option<String>,
    pub decision: String,
    pub call_policy: CallPolicyEvidence,
    pub native_status: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct DirectHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub standard_error: Option<Value>,
    pub evidence: Option<DirectHttpEvidence>,
}

impl DirectHttpResponse {
    #[must_use]
    pub fn json(status: StatusCode, body: Value) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let standard_error =
            (status.is_client_error() || status.is_server_error()).then(|| body.clone());
        Self {
            status,
            headers,
            body: Bytes::from(serde_json::to_vec(&body).expect("JSON value must serialize")),
            standard_error,
            evidence: None,
        }
    }

    fn problem(status: StatusCode, code: &str, detail: &str, operation_id: Option<String>) -> Self {
        let mut response = Self::json(
            status,
            json!({"type":"about:blank","title":detail,"status":status.as_u16(),"detail":detail,"code":code,"request_id":null,"correlation_id":null,"errors":[]}),
        );
        response.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response.evidence = Some(DirectHttpEvidence {
            operation_id,
            decision: code.to_owned(),
            call_policy: CallPolicyEvidence {
                events: vec![if code == "overload_rejected" {
                    CallPolicyEvent::OverloadRejected
                } else {
                    CallPolicyEvent::CallFailed
                }],
                attempts: 0,
                terminal_outcome: CallPolicyTerminalOutcome::Rejected,
                fallback_handler: None,
            },
            native_status: Some(status.as_u16()),
        });
        response
    }
}

type HandlerFuture = Pin<Box<dyn Future<Output = DirectHttpResponse> + Send>>;
type Handler = dyn Fn(DirectHttpRequest) -> HandlerFuture + Send + Sync;

#[derive(Clone)]
pub struct DirectHttpServerBinding {
    inner: Arc<ServerInner>,
}
struct ServerInner {
    bindings: DirectHttpBindings,
    handler: Arc<Handler>,
    policy_runtime: CallPolicyRuntime,
    workload_identity: Option<DirectHttpWorkloadIdentity>,
    service_context: Option<DirectHttpServiceContext>,
}

#[derive(Debug, Clone)]
struct DirectHttpWorkloadIdentity {
    provider: Arc<dyn WorkloadIdentityProvider>,
    audience: String,
}

#[derive(Debug, Clone)]
struct DirectHttpServiceContext {
    admission: ServiceContextAdmission,
    recorder: Arc<dyn IdentityDecisionRecorder>,
}

impl std::fmt::Debug for DirectHttpServerBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectHttpServerBinding")
            .field("bindings", &self.inner.bindings)
            .finish_non_exhaustive()
    }
}

impl DirectHttpServerBinding {
    pub fn new<F, Fut>(
        bindings: DirectHttpBindings,
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
        context_provider: Arc<dyn DelegatedContextProvider>,
        context_policies: impl IntoIterator<Item = (String, ServiceContextPolicy)>,
        evidence_recorder: Arc<dyn IdentityDecisionRecorder>,
        handler: F,
    ) -> Self
    where
        F: Fn(DirectHttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = DirectHttpResponse> + Send + 'static,
    {
        Self::new_with_policy_runtime_unchecked(bindings, CallPolicyRuntime::default(), handler)
            .with_workload_identity(provider, audience)
            .with_service_context(context_provider, context_policies, evidence_recorder)
    }

    #[cfg(debug_assertions)]
    pub fn new_without_workload_identity<F, Fut>(bindings: DirectHttpBindings, handler: F) -> Self
    where
        F: Fn(DirectHttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = DirectHttpResponse> + Send + 'static,
    {
        Self::new_with_policy_runtime_unchecked(bindings, CallPolicyRuntime::default(), handler)
    }

    pub fn new_with_policy_runtime<F, Fut>(
        bindings: DirectHttpBindings,
        policy_runtime: CallPolicyRuntime,
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
        context_provider: Arc<dyn DelegatedContextProvider>,
        context_policies: impl IntoIterator<Item = (String, ServiceContextPolicy)>,
        evidence_recorder: Arc<dyn IdentityDecisionRecorder>,
        handler: F,
    ) -> Self
    where
        F: Fn(DirectHttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = DirectHttpResponse> + Send + 'static,
    {
        Self::new_with_policy_runtime_unchecked(bindings, policy_runtime, handler)
            .with_workload_identity(provider, audience)
            .with_service_context(context_provider, context_policies, evidence_recorder)
    }

    #[cfg(debug_assertions)]
    pub fn new_with_policy_runtime_without_workload_identity<F, Fut>(
        bindings: DirectHttpBindings,
        policy_runtime: CallPolicyRuntime,
        handler: F,
    ) -> Self
    where
        F: Fn(DirectHttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = DirectHttpResponse> + Send + 'static,
    {
        Self::new_with_policy_runtime_unchecked(bindings, policy_runtime, handler)
    }

    fn new_with_policy_runtime_unchecked<F, Fut>(
        bindings: DirectHttpBindings,
        policy_runtime: CallPolicyRuntime,
        handler: F,
    ) -> Self
    where
        F: Fn(DirectHttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = DirectHttpResponse> + Send + 'static,
    {
        Self {
            inner: Arc::new(ServerInner {
                bindings,
                handler: Arc::new(move |request| Box::pin(handler(request))),
                policy_runtime,
                workload_identity: None,
                service_context: None,
            }),
        }
    }

    #[must_use]
    pub fn with_workload_identity(
        self,
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
    ) -> Self {
        Self {
            inner: Arc::new(ServerInner {
                bindings: self.inner.bindings.clone(),
                handler: Arc::clone(&self.inner.handler),
                policy_runtime: self.inner.policy_runtime.clone(),
                workload_identity: Some(DirectHttpWorkloadIdentity {
                    provider,
                    audience: audience.into(),
                }),
                service_context: self.inner.service_context.clone(),
            }),
        }
    }

    #[must_use]
    pub fn with_service_context<I, S>(
        self,
        provider: Arc<dyn DelegatedContextProvider>,
        policies: I,
        recorder: Arc<dyn IdentityDecisionRecorder>,
    ) -> Self
    where
        I: IntoIterator<Item = (S, ServiceContextPolicy)>,
        S: Into<String>,
    {
        Self {
            inner: Arc::new(ServerInner {
                bindings: self.inner.bindings.clone(),
                handler: Arc::clone(&self.inner.handler),
                policy_runtime: self.inner.policy_runtime.clone(),
                workload_identity: self.inner.workload_identity.clone(),
                service_context: Some(DirectHttpServiceContext {
                    admission: ServiceContextAdmission::new(provider, policies),
                    recorder,
                }),
            }),
        }
    }

    pub async fn handle(&self, request: DirectHttpRequest) -> DirectHttpResponse {
        self.inner.handle(request).await
    }

    #[must_use]
    pub fn router(self) -> Router {
        Router::new()
            .fallback(any(handle_axum))
            .with_state(self.inner)
    }
}

impl ServerInner {
    async fn handle(&self, mut request: DirectHttpRequest) -> DirectHttpResponse {
        let Some(operation) = self.bindings.match_request(&request.method, &request.path) else {
            return DirectHttpResponse::problem(
                StatusCode::NOT_FOUND,
                "operation_not_found",
                "Operation not found",
                None,
            );
        };
        if let Some(identity) = &self.workload_identity {
            let Some(credential) = request.workload_credential.as_deref() else {
                return DirectHttpResponse::problem(
                    StatusCode::UNAUTHORIZED,
                    "workload_identity_required",
                    "Workload Identity credential is required",
                    Some(operation.operation_id.clone()),
                );
            };
            let Some(binding) = request.authenticated_transport_binding.as_deref() else {
                return DirectHttpResponse::problem(
                    StatusCode::UNAUTHORIZED,
                    "authenticated_transport_binding_required",
                    "Authenticated transport binding is required",
                    Some(operation.operation_id.clone()),
                );
            };
            match identity.provider.verify(
                credential,
                &WorkloadIdentityVerification::new(&identity.audience, binding, now_ms()),
            ) {
                Ok(principal) => request.authenticated_service_principal = Some(principal),
                Err(error) => {
                    return DirectHttpResponse::problem(
                        StatusCode::UNAUTHORIZED,
                        &error.evidence.outcome,
                        &error.message,
                        Some(operation.operation_id.clone()),
                    );
                }
            }
        }
        if let Some(context) = &self.service_context {
            let decision = if request.service_context_decode_failed {
                Err(context.admission.invalid_proof(&operation.operation_id))
            } else {
                let service_context = request
                    .delegated_actor_context
                    .clone()
                    .map(|actor| ServiceContext::new(actor, request.tenant_context.clone()));
                context
                    .admission
                    .admit(&operation.operation_id, service_context.as_ref(), now_ms())
            };
            match decision {
                Ok(authenticated) => {
                    if context.recorder.record(&authenticated.evidence).is_err() {
                        return DirectHttpResponse::problem(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "identity_evidence_persistence_failed",
                            "Identity decision evidence could not be persisted",
                            Some(operation.operation_id.clone()),
                        );
                    }
                    request.authenticated_service_context = Some(authenticated);
                }
                Err(error) => {
                    if context.recorder.record(&error.evidence).is_err() {
                        return DirectHttpResponse::problem(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "identity_evidence_persistence_failed",
                            "Identity decision evidence could not be persisted",
                            Some(operation.operation_id.clone()),
                        );
                    }
                    return DirectHttpResponse::problem(
                        StatusCode::FORBIDDEN,
                        &error.evidence.outcome,
                        &error.message,
                        Some(operation.operation_id.clone()),
                    );
                }
            }
        }
        if request
            .deadline_unix_ms
            .is_none_or(|deadline| deadline <= now_ms())
        {
            return DirectHttpResponse::problem(
                StatusCode::GATEWAY_TIMEOUT,
                "deadline_expired",
                "Deadline is missing or expired",
                Some(operation.operation_id.clone()),
            );
        }
        if operation.idempotency == HttpIdempotency::RequiresKey
            && request.idempotency_key.as_deref().is_none_or(str::is_empty)
        {
            return DirectHttpResponse::problem(
                StatusCode::BAD_REQUEST,
                "idempotency_key_required",
                "Idempotency Key is required",
                Some(operation.operation_id.clone()),
            );
        }
        let operation_key = format!("{}:{}", self.bindings.contract_id, operation.operation_id);
        let Ok(_admission) = self
            .policy_runtime
            .admit(operation_key, &operation.call_policy)
        else {
            return DirectHttpResponse::problem(
                StatusCode::TOO_MANY_REQUESTS,
                "overload_rejected",
                "Service operation is overloaded",
                Some(operation.operation_id.clone()),
            );
        };
        (self.handler)(request).await
    }
}

async fn handle_axum(State(inner): State<Arc<ServerInner>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let deadline_unix_ms = parts
        .headers
        .get(DEADLINE_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let idempotency_key = parts
        .headers
        .get(IDEMPOTENCY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let workload_credential = parts
        .headers
        .get(AUTHORIZATION_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_owned);
    let authenticated_transport_binding = parts
        .extensions
        .get::<AuthenticatedTransportBinding>()
        .map(|binding| binding.0.clone());
    let actor =
        decode_context_header::<DelegatedActorContext>(&parts.headers, DELEGATED_ACTOR_HEADER);
    let tenant = decode_context_header::<TenantContext>(&parts.headers, TENANT_CONTEXT_HEADER);
    let service_context_decode_failed = actor.is_err() || tenant.is_err();
    let delegated_actor_context = actor.ok().flatten();
    let tenant_context = tenant.ok().flatten();
    let body = to_bytes(body, 16 * 1024 * 1024).await.unwrap_or_default();
    let response = inner
        .handle(DirectHttpRequest {
            method: parts.method,
            path: parts.uri.path().to_owned(),
            headers: parts.headers,
            body,
            deadline_unix_ms,
            idempotency_key,
            workload_credential,
            authenticated_transport_binding,
            authenticated_service_principal: None,
            delegated_actor_context,
            tenant_context,
            service_context_decode_failed,
            authenticated_service_context: None,
        })
        .await;
    response.into_response()
}

impl IntoResponse for DirectHttpResponse {
    fn into_response(self) -> Response {
        let mut response = Response::new(Body::from(self.body));
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers;
        response
    }
}

#[derive(Debug, Clone)]
pub struct DirectHttpCall {
    operation_id: String,
    path_parameters: BTreeMap<String, String>,
    body: Option<Value>,
    deadline_unix_ms: Option<u64>,
    idempotency_key: Option<String>,
    workload_credential: Option<String>,
    delegated_actor_context: Option<DelegatedActorContext>,
    tenant_context: Option<TenantContext>,
}
impl DirectHttpCall {
    #[must_use]
    pub fn new(operation_id: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            path_parameters: BTreeMap::new(),
            body: None,
            deadline_unix_ms: None,
            idempotency_key: None,
            workload_credential: None,
            delegated_actor_context: None,
            tenant_context: None,
        }
    }
    #[must_use]
    pub fn with_path_parameter(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.path_parameters.insert(name.into(), value.into());
        self
    }
    #[must_use]
    pub fn with_json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }
    #[must_use]
    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline_unix_ms = Some(deadline);
        self
    }
    #[must_use]
    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn with_workload_credential(mut self, credential: impl Into<String>) -> Self {
        self.workload_credential = Some(credential.into());
        self
    }

    #[must_use]
    pub fn with_service_context(
        mut self,
        actor: DelegatedActorContext,
        tenant: Option<TenantContext>,
    ) -> Self {
        self.delegated_actor_context = Some(actor);
        self.tenant_context = tenant;
        self
    }
}

pub struct DirectHttpClient<R> {
    resolver: R,
    bindings: DirectHttpBindings,
    http: reqwest::Client,
    policy_runtime: CallPolicyRuntime,
    fallbacks: BTreeMap<String, Arc<HttpFallback>>,
}
type HttpFallback = dyn Fn(CallPolicyFailure) -> DirectHttpResponse + Send + Sync;
impl<R> std::fmt::Debug for DirectHttpClient<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectHttpClient")
            .field("bindings", &self.bindings)
            .field("policy_runtime", &self.policy_runtime)
            .field("fallbacks", &self.fallbacks.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}
impl<R: EndpointResolver> DirectHttpClient<R> {
    #[must_use]
    pub fn new(resolver: R, bindings: DirectHttpBindings) -> Self {
        Self {
            resolver,
            bindings,
            http: reqwest::Client::new(),
            policy_runtime: CallPolicyRuntime::default(),
            fallbacks: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_policy_runtime(mut self, policy_runtime: CallPolicyRuntime) -> Self {
        self.policy_runtime = policy_runtime;
        self
    }

    #[must_use]
    pub fn with_fallback<F>(mut self, handler: impl Into<String>, fallback: F) -> Self
    where
        F: Fn(CallPolicyFailure) -> DirectHttpResponse + Send + Sync + 'static,
    {
        self.fallbacks.insert(handler.into(), Arc::new(fallback));
        self
    }
    pub async fn call(
        &self,
        service: &ServiceReference,
        call: DirectHttpCall,
    ) -> Result<DirectHttpResponse, DirectHttpCallError> {
        let operation = self.bindings.operation(&call.operation_id).ok_or_else(|| {
            DirectHttpCallError::Contract(
                "operation is not declared by the generated binding".to_owned(),
            )
        })?;
        let deadline = call
            .deadline_unix_ms
            .ok_or_else(|| DirectHttpCallError::Contract("deadline_required".to_owned()))?;
        if deadline <= now_ms() {
            return self.deadline_failure(operation, 0, Vec::new());
        }
        if operation.idempotency == HttpIdempotency::RequiresKey
            && call.idempotency_key.as_deref().is_none_or(str::is_empty)
        {
            return Err(DirectHttpCallError::Contract(
                "idempotency_key_required".to_owned(),
            ));
        }
        let method = Method::from_bytes(operation.method.as_bytes())
            .map_err(|error| DirectHttpCallError::Contract(error.to_string()))?;
        let path = expand_path(&operation.path, &call.path_parameters)?;
        let operation_key = format!(
            "{}:{}:{}",
            service.as_str(),
            self.bindings.contract_id,
            operation.operation_id
        );
        let permit = match self
            .policy_runtime
            .begin_call(operation_key, &operation.call_policy)
        {
            Ok(permit) => permit,
            Err(event) => {
                let failure = match event {
                    CallPolicyEvent::CircuitOpen => CallPolicyFailure::CircuitOpen,
                    CallPolicyEvent::BulkheadSaturated => CallPolicyFailure::BulkheadSaturated,
                    _ => CallPolicyFailure::NonRetryableFailure,
                };
                if let Some(response) =
                    self.fallback_response(operation, failure, 0, vec![event], None)
                {
                    return Ok(response);
                }
                return Err(DirectHttpCallError::Policy {
                    failure,
                    evidence: CallPolicyEvidence {
                        events: vec![event],
                        attempts: 0,
                        terminal_outcome: CallPolicyTerminalOutcome::Rejected,
                        fallback_handler: None,
                    },
                });
            }
        };
        let state = self
            .resolver
            .resolve(service)
            .map_err(|error| DirectHttpCallError::Resolution(error.to_string()))?;
        let endpoint = state
            .endpoints
            .first()
            .ok_or_else(|| DirectHttpCallError::Resolution("no usable endpoint".to_owned()))?;
        let url = format!("{}{}", endpoint.address.trim_end_matches('/'), path);
        let mut retry_events = Vec::new();
        for attempt in 1..=operation.call_policy.max_attempts {
            let remaining_ms = deadline.saturating_sub(now_ms());
            if remaining_ms == 0 {
                retry_events.push(CallPolicyEvent::DeadlineExpired);
                let events = permit.failure_after(retry_events);
                return self.deadline_failure(operation, attempt - 1, events);
            }
            let mut request = self
                .http
                .request(method.clone(), &url)
                .timeout(Duration::from_millis(remaining_ms))
                .header(DEADLINE_HEADER, deadline);
            if let Some(key) = call.idempotency_key.as_deref() {
                request = request.header(IDEMPOTENCY_HEADER, key);
            }
            if let Some(credential) = call.workload_credential.as_deref() {
                request = request.bearer_auth(credential);
            }
            if let Some(actor) = call.delegated_actor_context.as_ref() {
                request = request.header(DELEGATED_ACTOR_HEADER, encode_context_header(actor)?);
            }
            if let Some(tenant) = call.tenant_context.as_ref() {
                request = request.header(TENANT_CONTEXT_HEADER, encode_context_header(tenant)?);
            }
            if let Some(body) = call.body.as_ref() {
                request = request.json(body);
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    let failure = if error.is_timeout() {
                        retry_events.push(CallPolicyEvent::DeadlineExpired);
                        CallPolicyFailure::DeadlineExpired
                    } else {
                        retry_events.push(CallPolicyEvent::CallFailed);
                        CallPolicyFailure::TransportFailure
                    };
                    let events = permit.failure_after(retry_events);
                    if let Some(response) =
                        self.fallback_response(operation, failure, attempt, events.clone(), None)
                    {
                        return Ok(response);
                    }
                    return Err(DirectHttpCallError::Transport {
                        message: format!("transport_failure_no_retry: {error}"),
                        evidence: CallPolicyEvidence {
                            events,
                            attempts: attempt,
                            terminal_outcome: CallPolicyTerminalOutcome::Failed,
                            fallback_handler: None,
                        },
                    });
                }
            };
            let status = response.status();
            let decision =
                operation.retry_decision_for(status, attempt, call.idempotency_key.as_deref());
            if decision.should_retry {
                retry_events.push(CallPolicyEvent::RetryScheduled);
                continue;
            }
            let headers = response.headers().clone();
            let body = match response.bytes().await {
                Ok(body) => body,
                Err(error) => {
                    retry_events.push(CallPolicyEvent::CallFailed);
                    let events = permit.failure_after(retry_events);
                    return Err(DirectHttpCallError::Transport {
                        message: error.to_string(),
                        evidence: CallPolicyEvidence {
                            events,
                            attempts: attempt,
                            terminal_outcome: CallPolicyTerminalOutcome::Failed,
                            fallback_handler: None,
                        },
                    });
                }
            };
            let standard_error = serde_json::from_slice(&body).ok().filter(|value| {
                is_standard_problem(value, status, operation.standard_error_schema.as_ref())
            });
            let retryable_failure = matches!(status.as_u16(), 429 | 502 | 503 | 504);
            if status == StatusCode::TOO_MANY_REQUESTS {
                retry_events.push(CallPolicyEvent::OverloadRejected);
            }
            retry_events.push(if status.is_success() {
                CallPolicyEvent::CallCompleted
            } else {
                CallPolicyEvent::CallFailed
            });
            let events = if retryable_failure {
                permit.failure_after(retry_events)
            } else {
                permit.success_after(retry_events)
            };
            let failure = if status == StatusCode::TOO_MANY_REQUESTS {
                CallPolicyFailure::OverloadRejected
            } else if retryable_failure {
                CallPolicyFailure::RetryableFailure
            } else {
                CallPolicyFailure::NonRetryableFailure
            };
            if !status.is_success() {
                if let Some(response) = self.fallback_response(
                    operation,
                    failure,
                    attempt,
                    events.clone(),
                    Some(status),
                ) {
                    return Ok(response);
                }
            }
            let evidence = Some(DirectHttpEvidence {
                operation_id: Some(operation.operation_id.clone()),
                decision: if status.is_success() {
                    "call_completed".to_owned()
                } else {
                    decision.reason.to_owned()
                },
                call_policy: CallPolicyEvidence {
                    events,
                    attempts: attempt,
                    terminal_outcome: if status.is_success() {
                        CallPolicyTerminalOutcome::Completed
                    } else {
                        CallPolicyTerminalOutcome::Failed
                    },
                    fallback_handler: None,
                },
                native_status: Some(status.as_u16()),
            });
            return Ok(DirectHttpResponse {
                status,
                headers,
                body,
                standard_error,
                evidence,
            });
        }
        unreachable!("the direct HTTP call loop always returns by its final attempt")
    }

    fn fallback_response(
        &self,
        operation: &DirectHttpOperation,
        failure: CallPolicyFailure,
        attempts: u32,
        mut events: Vec<CallPolicyEvent>,
        native_status: Option<StatusCode>,
    ) -> Option<DirectHttpResponse> {
        let declaration = operation.call_policy.fallback_for(failure)?;
        let fallback = self.fallbacks.get(&declaration.handler)?;
        let mut response = fallback(failure);
        events.push(CallPolicyEvent::FallbackApplied);
        response.evidence = Some(DirectHttpEvidence {
            operation_id: Some(operation.operation_id.clone()),
            decision: "fallback_applied".to_owned(),
            call_policy: CallPolicyEvidence {
                events,
                attempts,
                terminal_outcome: CallPolicyTerminalOutcome::Fallback,
                fallback_handler: Some(declaration.handler.clone()),
            },
            native_status: native_status.map(|status| status.as_u16()),
        });
        Some(response)
    }

    fn deadline_failure(
        &self,
        operation: &DirectHttpOperation,
        attempts: u32,
        mut events: Vec<CallPolicyEvent>,
    ) -> Result<DirectHttpResponse, DirectHttpCallError> {
        if !events.contains(&CallPolicyEvent::DeadlineExpired) {
            events.push(CallPolicyEvent::DeadlineExpired);
        }
        if let Some(response) = self.fallback_response(
            operation,
            CallPolicyFailure::DeadlineExpired,
            attempts,
            events.clone(),
            None,
        ) {
            return Ok(response);
        }
        Err(DirectHttpCallError::Policy {
            failure: CallPolicyFailure::DeadlineExpired,
            evidence: CallPolicyEvidence {
                events,
                attempts,
                terminal_outcome: CallPolicyTerminalOutcome::Failed,
                fallback_handler: None,
            },
        })
    }
}

fn encode_context_header<T: Serialize>(value: &T) -> Result<String, DirectHttpCallError> {
    serde_json::to_vec(value)
        .map(|json| URL_SAFE_NO_PAD.encode(json))
        .map_err(|error| DirectHttpCallError::Contract(error.to_string()))
}

fn decode_context_header<T: for<'de> Deserialize<'de>>(
    headers: &HeaderMap,
    name: &str,
) -> Result<Option<T>, ()> {
    let Some(value) = headers.get(name) else {
        return Ok(None);
    };
    value
        .to_str()
        .map_err(|_| ())
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).map_err(|_| ()))
        .and_then(|value| serde_json::from_slice(&value).map_err(|_| ()))
        .map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectHttpCallError {
    Contract(String),
    Resolution(String),
    Transport {
        message: String,
        evidence: CallPolicyEvidence,
    },
    Policy {
        failure: CallPolicyFailure,
        evidence: CallPolicyEvidence,
    },
}
impl std::fmt::Display for DirectHttpCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(value) | Self::Resolution(value) => f.write_str(value),
            Self::Transport { message, .. } => f.write_str(message),
            Self::Policy { failure, .. } => write!(f, "call policy rejected: {failure:?}"),
        }
    }
}
impl std::error::Error for DirectHttpCallError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryDecision {
    pub should_retry: bool,
    pub reason: &'static str,
}
impl RetryDecision {
    pub(crate) fn yes() -> Self {
        Self {
            should_retry: true,
            reason: "declared_safe_retry",
        }
    }
    pub(crate) fn no(reason: &'static str) -> Self {
        Self {
            should_retry: false,
            reason,
        }
    }
}

fn path_matches(template: &str, actual: &str) -> bool {
    let template = template.trim_matches('/').split('/');
    let actual = actual.trim_matches('/').split('/');
    let template: Vec<_> = template.collect();
    let actual: Vec<_> = actual.collect();
    template.len() == actual.len()
        && template.iter().zip(actual).all(|(expected, value)| {
            (expected.starts_with('{') && expected.ends_with('}')) || *expected == value
        })
}

fn expand_path(
    template: &str,
    parameters: &BTreeMap<String, String>,
) -> Result<String, DirectHttpCallError> {
    let mut path = String::new();
    for segment in template.trim_start_matches('/').split('/') {
        path.push('/');
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        {
            let value = parameters
                .get(name)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DirectHttpCallError::Contract(format!("missing path parameter `{name}`"))
                })?;
            if value.contains('/') {
                return Err(DirectHttpCallError::Contract(format!(
                    "path parameter `{name}` must be one segment"
                )));
            }
            path.push_str(&encode_path_segment(value));
        } else if !segment.is_empty() {
            path.push_str(segment);
        }
    }
    Ok(path)
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn is_standard_problem(value: &Value, status: StatusCode, schema: Option<&Value>) -> bool {
    let Some(required) = schema
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    required
        .iter()
        .filter_map(Value::as_str)
        .all(|field| value.get(field).is_some())
        && value.get("status").and_then(Value::as_u64) == Some(u64::from(status.as_u16()))
}

fn resolve_local_schema(openapi: &Value, schema: &Value) -> Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix('#'))
        .and_then(|pointer| openapi.pointer(pointer))
        .cloned()
        .unwrap_or_else(|| schema.clone())
}

fn response_schemas(
    openapi: &Value,
    operation: &serde_json::Map<String, Value>,
) -> BTreeMap<String, Value> {
    operation
        .get("responses")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(status, response)| {
            response
                .get("content")
                .and_then(Value::as_object)
                .and_then(|content| content.values().find_map(|media| media.get("schema")))
                .map(|schema| (status.clone(), resolve_local_schema(openapi, schema)))
        })
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
