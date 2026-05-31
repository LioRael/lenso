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
async fn admin_runtime_outbox_detail_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox/evt_1")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_outbox_retry_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_post("/admin/runtime/outbox/evt_1/retry"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_timeline_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/timeline/corr_1")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_function_retry_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_post("/admin/runtime/functions/fnrun_1/retry")
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
    assert!(body["data"][0].get("payload").is_none());
    assert!(body["data"][0].get("headers").is_none());
    assert!(body["data"][0]["available_at"].is_string());
    assert!(body["data"][0]["created_at"].is_string());
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_outbox_detail() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox/evt_1")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "evt_1");
    assert_eq!(body["data"]["event_name"], "identity.user_registered.v1");
    assert_eq!(body["data"]["payload"]["user_id"], "usr_1");
    assert_eq!(body["data"]["actor"]["kind"], "service");
    assert_eq!(body["data"]["actor"]["service_id"], "api");
    assert_eq!(body["data"]["trace"]["trace_id"], "trace_1");
    assert_eq!(body["data"]["correlation_id"], "corr_1");
    assert_eq!(body["data"]["causation_id"], "req_1");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempts"], 0);
    assert_eq!(body["data"]["max_attempts"], 3);
    assert!(body["data"]["occurred_at"].is_string());
    assert!(body["data"]["created_at"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn unknown_outbox_detail_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox/missing")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_retry_failed_outbox_event() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event_with_status(&db.pool, "evt_failed", "failed", 2, Some("boom")).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/outbox/evt_failed/retry")
                .with_header("authorization", "Bearer dev-service:admin")
                .with_header("x-correlation-id", "corr-admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "evt_failed");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempts"], 2);
    assert_eq!(body["data"]["locked_by"], Value::Null);
    assert_eq!(body["data"]["last_error"], Value::Null);

    let row = outbox_retry_state(&db.pool, "evt_failed").await;
    assert_eq!(row.status, "pending");
    assert_eq!(row.attempts, 2);
    assert!(row.locked_at.is_none());
    assert!(row.locked_by.is_none());
    assert!(row.last_error.is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn retry_pending_outbox_event_returns_conflict() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event_with_status(&db.pool, "evt_pending", "pending", 0, None).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/outbox/evt_pending/retry")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "conflict");

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
    assert!(body["data"][0].get("input_json").is_none());
    assert!(body["data"][0].get("actor").is_none());
    assert!(body["data"][0]["available_at"].is_string());
    assert!(body["data"][0]["created_at"].is_string());
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_retry_dead_function_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run_with_status(&db.pool, "fnrun_dead", "dead", 3, Some("exhausted")).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/functions/fnrun_dead/retry")
                .with_header("authorization", "Bearer dev-service:admin")
                .with_header("x-correlation-id", "corr-admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "fnrun_dead");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempts"], 3);
    assert_eq!(body["data"]["locked_by"], Value::Null);
    assert_eq!(body["data"]["last_error"], Value::Null);

    let row = function_retry_state(&db.pool, "fnrun_dead").await;
    assert_eq!(row.status, "pending");
    assert_eq!(row.attempts, 3);
    assert!(row.locked_at.is_none());
    assert!(row.locked_by.is_none());
    assert!(row.last_error.is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn retry_unknown_function_run_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/functions/missing/retry")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");

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
    assert_eq!(body["data"]["input_json"]["user_id"], "usr_1");
    assert_eq!(body["data"]["actor"]["kind"], "system");
    assert_eq!(body["data"]["correlation_id"], "corr_1");
    assert_eq!(body["data"]["attempts"], 0);
    assert_eq!(body["data"]["max_attempts"], 3);
    assert!(body["data"]["available_at"].is_string());
    assert!(body["data"]["created_at"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_runtime_timeline() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_timeline_outbox_event(&db.pool).await;
    insert_timeline_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/timeline/corr_timeline?limit=10")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["type"], "outbox_event");
    assert_eq!(body["data"][0]["id"], "evt_timeline");
    assert_eq!(body["data"][0]["name"], "identity.user_registered.v1");
    assert_eq!(body["data"][0]["status"], "published");
    assert_eq!(body["data"][0]["attempts"], 1);
    assert_eq!(body["data"][0]["max_attempts"], 3);
    assert_eq!(body["data"][0]["correlation_id"], "corr_timeline");
    assert_eq!(body["data"][0]["completed_at"], "2026-05-31T00:00:30Z");
    assert_eq!(body["data"][1]["type"], "function_run");
    assert_eq!(body["data"][1]["id"], "fnrun_timeline");
    assert_eq!(
        body["data"][1]["name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(body["data"][1]["status"], "completed");
    assert_eq!(body["data"][1]["started_at"], "2026-05-31T00:01:10Z");
    assert_eq!(body["data"][1]["completed_at"], "2026-05-31T00:01:30Z");
    assert_eq!(body["page"]["limit"], 10);
    assert_eq!(body["order"], "created_at_asc");
    assert!(body["data"][0].get("payload").is_none());
    assert!(body["data"][1].get("input_json").is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn unknown_correlation_id_returns_empty_timeline() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/timeline/missing")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
    assert_eq!(body["order"], "created_at_asc");

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
        value["paths"]["/admin/runtime/outbox/{id}"]["get"]["operationId"],
        "admin_runtime_get_outbox"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions"]["get"]["operationId"],
        "admin_runtime_list_function_runs"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions/{id}"]["get"]["operationId"],
        "admin_runtime_get_function_run"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/timeline/{correlation_id}"]["get"]["operationId"],
        "admin_runtime_get_timeline"
    );
    assert!(value["components"]["schemas"]["AdminOutboxEvent"].is_object());
    assert!(value["components"]["schemas"]["AdminOutboxEventDetail"].is_object());
    assert!(value["components"]["schemas"]["AdminFunctionRun"].is_object());
    assert!(value["components"]["schemas"]["AdminFunctionRunDetail"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeTimelineItem"].is_object());
    assert_eq!(
        value["paths"]["/admin/runtime/outbox/{id}/retry"]["post"]["operationId"],
        "admin_runtime_retry_outbox"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions/{id}/retry"]["post"]["operationId"],
        "admin_runtime_retry_function_run"
    );
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
            causation_id,
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
            'req_1',
            now(),
            $1,
            $2
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .bind(json!({
        "actor": {
            "kind": "service",
            "service_id": "api",
            "scopes": []
        },
        "trace": {
            "trace_id": "trace_1",
            "span_id": "span_1"
        },
        "schema_ref": "contracts/events/identity/identity.user_registered.v1.schema.json"
    }))
    .execute(pool)
    .await
    .expect("outbox event should insert");
}

async fn insert_outbox_event_with_status(
    pool: &platform_core::DbPool,
    id: &str,
    status: &str,
    attempts: i32,
    last_error: Option<&str>,
) {
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
            headers,
            status,
            attempts,
            locked_at,
            locked_by,
            last_error
        )
        values (
            $1,
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_1',
            now(),
            $2,
            '{}'::jsonb,
            $3,
            $4,
            now(),
            'worker-a',
            $5
        )
        "#,
    )
    .bind(id)
    .bind(json!({ "user_id": "usr_1" }))
    .bind(status)
    .bind(attempts)
    .bind(last_error)
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

async fn insert_function_run_with_status(
    pool: &platform_core::DbPool,
    id: &str,
    status: &str,
    attempts: i32,
    last_error: Option<&str>,
) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            status,
            attempts,
            locked_at,
            locked_by,
            last_error,
            correlation_id,
            actor
        )
        values (
            $1,
            'notifications.send_welcome_email.v1',
            $2,
            $3,
            $4,
            now(),
            'worker-a',
            $5,
            'corr_1',
            '{"kind":"system"}'::jsonb
        )
        "#,
    )
    .bind(id)
    .bind(json!({ "user_id": "usr_1" }))
    .bind(status)
    .bind(attempts)
    .bind(last_error)
    .execute(pool)
    .await
    .expect("function run should insert");
}

async fn insert_timeline_outbox_event(pool: &platform_core::DbPool) {
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
            headers,
            status,
            attempts,
            max_attempts,
            locked_at,
            published_at,
            last_error,
            created_at
        )
        values (
            'evt_timeline',
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_timeline',
            '2026-05-31T00:00:00Z',
            $1,
            '{}'::jsonb,
            'published',
            1,
            3,
            '2026-05-31T00:00:05Z',
            '2026-05-31T00:00:30Z',
            null,
            '2026-05-31T00:00:00Z'
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .execute(pool)
    .await
    .expect("timeline outbox event should insert");
}

async fn insert_timeline_function_run(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            status,
            attempts,
            max_attempts,
            locked_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            actor,
            created_at,
            updated_at
        )
        values (
            'fnrun_timeline',
            'notifications.send_welcome_email.v1',
            $1,
            'completed',
            1,
            3,
            '2026-05-31T00:01:05Z',
            '2026-05-31T00:01:10Z',
            '2026-05-31T00:01:30Z',
            null,
            'corr_timeline',
            '{"kind":"system"}'::jsonb,
            '2026-05-31T00:01:00Z',
            '2026-05-31T00:01:30Z'
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .execute(pool)
    .await
    .expect("timeline function run should insert");
}

fn admin_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn admin_post(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
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

#[derive(Debug)]
struct RetryState {
    status: String,
    attempts: i32,
    locked_at: Option<chrono::DateTime<chrono::Utc>>,
    locked_by: Option<String>,
    last_error: Option<String>,
}

async fn outbox_retry_state(pool: &platform_core::DbPool, id: &str) -> RetryState {
    let (status, attempts, locked_at, locked_by, last_error) =
        sqlx::query_as::<_, (String, i32, Option<_>, Option<String>, Option<String>)>(
            "select status, attempts, locked_at, locked_by, last_error from platform.outbox where id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("outbox retry state should query");

    RetryState {
        status,
        attempts,
        locked_at,
        locked_by,
        last_error,
    }
}

async fn function_retry_state(pool: &platform_core::DbPool, id: &str) -> RetryState {
    let (status, attempts, locked_at, locked_by, last_error) =
        sqlx::query_as::<_, (String, i32, Option<_>, Option<String>, Option<String>)>(
            "select status, attempts, locked_at, locked_by, last_error from runtime.function_runs where id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("function retry state should query");

    RetryState {
        status,
        attempts,
        locked_at,
        locked_by,
        last_error,
    }
}
