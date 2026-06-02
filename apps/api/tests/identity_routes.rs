use app_api::build_router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_core::{
    AppConfig, AppContext, DatabaseConfig, LoggingEventPublisher, PLATFORM_MIGRATIONS,
    apply_migrations,
};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn create_user_route_persists_user_and_returns_conflict_for_duplicate() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(identity::migrations::IDENTITY_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations should apply");

    let mut config = AppConfig::from_env();
    config.database = DatabaseConfig {
        url: db.url.clone(),
        max_connections: 5,
    };
    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    let app = build_router(ctx);

    let first_response = app
        .clone()
        .oneshot(create_user_request("ada@example.com"))
        .await
        .expect("request should complete");
    assert_eq!(first_response.status(), StatusCode::OK);

    let duplicate_response = app
        .oneshot(create_user_request("ada@example.com"))
        .await
        .expect("request should complete");
    assert_eq!(duplicate_response.status(), StatusCode::CONFLICT);

    db.cleanup().await;
}

#[tokio::test]
async fn missing_request_headers_generate_ids() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(create_user_request("ada@example.com"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-request-id").is_some());
    assert!(response.headers().get("x-correlation-id").is_some());

    db.cleanup().await;
}

#[tokio::test]
async fn provided_request_headers_are_preserved() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            create_user_request("ada@example.com")
                .map(|body| body)
                .with_header("x-request-id", "req-provided")
                .with_header("x-correlation-id", "corr-provided"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "req-provided"
    );
    assert_eq!(
        response.headers().get("x-correlation-id").unwrap(),
        "corr-provided"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn traceparent_is_preserved_in_outbox_headers() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            create_user_request("ada@example.com")
                .with_header("x-request-id", "req-trace")
                .with_header("x-correlation-id", "corr-trace")
                .with_header(
                    "traceparent",
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                ),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let headers: Value = sqlx::query_scalar(
        r#"
        select headers
        from platform.outbox
        where correlation_id = 'corr-trace'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("outbox headers should query");

    assert_eq!(
        headers["trace"]["trace_id"],
        "4bf92f3577b34da6a3ce929d0e0e4736"
    );
    assert_eq!(headers["trace"]["span_id"], "00f067aa0ba902b7");

    db.cleanup().await;
}

#[tokio::test]
async fn request_without_traceparent_generates_outbox_trace_context() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            create_user_request("ada@example.com")
                .with_header("x-request-id", "req-generated-trace")
                .with_header("x-correlation-id", "corr-generated-trace"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let headers: Value = sqlx::query_scalar(
        r#"
        select headers
        from platform.outbox
        where correlation_id = 'corr-generated-trace'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("outbox headers should query");

    assert_eq!(
        headers["trace"]["trace_id"]
            .as_str()
            .unwrap_or_default()
            .len(),
        32
    );
    assert_eq!(
        headers["trace"]["span_id"]
            .as_str()
            .unwrap_or_default()
            .len(),
        16
    );

    db.cleanup().await;
}

#[tokio::test]
async fn validation_error_returns_standard_shape() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            create_user_request("not-an-email")
                .with_header("x-request-id", "req-validation")
                .with_header("x-correlation-id", "corr-validation"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["message"], "Request validation failed");
    assert_eq!(body["error"]["request_id"], "req-validation");
    assert_eq!(body["error"]["correlation_id"], "corr-validation");
    assert!(body["error"]["details"].as_array().unwrap().is_empty());

    db.cleanup().await;
}

#[tokio::test]
async fn duplicate_email_returns_409_standard_shape() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    app.clone()
        .oneshot(create_user_request("ada@example.com"))
        .await
        .expect("request should complete");

    let response = app
        .oneshot(
            create_user_request("ada@example.com")
                .with_header("x-request-id", "req-conflict")
                .with_header("x-correlation-id", "corr-conflict"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "conflict");
    assert_eq!(
        body["error"]["message"],
        "A user with this email already exists"
    );
    assert_eq!(body["error"]["request_id"], "req-conflict");
    assert_eq!(body["error"]["correlation_id"], "corr-conflict");

    db.cleanup().await;
}

#[tokio::test]
async fn malformed_json_returns_standard_shape() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/identity/users")
                .header("content-type", "application/json")
                .header("x-request-id", "req-json")
                .header("x-correlation-id", "corr-json")
                .body(Body::from(r#"{"email":"ada@example.com""#))
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

    db.cleanup().await;
}

#[tokio::test]
async fn me_requires_authentication() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/identity/me")
                .header("x-request-id", "req-me-missing")
                .header("x-correlation-id", "corr-me-missing")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "unauthorized");
    assert_eq!(body["error"]["request_id"], "req-me-missing");
    assert_eq!(body["error"]["correlation_id"], "corr-me-missing");

    db.cleanup().await;
}

#[tokio::test]
async fn dev_user_can_call_me() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/identity/me")
                .header("authorization", "Bearer dev-user:user_123")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["user_id"], "user_123");

    db.cleanup().await;
}

#[tokio::test]
async fn dev_service_cannot_call_user_only_me() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/identity/me")
                .header("authorization", "Bearer dev-service:worker")
                .header("x-request-id", "req-me-service")
                .header("x-correlation-id", "corr-me-service")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "forbidden");
    assert_eq!(body["error"]["request_id"], "req-me-service");
    assert_eq!(body["error"]["correlation_id"], "corr-me-service");

    db.cleanup().await;
}

#[tokio::test]
async fn outbox_event_uses_request_correlation_and_http_causation_id() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            create_user_request("ada@example.com")
                .with_header("x-request-id", "req-outbox")
                .with_header("x-correlation-id", "corr-outbox"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let (correlation_id, causation_id): (String, Option<String>) =
        sqlx::query_as("select correlation_id, causation_id from platform.outbox limit 1")
            .fetch_one(&db.pool)
            .await
            .expect("outbox row should exist");

    assert_eq!(correlation_id, "corr-outbox");
    assert_eq!(causation_id.as_deref(), Some("httpreq_req-outbox"));

    db.cleanup().await;
}

#[tokio::test]
async fn outbox_event_includes_authenticated_actor_context() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            create_user_request("grace@example.com")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let headers: Value = sqlx::query_scalar("select headers from platform.outbox limit 1")
        .fetch_one(&db.pool)
        .await
        .expect("outbox row should exist");

    assert_eq!(headers["actor"]["kind"], "user");
    assert_eq!(headers["actor"]["user_id"], "user_123");

    db.cleanup().await;
}

fn create_user_request(email: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/identity/users")
        .header("content-type", "application/json")
        .body(Body::from(format!(
            r#"{{"email":"{email}","display_name":"Ada"}}"#
        )))
        .expect("request should build")
}

async fn test_app(db: &TestDatabase) -> axum::Router {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(identity::migrations::IDENTITY_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations should apply");

    let mut config = AppConfig::from_env();
    config.database = DatabaseConfig {
        url: db.url.clone(),
        max_connections: 5,
    };
    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    build_router(ctx)
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice(&bytes).expect("body should be json")
}

trait RequestBuilderExt {
    fn with_header(self, name: &'static str, value: &'static str) -> Self;
}

impl RequestBuilderExt for Request<Body> {
    fn with_header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers_mut().insert(name, value.parse().unwrap());
        self
    }
}
