use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use platform_http::{HttpRequestContext, JsonBody};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn missing_headers_generate_request_context_ids() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let request_id = response.headers().get("x-request-id").cloned();
    let correlation_id = response.headers().get("x-correlation-id").cloned();
    let body = json_body(response).await;

    assert!(request_id.is_some());
    assert!(correlation_id.is_some());
    assert!(body["request_id"].as_str().unwrap().starts_with("req_"));
    assert!(
        body["correlation_id"]
            .as_str()
            .unwrap()
            .starts_with("corr_")
    );
}

#[tokio::test]
async fn provided_headers_are_preserved_in_request_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("x-request-id", "req-provided")
                .header("x-correlation-id", "corr-provided")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "req-provided"
    );
    assert_eq!(
        response.headers().get("x-correlation-id").unwrap(),
        "corr-provided"
    );

    let body = json_body(response).await;
    assert_eq!(body["request_id"], "req-provided");
    assert_eq!(body["correlation_id"], "corr-provided");
}

#[tokio::test]
async fn dev_user_bearer_token_sets_user_actor_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("authorization", "Bearer dev-user:user_123")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "user");
    assert_eq!(body["actor"]["user_id"], "user_123");
}

#[tokio::test]
async fn dev_service_bearer_token_sets_service_actor_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("authorization", "Bearer dev-service:worker")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "service");
    assert_eq!(body["actor"]["service_id"], "worker");
}

#[tokio::test]
async fn malformed_json_returns_standard_error_shape_with_request_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/json")
                .header("content-type", "application/json")
                .header("x-request-id", "req-json")
                .header("x-correlation-id", "corr-json")
                .body(Body::from(r#"{"name":"Ada""#))
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["message"], "Request validation failed");
    assert_eq!(body["error"]["request_id"], "req-json");
    assert_eq!(body["error"]["correlation_id"], "corr-json");
    assert!(!body["error"]["details"].as_array().unwrap().is_empty());
}

fn router() -> Router {
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy db pool should construct"),
        Arc::new(LoggingEventPublisher),
    );

    Router::new()
        .route("/context", get(context_handler))
        .route("/json", post(json_handler))
        .layer(middleware::from_fn_with_state(
            ctx,
            platform_http::request_context_middleware,
        ))
}

async fn context_handler(HttpRequestContext(ctx): HttpRequestContext) -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "request_id": ctx.request_id.0,
        "correlation_id": ctx.correlation_id.0,
        "actor": ctx.actor,
    }))
}

async fn json_handler(JsonBody(input): JsonBody<JsonInput>) -> impl IntoResponse {
    axum::Json(serde_json::json!({ "name": input.name }))
}

#[derive(Debug, Deserialize)]
struct JsonInput {
    name: String,
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice(&bytes).expect("body should be json")
}
