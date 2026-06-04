use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, Router, routing::post};
use platform_core::{ActorContext, CorrelationId, ExecutionContext, ExecutionId, TraceContext};
use platform_module_remote::{RemoteModuleConfig, RemoteRuntimeFunction};
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

fn test_context() -> ExecutionContext {
    ExecutionContext {
        execution_id: ExecutionId("fnrun_remote_1".to_owned()),
        function_name: "remote_crm.sync_contact.v1".to_owned(),
        attempt: 2,
        queue: "remote-crm".to_owned(),
        correlation_id: CorrelationId::new("corr_remote_runtime_1"),
        causation_id: Some("httpreq_1".to_owned()),
        actor: ActorContext::Service {
            service_id: "worker".to_owned(),
            scopes: vec!["runtime.functions.invoke".to_owned()],
        },
        tenant_id: None,
        trace: TraceContext {
            trace_id: Some("00000000000000000000000000000001".to_owned()),
            span_id: Some("0000000000000001".to_owned()),
            baggage: vec![("region".to_owned(), "test".to_owned())],
        },
        deadline: None,
    }
}

#[tokio::test]
async fn remote_runtime_function_posts_invocation_and_returns_output() {
    let state = InvokeState::default();
    let base_url = spawn_server(
        Router::new()
            .route(
                "/runtime/functions/remote_crm.sync_contact.v1/invoke",
                post(successful_invoke),
            )
            .with_state(state.clone()),
    )
    .await;
    let function = RemoteRuntimeFunction::new(
        RemoteModuleConfig::new("remote-crm", base_url).with_auth_token("remote-secret"),
        "remote_crm.sync_contact.v1",
    )
    .expect("runtime function");

    let output = function
        .invoke(test_context(), json!({ "contact_id": "contact_1" }))
        .await
        .expect("remote invocation should succeed");

    assert_eq!(output, json!({ "synced": true, "id": "contact_1" }));
    let observed = state.observed.lock().await;
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].authorization.as_deref(),
        Some("Bearer remote-secret")
    );
    assert_eq!(observed[0].body["function_run_id"], "fnrun_remote_1");
    assert_eq!(
        observed[0].body["function_name"],
        "remote_crm.sync_contact.v1"
    );
    assert_eq!(observed[0].body["attempt"], 2);
    assert_eq!(observed[0].body["correlation_id"], "corr_remote_runtime_1");
    assert_eq!(observed[0].body["causation_id"], "httpreq_1");
    assert_eq!(observed[0].body["actor"]["kind"], "service");
    assert_eq!(observed[0].body["actor"]["service_id"], "worker");
    assert_eq!(
        observed[0].body["trace"]["trace_id"],
        "00000000000000000000000000000001"
    );
    assert_eq!(observed[0].body["input"]["contact_id"], "contact_1");
}

#[tokio::test]
async fn remote_runtime_function_preserves_retryable_error_envelope() {
    let base_url = spawn_server(Router::new().route(
        "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        post(failing_invoke),
    ))
    .await;
    let function = RemoteRuntimeFunction::new(
        RemoteModuleConfig::new("remote-crm", base_url),
        "remote_crm.sync_contact.v1",
    )
    .expect("runtime function");

    let error = function
        .invoke(test_context(), json!({ "contact_id": "contact_1" }))
        .await
        .expect_err("remote invocation should fail");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert_eq!(error.public_message, "remote CRM was unavailable");
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
async fn remote_runtime_function_timeout_is_retryable() {
    let base_url = spawn_server(Router::new().route(
        "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        post(slow_invoke),
    ))
    .await;
    let function = RemoteRuntimeFunction::new(
        RemoteModuleConfig::new("remote-crm", base_url).with_timeout_ms(10),
        "remote_crm.sync_contact.v1",
    )
    .expect("runtime function");

    let error = function
        .invoke(test_context(), json!({ "contact_id": "contact_1" }))
        .await
        .expect_err("remote invocation should time out");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert!(error.retryable);
    assert!(error.public_message.contains("request failed"));
}

#[test]
fn remote_runtime_function_rejects_names_that_are_not_path_segments() {
    let error = RemoteRuntimeFunction::new(
        RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
        "remote_crm/sync_contact.v1",
    )
    .expect_err("slash should not be accepted in function path segment");

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
}

#[tokio::test]
async fn remote_runtime_function_requires_json_success_response() {
    let base_url = spawn_server(Router::new().route(
        "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        post(text_invoke),
    ))
    .await;
    let function = RemoteRuntimeFunction::new(
        RemoteModuleConfig::new("remote-crm", base_url),
        "remote_crm.sync_contact.v1",
    )
    .expect("runtime function");

    let error = function
        .invoke(test_context(), json!({ "contact_id": "contact_1" }))
        .await
        .expect_err("text response should fail");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert!(
        error
            .public_message
            .contains("response content-type was not JSON")
    );
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
        body: body.clone(),
    });
    Json(json!({
        "output": {
            "synced": true,
            "id": body["input"]["contact_id"],
        }
    }))
}

async fn failing_invoke() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": {
                "code": "external_dependency_failure",
                "message": "remote CRM was unavailable",
                "retryable": true,
                "details": [{ "field": "upstream", "reason": "timeout" }]
            }
        })),
    )
}

async fn slow_invoke() -> Json<Value> {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Json(json!({ "output": {} }))
}

async fn text_invoke() -> &'static str {
    "not json"
}
