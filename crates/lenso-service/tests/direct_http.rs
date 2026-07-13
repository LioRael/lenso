use http::{Method, StatusCode};
use lenso_service::{
    CallPolicyEvent, CallPolicyFailure, CallPolicyRuntime, CallPolicyTerminalOutcome,
    DirectHttpCall, DirectHttpClient, DirectHttpRequest, DirectHttpResponse,
    DirectHttpServerBinding, Endpoint, EndpointState, ManualCallPolicyClock, ServiceReference,
    StaticEndpointResolver, generate_direct_http_bindings,
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    assert_eq!(
        bindings.operations[0]
            .call_policy
            .circuit_breaker
            .as_ref()
            .unwrap()
            .failure_threshold,
        2
    );
    assert_eq!(
        bindings.operations[0]
            .call_policy
            .fallback
            .as_ref()
            .unwrap()
            .handler,
        "support.cached_ticket"
    );
    assert!(bindings.operations[1].request_schema.is_some());
    assert!(bindings.operations[1].standard_error_schema.is_some());
}

#[test]
fn invalid_http_call_policy_is_rejected_deterministically() {
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "invalid", "version": "v1"},
        "paths": {"/probe": {"post": {
            "operationId": "probe",
            "x-lenso-call-policy": {"maxAttempts": 2},
            "responses": {"200": {"description": "ok"}}
        }}}
    });
    assert_eq!(
        generate_direct_http_bindings("invalid", "v1", &document)
            .unwrap_err()
            .to_string(),
        "post /probe x-lenso-call-policy.maxAttempts: unsafe_retry_policy"
    );
}

#[test]
fn generated_http_bindings_require_an_explicit_call_policy() {
    let document = json!({
        "openapi": "3.1.0",
        "info": {"title": "missing", "version": "v1"},
        "paths": {"/probe": {"get": {
            "operationId": "probe",
            "x-lenso-idempotency": "idempotent",
            "responses": {"200": {"description": "ok"}}
        }}}
    });
    assert_eq!(
        generate_direct_http_bindings("missing", "v1", &document)
            .unwrap_err()
            .to_string(),
        "get /probe requires x-lenso-call-policy"
    );
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
async fn direct_policy_runs_without_planes_and_records_circuit_fallback_and_recovery() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let available = Arc::new(AtomicBool::new(false));
    let count = Arc::clone(&attempts);
    let serving = Arc::clone(&available);
    let server = DirectHttpServerBinding::new(bindings(), move |_| {
        let count = Arc::clone(&count);
        let serving = Arc::clone(&serving);
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            if serving.load(Ordering::SeqCst) {
                DirectHttpResponse::json(StatusCode::OK, json!({"source":"live"}))
            } else {
                DirectHttpResponse::json(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"code":"unavailable"}),
                )
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, server.router()).await.unwrap() });
    let healthy_server = DirectHttpServerBinding::new(bindings(), |_| async move {
        DirectHttpResponse::json(StatusCode::OK, json!({"source":"other-service"}))
    });
    let healthy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let healthy_address = healthy_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(healthy_listener, healthy_server.router())
            .await
            .unwrap()
    });
    let resolver = StaticEndpointResolver::new([
        EndpointState::new(
            ServiceReference::new("support"),
            vec![Endpoint::new(format!("http://{address}"))],
        ),
        EndpointState::new(
            ServiceReference::new("other-support"),
            vec![Endpoint::new(format!("http://{healthy_address}"))],
        ),
    ])
    .unwrap();
    let clock = Arc::new(ManualCallPolicyClock::new(1_000));
    let client = DirectHttpClient::new(resolver, bindings())
        .with_policy_runtime(CallPolicyRuntime::new(clock.clone()))
        .with_fallback("support.cached_ticket", |_| {
            DirectHttpResponse::json(StatusCode::OK, json!({"source":"cache"}))
        });
    let call = || {
        DirectHttpCall::new("getTicket")
            .with_path_parameter("ticket_id", "42")
            .with_deadline(4_102_444_800_000)
    };

    let first = client
        .call(&ServiceReference::new("support"), call())
        .await
        .unwrap();
    assert_eq!(first.status, StatusCode::SERVICE_UNAVAILABLE);
    let second = client
        .call(&ServiceReference::new("support"), call())
        .await
        .unwrap();
    assert_eq!(
        second.evidence.unwrap().call_policy.events,
        [
            CallPolicyEvent::RetryScheduled,
            CallPolicyEvent::CallFailed,
            CallPolicyEvent::CircuitOpened
        ]
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 4);

    let isolated = client
        .call(&ServiceReference::new("other-support"), call())
        .await
        .unwrap();
    assert_eq!(isolated.status, StatusCode::OK);

    let fallback = client
        .call(&ServiceReference::new("support"), call())
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fallback.body).unwrap()["source"],
        "cache"
    );
    let evidence = fallback.evidence.unwrap().call_policy;
    assert_eq!(evidence.attempts, 0);
    assert_eq!(
        evidence.terminal_outcome,
        CallPolicyTerminalOutcome::Fallback
    );
    assert_eq!(
        evidence.events,
        [
            CallPolicyEvent::CircuitOpen,
            CallPolicyEvent::FallbackApplied
        ]
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 4);

    clock.advance_ms(1_000);
    available.store(true, Ordering::SeqCst);
    let recovered = client
        .call(&ServiceReference::new("support"), call())
        .await
        .unwrap();
    assert_eq!(recovered.status, StatusCode::OK);
    let events = recovered.evidence.unwrap().call_policy.events;
    assert!(events.contains(&CallPolicyEvent::CircuitHalfOpen));
    assert!(events.contains(&CallPolicyEvent::CircuitRecovered));
    assert!(events.contains(&CallPolicyEvent::CallCompleted));
}

#[tokio::test]
async fn declared_http_deadline_fallback_is_supplied_by_composition() {
    let mut generated_bindings = bindings();
    generated_bindings.operations[0]
        .call_policy
        .fallback
        .as_mut()
        .unwrap()
        .on
        .push(CallPolicyFailure::DeadlineExpired);
    let resolver = StaticEndpointResolver::new(Vec::<EndpointState>::new()).unwrap();
    let response = DirectHttpClient::new(resolver, generated_bindings)
        .with_fallback("support.cached_ticket", |_| {
            DirectHttpResponse::json(StatusCode::OK, json!({"source":"deadline-cache"}))
        })
        .call(
            &ServiceReference::new("support"),
            DirectHttpCall::new("getTicket")
                .with_path_parameter("ticket_id", "42")
                .with_deadline(1),
        )
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::OK);
    let evidence = response.evidence.unwrap().call_policy;
    assert_eq!(evidence.attempts, 0);
    assert_eq!(
        evidence.events,
        [
            CallPolicyEvent::DeadlineExpired,
            CallPolicyEvent::FallbackApplied
        ]
    );
}

#[tokio::test]
async fn generated_server_rejects_declared_overload_before_business_handling() {
    let entered = Arc::new(tokio::sync::Barrier::new(2));
    let release = Arc::new(tokio::sync::Notify::new());
    let handler_entered = Arc::clone(&entered);
    let handler_release = Arc::clone(&release);
    let mut server_bindings = bindings();
    server_bindings
        .operation("getTicket")
        .expect("fixture operation")
        .call_policy
        .overload
        .as_ref()
        .expect("fixture overload declaration");
    server_bindings.operations[0]
        .call_policy
        .overload
        .as_mut()
        .unwrap()
        .max_in_flight = 1;
    let server = DirectHttpServerBinding::new(server_bindings, move |_| {
        let entered = Arc::clone(&handler_entered);
        let release = Arc::clone(&handler_release);
        async move {
            entered.wait().await;
            release.notified().await;
            DirectHttpResponse::json(StatusCode::OK, json!({"ok":true}))
        }
    });
    let first_server = server.clone();
    let first = tokio::spawn(async move {
        first_server
            .handle(
                DirectHttpRequest::new(Method::GET, "/v1/tickets/1")
                    .with_deadline(4_102_444_800_000),
            )
            .await
    });
    entered.wait().await;
    let rejected = server
        .handle(
            DirectHttpRequest::new(Method::GET, "/v1/tickets/2").with_deadline(4_102_444_800_000),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(rejected.evidence.unwrap().decision, "overload_rejected");
    release.notify_one();
    assert_eq!(first.await.unwrap().status, StatusCode::OK);
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
