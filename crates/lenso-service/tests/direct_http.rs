use http::{Method, StatusCode};
use lenso_service::{
    DirectHttpCall, DirectHttpClient, DirectHttpRequest, DirectHttpResponse,
    DirectHttpServerBinding, Endpoint, EndpointState, ServiceReference, StaticEndpointResolver,
    generate_direct_http_bindings,
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

fn bindings() -> lenso_service::DirectHttpBindings {
    let document: serde_json::Value = serde_yaml::from_str(include_str!(
        "../fixtures/contracts/v2/support-http.v1.yaml"
    ))
    .unwrap();
    generate_direct_http_bindings("support-http", "v1", &document).unwrap()
}

#[test]
fn versioned_openapi_generates_protocol_preserving_bindings() {
    let bindings = bindings();
    assert_eq!(bindings.contract_id, "support-http");
    assert_eq!(bindings.version, "v1");
    assert_eq!(bindings.operations.len(), 2);
    assert_eq!(bindings.operations[0].operation_id, "getTicket");
    assert_eq!(bindings.operations[1].operation_id, "updateTicket");
    assert_eq!(bindings.operations[1].no_retry_reason(), None);
    assert!(bindings.operations[1].request_schema.is_some());
    assert!(bindings.operations[1].standard_error_schema.is_some());
}

#[tokio::test]
async fn server_rejects_expired_deadline_and_missing_key_before_business_handling() {
    let handled = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&handled);
    let server = DirectHttpServerBinding::new(bindings(), move |_| {
        let count = Arc::clone(&count);
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            DirectHttpResponse::json(StatusCode::OK, json!({"ok": true}))
        }
    });

    let expired = server
        .handle(DirectHttpRequest::new(Method::GET, "/v1/tickets/42").with_deadline(1))
        .await;
    assert_eq!(expired.status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(expired.evidence.unwrap().decision, "deadline_expired");

    let missing_key = server
        .handle(
            DirectHttpRequest::new(Method::POST, "/v1/tickets/42").with_deadline(now_ms() + 30_000),
        )
        .await;
    assert_eq!(missing_key.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        missing_key.evidence.unwrap().decision,
        "idempotency_key_required"
    );
    assert_eq!(handled.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn client_resolves_service_and_preserves_http_response_and_context() {
    let server = DirectHttpServerBinding::new(bindings(), |request| async move {
        assert_eq!(request.deadline_unix_ms, Some(4_102_444_800_000));
        assert_eq!(request.idempotency_key.as_deref(), Some("ticket-42:update"));
        let mut response = DirectHttpResponse::json(
            StatusCode::CONFLICT,
            json!({"type":"about:blank","title":"Conflict","status":409,"detail":"stale ticket","code":"conflict","request_id":"req-1","correlation_id":null,"errors":[]}),
        );
        response
            .headers
            .insert("x-request-id", "req-1".parse().unwrap());
        response
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });

    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();
    let client = DirectHttpClient::new(resolver, bindings());
    let response = client
        .call(
            &ServiceReference::new("support"),
            DirectHttpCall::new("updateTicket")
                .with_path_parameter("ticket_id", "42")
                .with_json(json!({"title": "updated"}))
                .with_deadline(4_102_444_800_000)
                .with_idempotency_key("ticket-42:update"),
        )
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::CONFLICT);
    assert_eq!(response.headers["x-request-id"], "req-1");
    assert_eq!(response.standard_error.unwrap()["code"], "conflict");
}

#[tokio::test]
async fn keyed_operation_retries_one_retryable_response() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let server = DirectHttpServerBinding::new(bindings(), move |_| {
        let count = Arc::clone(&count);
        async move {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                DirectHttpResponse::json(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"code":"external_dependency"}),
                )
            } else {
                DirectHttpResponse::json(StatusCode::OK, json!({"ok":true}))
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();

    let response = DirectHttpClient::new(resolver, bindings())
        .call(
            &ServiceReference::new("support"),
            DirectHttpCall::new("updateTicket")
                .with_path_parameter("ticket_id", "42")
                .with_deadline(now_ms() + 30_000)
                .with_idempotency_key("ticket-42:update"),
        )
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn client_bounds_attempts_by_the_absolute_deadline() {
    let server = DirectHttpServerBinding::new(bindings(), |_| async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        DirectHttpResponse::json(StatusCode::OK, json!({"ok": true}))
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();

    let error = DirectHttpClient::new(resolver, bindings())
        .call(
            &ServiceReference::new("support"),
            DirectHttpCall::new("getTicket")
                .with_path_parameter("ticket_id", "42")
                .with_deadline(now_ms() + 20),
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("transport_failure_no_retry"));
}

#[tokio::test]
async fn generated_client_requires_declared_path_parameters() {
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new("http://127.0.0.1:1")],
    )])
    .unwrap();
    let error = DirectHttpClient::new(resolver, bindings())
        .call(
            &ServiceReference::new("support"),
            DirectHttpCall::new("getTicket").with_deadline(now_ms() + 30_000),
        )
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "missing path parameter `ticket_id`");
}

#[tokio::test]
async fn generated_client_encodes_path_parameters_as_one_segment() {
    let seen_path = Arc::new(std::sync::Mutex::new(String::new()));
    let captured = Arc::clone(&seen_path);
    let server = DirectHttpServerBinding::new(bindings(), move |request| {
        *captured.lock().unwrap() = request.path;
        async { DirectHttpResponse::json(StatusCode::OK, json!({"ok": true})) }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();
    DirectHttpClient::new(resolver, bindings())
        .call(
            &ServiceReference::new("support"),
            DirectHttpCall::new("getTicket")
                .with_path_parameter("ticket_id", "ticket ?#%")
                .with_deadline(now_ms() + 30_000),
        )
        .await
        .unwrap();
    assert_eq!(*seen_path.lock().unwrap(), "/v1/tickets/ticket%20%3F%23%25");
}

#[test]
fn retry_policy_never_retries_unknown_or_unsafe_operations() {
    let bindings = bindings();
    let get = bindings.operation("getTicket").unwrap();
    let post = bindings.operation("updateTicket").unwrap();
    assert!(
        get.retry_decision(StatusCode::SERVICE_UNAVAILABLE, 1)
            .should_retry
    );
    assert_eq!(
        post.retry_decision(StatusCode::SERVICE_UNAVAILABLE, 1)
            .reason,
        "idempotency_key_required"
    );
    assert_eq!(
        get.retry_decision(StatusCode::BAD_REQUEST, 1).reason,
        "failure_not_retryable"
    );
    assert_eq!(
        get.retry_decision(StatusCode::SERVICE_UNAVAILABLE, 2)
            .reason,
        "initial_policy_attempt_limit"
    );
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
