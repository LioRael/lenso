use axum::body::Body;
use axum::extract::{FromRequestParts, Request};
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use platform_core::{
    ActorContext, CorrelationId, IdGenerator, RequestContext, RequestId, TraceContext,
    UuidGenerator,
};
use tracing::Instrument;

const REQUEST_ID_HEADER: &str = "x-request-id";
const CORRELATION_ID_HEADER: &str = "x-correlation-id";

#[derive(Debug, Clone)]
pub struct HttpRequestContext(pub RequestContext);

impl std::ops::Deref for HttpRequestContext {
    type Target = RequestContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub async fn request_context_middleware(mut request: Request<Body>, next: Next) -> Response {
    let request_id = header_value(request.headers(), REQUEST_ID_HEADER)
        .unwrap_or_else(|| UuidGenerator.new_id("req"));
    let correlation_id = header_value(request.headers(), CORRELATION_ID_HEADER)
        .unwrap_or_else(|| UuidGenerator.new_id("corr"));
    let method = request.method().clone();
    let path = request.uri().path().to_owned();

    let context = RequestContext {
        request_id: RequestId::new(request_id),
        correlation_id: CorrelationId::new(correlation_id),
        trace: TraceContext::default(),
        actor: ActorContext::Anonymous,
        tenant_id: None,
        causation_id: None,
    };

    request
        .extensions_mut()
        .insert(HttpRequestContext(context.clone()));

    let span = tracing::info_span!(
        "http_request",
        request_id = %context.request_id.0,
        correlation_id = %context.correlation_id.0,
        http_method = %method,
        http_path = %path,
    );

    let mut response = next.run(request).instrument(span).await;
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

#[async_trait::async_trait]
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
