use crate::{EndpointResolver, ServiceReference};
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
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
        if attempt > 1 {
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
            HttpIdempotency::Unknown => RetryDecision::no("operation_retry_safety_unknown"),
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
            operations.push(DirectHttpOperation {
                operation_id: operation_id.to_owned(),
                method: method.to_uppercase(),
                path: path.clone(),
                idempotency,
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
        }
    }

    #[must_use]
    pub fn with_deadline(mut self, deadline_unix_ms: u64) -> Self {
        self.deadline_unix_ms = Some(deadline_unix_ms);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectHttpEvidence {
    pub operation_id: Option<String>,
    pub decision: String,
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
    pub fn new<F, Fut>(bindings: DirectHttpBindings, handler: F) -> Self
    where
        F: Fn(DirectHttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = DirectHttpResponse> + Send + 'static,
    {
        Self {
            inner: Arc::new(ServerInner {
                bindings,
                handler: Arc::new(move |request| Box::pin(handler(request))),
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
    async fn handle(&self, request: DirectHttpRequest) -> DirectHttpResponse {
        let Some(operation) = self.bindings.match_request(&request.method, &request.path) else {
            return DirectHttpResponse::problem(
                StatusCode::NOT_FOUND,
                "operation_not_found",
                "Operation not found",
                None,
            );
        };
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
    let body = to_bytes(body, 16 * 1024 * 1024).await.unwrap_or_default();
    let response = inner
        .handle(DirectHttpRequest {
            method: parts.method,
            path: parts.uri.path().to_owned(),
            headers: parts.headers,
            body,
            deadline_unix_ms,
            idempotency_key,
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
}

#[derive(Debug)]
pub struct DirectHttpClient<R> {
    resolver: R,
    bindings: DirectHttpBindings,
    http: reqwest::Client,
}
impl<R: EndpointResolver> DirectHttpClient<R> {
    #[must_use]
    pub fn new(resolver: R, bindings: DirectHttpBindings) -> Self {
        Self {
            resolver,
            bindings,
            http: reqwest::Client::new(),
        }
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
            return Err(DirectHttpCallError::Contract("deadline_expired".to_owned()));
        }
        if operation.idempotency == HttpIdempotency::RequiresKey
            && call.idempotency_key.as_deref().is_none_or(str::is_empty)
        {
            return Err(DirectHttpCallError::Contract(
                "idempotency_key_required".to_owned(),
            ));
        }
        let state = self
            .resolver
            .resolve(service)
            .map_err(|error| DirectHttpCallError::Resolution(error.to_string()))?;
        let endpoint = state
            .endpoints
            .first()
            .ok_or_else(|| DirectHttpCallError::Resolution("no usable endpoint".to_owned()))?;
        let method = Method::from_bytes(operation.method.as_bytes())
            .map_err(|error| DirectHttpCallError::Contract(error.to_string()))?;
        let path = expand_path(&operation.path, &call.path_parameters)?;
        let url = format!("{}{}", endpoint.address.trim_end_matches('/'), path);
        for attempt in 1..=2 {
            let remaining_ms = deadline.saturating_sub(now_ms());
            if remaining_ms == 0 {
                return Err(DirectHttpCallError::Contract("deadline_expired".to_owned()));
            }
            let mut request = self
                .http
                .request(method.clone(), &url)
                .timeout(Duration::from_millis(remaining_ms))
                .header(DEADLINE_HEADER, deadline);
            if let Some(key) = call.idempotency_key.as_deref() {
                request = request.header(IDEMPOTENCY_HEADER, key);
            }
            if let Some(body) = call.body.as_ref() {
                request = request.json(body);
            }
            let response = request.send().await.map_err(|error| {
                DirectHttpCallError::Transport(format!("transport_failure_no_retry: {error}"))
            })?;
            let status = response.status();
            let decision =
                operation.retry_decision_for(status, attempt, call.idempotency_key.as_deref());
            if decision.should_retry {
                continue;
            }
            let headers = response.headers().clone();
            let body = response
                .bytes()
                .await
                .map_err(|error| DirectHttpCallError::Transport(error.to_string()))?;
            let standard_error = serde_json::from_slice(&body).ok().filter(|value| {
                is_standard_problem(value, status, operation.standard_error_schema.as_ref())
            });
            let evidence = Some(DirectHttpEvidence {
                operation_id: Some(operation.operation_id.clone()),
                decision: if status.is_success() {
                    "call_completed".to_owned()
                } else {
                    decision.reason.to_owned()
                },
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectHttpCallError {
    Contract(String),
    Resolution(String),
    Transport(String),
}
impl std::fmt::Display for DirectHttpCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(value) | Self::Resolution(value) | Self::Transport(value) => {
                f.write_str(value)
            }
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
    fn yes() -> Self {
        Self {
            should_retry: true,
            reason: "declared_safe_retry",
        }
    }
    fn no(reason: &'static str) -> Self {
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
