use axum::body::Bytes;
use axum::http::HeaderMap;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::ModuleHttpMethod;

const ACCEPT_HEADER: &str = "accept";
const CONTENT_TYPE_HEADER: &str = "content-type";
const TRACEPARENT_HEADER: &str = "traceparent";
const X_CORRELATION_ID_HEADER: &str = "x-correlation-id";
const X_REQUEST_ID_HEADER: &str = "x-request-id";
const MAX_PROXY_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) enum ProxyRequestBody {
    Empty,
    #[allow(dead_code)]
    Json(Bytes),
}

pub(crate) fn apply_proxy_request_policy(
    request: reqwest::RequestBuilder,
    method: ModuleHttpMethod,
    headers: &HeaderMap,
    request_ctx: &platform_core::RequestContext,
    auth_token: Option<&str>,
    body: ProxyRequestBody,
) -> AppResult<reqwest::RequestBuilder> {
    let mut request = request;
    request = forward_header(request, headers, ACCEPT_HEADER);
    if let Some(token) = auth_token {
        request = request.bearer_auth(token);
    }
    request = request
        .header(X_REQUEST_ID_HEADER, request_ctx.request_id.0.as_str())
        .header(
            X_CORRELATION_ID_HEADER,
            request_ctx.correlation_id.0.as_str(),
        );
    if let (Some(trace_id), Some(span_id)) = (
        request_ctx.trace.trace_id.as_deref(),
        request_ctx.trace.span_id.as_deref(),
    ) {
        request = request.header(TRACEPARENT_HEADER, format!("00-{trace_id}-{span_id}-01"));
    }

    match body {
        ProxyRequestBody::Empty => Ok(request),
        ProxyRequestBody::Json(body) => apply_json_body_policy(request, method, headers, body),
    }
}

fn apply_json_body_policy(
    request: reqwest::RequestBuilder,
    method: ModuleHttpMethod,
    headers: &HeaderMap,
    body: Bytes,
) -> AppResult<reqwest::RequestBuilder> {
    if !method_allows_request_body(method) {
        return Err(AppError::new(
            ErrorCode::Validation,
            format!(
                "remote HTTP proxy method {} does not accept a request body",
                module_http_method_label(method)
            ),
        ));
    }
    if body.len() > MAX_PROXY_REQUEST_BYTES {
        return Err(AppError::new(
            ErrorCode::Validation,
            format!(
                "remote HTTP proxy request body exceeded {MAX_PROXY_REQUEST_BYTES} bytes: {} bytes",
                body.len()
            ),
        ));
    }

    let content_type = validated_json_content_type(headers)?;
    Ok(request.header(CONTENT_TYPE_HEADER, content_type).body(body))
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

fn validated_json_content_type(headers: &HeaderMap) -> AppResult<&str> {
    let Some(content_type) = headers
        .get(CONTENT_TYPE_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(AppError::new(
            ErrorCode::Validation,
            "remote HTTP proxy request content-type was missing",
        ));
    };

    if is_json_content_type(content_type) {
        return Ok(content_type);
    }

    Err(AppError::new(
        ErrorCode::Validation,
        format!("remote HTTP proxy request content-type was not JSON: {content_type}"),
    ))
}

fn is_json_content_type(content_type: &str) -> bool {
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    media_type == "application/json"
        || (media_type.starts_with("application/") && media_type.ends_with("+json"))
}

fn method_allows_request_body(method: ModuleHttpMethod) -> bool {
    matches!(
        method,
        ModuleHttpMethod::Post
            | ModuleHttpMethod::Put
            | ModuleHttpMethod::Patch
            | ModuleHttpMethod::Delete
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{CorrelationId, RequestContext, RequestId, TraceContext};

    const AUTHORIZATION_HEADER: &str = "authorization";
    const CONNECTION_HEADER: &str = "connection";
    const COOKIE_HEADER: &str = "cookie";
    const X_FORWARDED_FOR_HEADER: &str = "x-forwarded-for";

    fn request_context() -> RequestContext {
        RequestContext {
            request_id: RequestId::new("req_test"),
            correlation_id: CorrelationId::new("corr_test"),
            trace: TraceContext {
                trace_id: Some("00000000000000000000000000000001".to_owned()),
                span_id: Some("0000000000000001".to_owned()),
                baggage: Vec::new(),
            },
            actor: Default::default(),
            tenant_id: None,
            causation_id: None,
        }
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT_HEADER, "application/json".parse().expect("header"));
        headers.insert(
            AUTHORIZATION_HEADER,
            "Bearer caller-token".parse().expect("header"),
        );
        headers.insert(COOKIE_HEADER, "session=caller".parse().expect("header"));
        headers.insert(X_FORWARDED_FOR_HEADER, "127.0.0.1".parse().expect("header"));
        headers.insert(CONNECTION_HEADER, "upgrade".parse().expect("header"));
        headers
    }

    #[test]
    fn forwards_only_allowed_get_headers_and_host_auth() {
        let client = reqwest::Client::new();
        let request = apply_proxy_request_policy(
            client.get("http://remote.test/contacts"),
            ModuleHttpMethod::Get,
            &headers(),
            &request_context(),
            Some("remote-token"),
            ProxyRequestBody::Empty,
        )
        .expect("policy applies")
        .build()
        .expect("request builds");

        assert_eq!(
            request
                .headers()
                .get(ACCEPT_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            request
                .headers()
                .get(X_REQUEST_ID_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("req_test")
        );
        assert_eq!(
            request
                .headers()
                .get(X_CORRELATION_ID_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("corr_test")
        );
        assert_eq!(
            request
                .headers()
                .get(TRACEPARENT_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("00-00000000000000000000000000000001-0000000000000001-01")
        );
        assert_eq!(
            request
                .headers()
                .get(AUTHORIZATION_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer remote-token")
        );
        assert!(!request.headers().contains_key(COOKIE_HEADER));
        assert!(!request.headers().contains_key(X_FORWARDED_FOR_HEADER));
        assert!(!request.headers().contains_key(CONNECTION_HEADER));
    }

    #[test]
    fn accepts_json_body_for_future_body_methods() {
        let mut headers = headers();
        headers.insert(
            CONTENT_TYPE_HEADER,
            "application/vnd.lenso+json; charset=utf-8"
                .parse()
                .expect("header"),
        );
        let client = reqwest::Client::new();
        let request = apply_proxy_request_policy(
            client.post("http://remote.test/contacts"),
            ModuleHttpMethod::Post,
            &headers,
            &request_context(),
            None,
            ProxyRequestBody::Json(Bytes::from_static(br#"{"name":"Ada"}"#)),
        )
        .expect("policy applies")
        .build()
        .expect("request builds");

        assert_eq!(
            request
                .headers()
                .get(CONTENT_TYPE_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("application/vnd.lenso+json; charset=utf-8")
        );
        assert_eq!(
            request.body().and_then(reqwest::Body::as_bytes),
            Some(&b"{\"name\":\"Ada\"}"[..])
        );
    }

    #[test]
    fn rejects_non_json_body_content_type() {
        let mut headers = headers();
        headers.insert(CONTENT_TYPE_HEADER, "text/plain".parse().expect("header"));
        let client = reqwest::Client::new();

        let error = apply_proxy_request_policy(
            client.post("http://remote.test/contacts"),
            ModuleHttpMethod::Post,
            &headers,
            &request_context(),
            None,
            ProxyRequestBody::Json(Bytes::from_static(b"not json")),
        )
        .expect_err("policy rejects non-json content type");

        assert_eq!(error.code, ErrorCode::Validation);
        assert!(
            error
                .public_message
                .contains("request content-type was not JSON")
        );
    }

    #[test]
    fn rejects_oversized_json_body() {
        let mut headers = headers();
        headers.insert(
            CONTENT_TYPE_HEADER,
            "application/json".parse().expect("header"),
        );
        let client = reqwest::Client::new();

        let error = apply_proxy_request_policy(
            client.post("http://remote.test/contacts"),
            ModuleHttpMethod::Post,
            &headers,
            &request_context(),
            None,
            ProxyRequestBody::Json(Bytes::from(vec![b'x'; MAX_PROXY_REQUEST_BYTES + 1])),
        )
        .expect_err("policy rejects oversized body");

        assert_eq!(error.code, ErrorCode::Validation);
        assert!(error.public_message.contains("request body exceeded"));
    }

    #[test]
    fn rejects_body_for_get() {
        let mut headers = headers();
        headers.insert(
            CONTENT_TYPE_HEADER,
            "application/json".parse().expect("header"),
        );
        let client = reqwest::Client::new();

        let error = apply_proxy_request_policy(
            client.get("http://remote.test/contacts"),
            ModuleHttpMethod::Get,
            &headers,
            &request_context(),
            None,
            ProxyRequestBody::Json(Bytes::from_static(br#"{}"#)),
        )
        .expect_err("policy rejects GET body");

        assert_eq!(error.code, ErrorCode::Validation);
        assert!(error.public_message.contains("GET does not accept"));
    }
}
