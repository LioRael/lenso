use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, Router, routing::post};
use platform_core::{ClaimedOutboxEvent, EventHandler};
use platform_module_remote::{RemoteEventHandler, RemoteModuleConfig};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct InvokeState {
    observed: Arc<Mutex<Vec<ObservedInvoke>>>,
}

#[derive(Debug)]
struct ObservedInvoke {
    authorization: Option<String>,
    body: Value,
}

async fn spawn_server(router: Router) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test server should run");
    });
    format!("http://{address}")
}

fn claimed_event() -> ClaimedOutboxEvent {
    ClaimedOutboxEvent {
        id: "evt_remote_1".to_owned(),
        event_name: "identity.user_registered.v1".to_owned(),
        event_version: 1,
        source_module: "identity".to_owned(),
        aggregate_type: "user".to_owned(),
        aggregate_id: "usr_1".to_owned(),
        correlation_id: "corr_remote_event_1".to_owned(),
        causation_id: Some("httpreq_1".to_owned()),
        occurred_at: "2026-05-31T00:00:00Z"
            .parse()
            .expect("timestamp should parse"),
        payload: json!({
            "user_id": "usr_1",
            "email": "ada@example.com"
        }),
        headers: json!({
            "actor": {
                "kind": "user",
                "user_id": "usr_actor",
                "scopes": ["identity.users.write"]
            },
            "trace": {
                "trace_id": "00000000000000000000000000000001",
                "span_id": "0000000000000001",
                "baggage": [["region", "test"]]
            }
        }),
        attempts: 1,
        max_attempts: 3,
    }
}

#[tokio::test]
async fn remote_event_handler_posts_outbox_event_and_accepts_success() {
    let state = InvokeState::default();
    let base_url = spawn_server(
        Router::new()
            .route(
                "/events/handlers/sync_contact_on_user_registered/invoke",
                post(successful_invoke),
            )
            .with_state(state.clone()),
    )
    .await;
    let handler = RemoteEventHandler::new(
        RemoteModuleConfig::new("remote-crm", base_url).with_auth_token("remote-secret"),
        "sync_contact_on_user_registered",
        "identity.user_registered.v1",
    )
    .expect("event handler");

    handler
        .handle(&claimed_event())
        .await
        .expect("remote event handler should succeed");

    let observed = state.observed.lock().await;
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].authorization.as_deref(),
        Some("Bearer remote-secret")
    );
    assert_eq!(
        observed[0].body["request_id"],
        "evt_remote_1:sync_contact_on_user_registered"
    );
    assert_eq!(observed[0].body["outbox_event_id"], "evt_remote_1");
    assert_eq!(
        observed[0].body["handler_name"],
        "sync_contact_on_user_registered"
    );
    assert_eq!(
        observed[0].body["event_name"],
        "identity.user_registered.v1"
    );
    assert_eq!(observed[0].body["event_version"], 1);
    assert_eq!(observed[0].body["source_module"], "identity");
    assert_eq!(observed[0].body["aggregate_id"], "usr_1");
    assert_eq!(observed[0].body["correlation_id"], "corr_remote_event_1");
    assert_eq!(observed[0].body["causation_id"], "httpreq_1");
    assert_eq!(observed[0].body["actor"]["kind"], "user");
    assert_eq!(observed[0].body["actor"]["user_id"], "usr_actor");
    assert_eq!(
        observed[0].body["trace"]["trace_id"],
        "00000000000000000000000000000001"
    );
    assert_eq!(observed[0].body["payload"]["email"], "ada@example.com");
}

#[tokio::test]
async fn remote_event_handler_preserves_retryable_error_envelope() {
    let base_url = spawn_server(Router::new().route(
        "/events/handlers/sync_contact_on_user_registered/invoke",
        post(failing_invoke),
    ))
    .await;
    let handler = RemoteEventHandler::new(
        RemoteModuleConfig::new("remote-crm", base_url),
        "sync_contact_on_user_registered",
        "identity.user_registered.v1",
    )
    .expect("event handler");

    let error = handler
        .handle(&claimed_event())
        .await
        .expect_err("remote event handler should fail");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert_eq!(
        error.public_message,
        "remote CRM event sink was unavailable"
    );
    assert!(error.retryable);
    assert!(error.details.iter().any(|detail| {
        detail.field.as_deref() == Some("remote_status") && detail.reason == "503"
    }));
    assert!(error.details.iter().any(|detail| {
        detail.field.as_deref() == Some("remote_code")
            && detail.reason == "external_dependency_failure"
    }));
}

#[tokio::test]
async fn remote_event_handler_timeout_is_retryable() {
    let base_url = spawn_server(Router::new().route(
        "/events/handlers/sync_contact_on_user_registered/invoke",
        post(slow_invoke),
    ))
    .await;
    let handler = RemoteEventHandler::new(
        RemoteModuleConfig::new("remote-crm", base_url).with_timeout_ms(10),
        "sync_contact_on_user_registered",
        "identity.user_registered.v1",
    )
    .expect("event handler");

    let error = handler
        .handle(&claimed_event())
        .await
        .expect_err("remote event handler should time out");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert!(error.retryable);
    assert!(error.public_message.contains("request failed"));
}

#[test]
fn remote_event_handler_rejects_names_that_are_not_path_segments() {
    let error = RemoteEventHandler::new(
        RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
        "sync/contact",
        "identity.user_registered.v1",
    )
    .expect_err("slash should not be accepted in handler path segment");

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
}

async fn successful_invoke(
    State(state): State<InvokeState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    state.observed.lock().await.push(ObservedInvoke {
        authorization: headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        body,
    });
    Json(json!({ "accepted": true }))
}

async fn failing_invoke() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": {
                "code": "external_dependency_failure",
                "message": "remote CRM event sink was unavailable",
                "retryable": true,
                "details": [{ "field": "upstream", "reason": "timeout" }]
            }
        })),
    )
}

async fn slow_invoke() -> Json<Value> {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Json(json!({ "accepted": true }))
}
