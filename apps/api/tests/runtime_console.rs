use app_api::build_router;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use platform_core::{
    apply_migrations, AppConfig, AppContext, DatabaseConfig, LoggingEventPublisher,
    PLATFORM_MIGRATIONS,
};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn admin_runtime_outbox_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_get("/admin/runtime/outbox"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_outbox_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn service_actor_can_list_outbox() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox?status=pending&event_name=identity.user_registered.v1&limit=10")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "evt_1");
    assert_eq!(body["data"][0]["event_name"], "identity.user_registered.v1");
    assert_eq!(body["data"][0]["status"], "pending");
    assert_eq!(body["data"][0]["attempts"], 0);
    assert_eq!(body["data"][0]["max_attempts"], 3);
    assert_eq!(body["data"][0]["locked_by"], Value::Null);
    assert_eq!(body["data"][0]["published_at"], Value::Null);
    assert_eq!(body["data"][0]["last_error"], Value::Null);
    assert_eq!(body["data"][0]["correlation_id"], "corr_1");
    assert!(body["data"][0]["available_at"].is_string());
    assert!(body["data"][0]["created_at"].is_string());
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_list_function_runs() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get(
                "/admin/runtime/functions?status=pending&function_name=notifications.send_welcome_email.v1&limit=10",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "fnrun_1");
    assert_eq!(
        body["data"][0]["function_name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(body["data"][0]["status"], "pending");
    assert_eq!(body["data"][0]["attempts"], 0);
    assert_eq!(body["data"][0]["max_attempts"], 3);
    assert_eq!(body["data"][0]["locked_by"], Value::Null);
    assert_eq!(body["data"][0]["started_at"], Value::Null);
    assert_eq!(body["data"][0]["completed_at"], Value::Null);
    assert_eq!(body["data"][0]["last_error"], Value::Null);
    assert_eq!(body["data"][0]["correlation_id"], "corr_1");
    assert!(body["data"][0]["available_at"].is_string());
    assert!(body["data"][0]["created_at"].is_string());
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_get_function_run_by_id() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/functions/fnrun_1")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "fnrun_1");
    assert_eq!(
        body["data"]["function_name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(body["data"]["status"], "pending");

    db.cleanup().await;
}

#[tokio::test]
async fn admin_runtime_openapi_contract_is_present() {
    let document = app_api::openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");

    assert_eq!(
        value["paths"]["/admin/runtime/outbox"]["get"]["operationId"],
        "admin_runtime_list_outbox"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions"]["get"]["operationId"],
        "admin_runtime_list_function_runs"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions/{id}"]["get"]["operationId"],
        "admin_runtime_get_function_run"
    );
    assert!(value["components"]["schemas"]["AdminOutboxEvent"].is_object());
    assert!(value["components"]["schemas"]["AdminFunctionRun"].is_object());
}

async fn test_app(db: &TestDatabase) -> axum::Router {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
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

fn auth_only_app() -> axum::Router {
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    build_router(ctx)
}

async fn insert_outbox_event(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into platform.outbox (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            occurred_at,
            payload,
            headers
        )
        values (
            'evt_1',
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_1',
            now(),
            $1,
            '{}'::jsonb
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .execute(pool)
    .await
    .expect("outbox event should insert");
}

async fn insert_function_run(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            correlation_id,
            actor
        )
        values (
            'fnrun_1',
            'notifications.send_welcome_email.v1',
            $1,
            'corr_1',
            '{"kind":"system"}'::jsonb
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .execute(pool)
    .await
    .expect("function run should insert");
}

fn admin_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice(&bytes).expect("body should be json")
}

trait RequestExt {
    fn with_header(self, name: &'static str, value: &'static str) -> Self;
}

impl RequestExt for Request<Body> {
    fn with_header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers_mut().insert(name, value.parse().unwrap());
        self
    }
}
