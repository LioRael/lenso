use axum::body::Body;
use axum::extract::State;
use axum::extract::{FromRequestParts, Request};
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use chrono::{DateTime, Utc};
use platform_core::{
    ActorContext, AppContext, CorrelationId, IdGenerator, RequestContext, RequestId, UuidGenerator,
    generate_trace_context, is_local_development_environment,
    story_events::{
        HttpRequestStoryEventRecord, http_request_story_creation, http_request_story_event_id,
        insert_http_request_story_projection,
    },
    trace_context_from_traceparent,
};
use std::time::Instant;
use tracing::Instrument;

const REQUEST_ID_HEADER: &str = "x-request-id";
const CORRELATION_ID_HEADER: &str = "x-correlation-id";
const AUTHORIZATION_HEADER: &str = "authorization";
const TRACEPARENT_HEADER: &str = "traceparent";

#[derive(Debug, Clone)]
pub struct HttpRequestContext(pub RequestContext);

impl std::ops::Deref for HttpRequestContext {
    type Target = RequestContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub async fn request_context_middleware(
    State(ctx): State<AppContext>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let started_at = Instant::now();
    let started_at_utc = Utc::now();
    let request_id = header_value(request.headers(), REQUEST_ID_HEADER)
        .unwrap_or_else(|| UuidGenerator.new_id("req"));
    let correlation_id = header_value(request.headers(), CORRELATION_ID_HEADER)
        .unwrap_or_else(|| UuidGenerator.new_id("corr"));
    let actor = authorization_header(request.headers())
        .and_then(|value| parse_dev_bearer_actor(value, &ctx.config.service.environment))
        .unwrap_or_default();
    let trace = traceparent_header(request.headers())
        .and_then(|value| trace_context_from_traceparent(&value))
        .unwrap_or_else(generate_trace_context);
    let method = request.method().clone();
    let path = request.uri().path().to_owned();

    let mut context = RequestContext {
        request_id: RequestId::new(request_id),
        correlation_id: CorrelationId::new(correlation_id),
        trace,
        actor,
        tenant_id: None,
        causation_id: None,
    };
    context.causation_id = Some(http_request_story_event_id(&context));

    request
        .extensions_mut()
        .insert(HttpRequestContext(context.clone()));

    let span = tracing::info_span!(
        "http_request",
        request_id = %context.request_id.0,
        correlation_id = %context.correlation_id.0,
        lenso.correlation_id = %context.correlation_id.0,
        lenso.story_id = %context.correlation_id.0,
        lenso.execution.kind = "http_request",
        lenso.execution.name = %format!("{} {}", method.as_str(), path.as_str()),
        otel.trace_id = context.trace.trace_id.as_deref().unwrap_or(""),
        otel.parent_span_id = context.trace.span_id.as_deref().unwrap_or(""),
        http_method = %method,
        http_path = %path,
    );

    let mut response = next.run(request).instrument(span).await;
    record_http_request_story(
        ctx.db.clone(),
        context.clone(),
        method.as_str(),
        path.as_str(),
        response.status().as_u16(),
        response.headers().get("x-lenso-error-code"),
        started_at,
        started_at_utc,
    );
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        context.request_id.0.parse().expect("valid request id"),
    );
    response.headers_mut().insert(
        CORRELATION_ID_HEADER,
        context
            .correlation_id
            .0
            .parse()
            .expect("valid correlation id"),
    );
    response
}

fn record_http_request_story(
    pool: platform_core::DbPool,
    request_ctx: RequestContext,
    method: &str,
    path: &str,
    status_code: u16,
    error_code_header: Option<&axum::http::HeaderValue>,
    started_at: Instant,
    started_at_utc: DateTime<Utc>,
) {
    let error_code = error_code_header
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let duration_ms = started_at.elapsed().as_millis().min(i64::MAX as u128) as i64;
    let record = HttpRequestStoryEventRecord {
        method: method.to_owned(),
        path: path.to_owned(),
        status_code,
        error_code,
        creation: http_request_story_creation(path, status_code),
        started_at: started_at_utc,
        completed_at: Utc::now(),
        duration_ms,
    };

    tokio::spawn(async move {
        if let Err(error) = insert_http_request_story_projection(&pool, &request_ctx, record).await
        {
            tracing::warn!(
                error = ?error,
                request_id = %request_ctx.request_id.0,
                correlation_id = %request_ctx.correlation_id.0,
                "failed to write HTTP request story projection"
            );
        }
    });
}

impl<S> FromRequestParts<S> for HttpRequestContext
where
    S: Send + Sync,
{
    type Rejection = crate::ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<HttpRequestContext>()
            .cloned()
            .ok_or_else(|| {
                platform_core::AppError::new(
                    platform_core::ErrorCode::Internal,
                    "Request context is missing",
                )
                .into()
            })
    }
}

fn header_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let name = HeaderName::from_static(match name {
        REQUEST_ID_HEADER => REQUEST_ID_HEADER,
        CORRELATION_ID_HEADER => CORRELATION_ID_HEADER,
        _ => unreachable!("known request context header"),
    });

    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn authorization_header(headers: &axum::http::HeaderMap) -> Option<String> {
    let name = HeaderName::from_static(AUTHORIZATION_HEADER);
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn traceparent_header(headers: &axum::http::HeaderMap) -> Option<String> {
    let name = HeaderName::from_static(TRACEPARENT_HEADER);
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_dev_bearer_actor(value: String, environment: &str) -> Option<ActorContext> {
    if !is_local_development_environment(environment) {
        return None;
    }

    let token = value.strip_prefix("Bearer ")?;

    if let Some(user_id) = token.strip_prefix("dev-user:") {
        return Some(ActorContext::User {
            user_id: user_id.to_owned(),
            scopes: Vec::new(),
        });
    }

    if let Some(service_token) = token.strip_prefix("dev-service:") {
        let (service_id, scopes) = parse_dev_actor_scopes(service_token);
        return Some(ActorContext::Service { service_id, scopes });
    }

    None
}

fn parse_dev_actor_scopes(value: &str) -> (String, Vec<String>) {
    let Some((id, raw_scopes)) = value.split_once(':') else {
        return (value.to_owned(), Vec::new());
    };
    let scopes = raw_scopes
        .split(',')
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    (id.to_owned(), scopes)
}
