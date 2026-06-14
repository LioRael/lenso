use axum::http::StatusCode;
use axum::{Json, Router, routing::post};
use platform_core::{EventHandlerRegistry, OutboxRelay, PLATFORM_MIGRATIONS, apply_migrations};
use platform_module_remote::{RemoteEventHandler, RemoteModuleConfig};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;

#[tokio::test]
async fn outbox_relay_publishes_remote_event_handler_success() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;

    let remote = spawn_remote(event_success_router()).await;
    insert_outbox_event(&db.pool, "evt_remote_1", 3).await;

    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(remote_handler(&remote)));
    let relay = OutboxRelay::new(db.pool.clone(), "worker-remote");
    let count = relay
        .relay_once(&registry, 10)
        .await
        .expect("remote event handler should dispatch");

    assert_eq!(count, 1);
    assert_eq!(event_status(&db.pool, "evt_remote_1").await, "published");
    assert!(
        execution_log_bodies(&db.pool, "evt_remote_1")
            .await
            .contains(&"Outbox event published".to_owned())
    );

    db.cleanup().await;
}

#[tokio::test]
async fn outbox_relay_retries_remote_event_handler_failure() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;

    let remote = spawn_remote(event_retryable_failure_router()).await;
    insert_outbox_event(&db.pool, "evt_remote_1", 3).await;

    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(remote_handler(&remote)));
    OutboxRelay::new(db.pool.clone(), "worker-remote")
        .relay_once(&registry, 10)
        .await
        .expect("relay should handle remote event failure");

    assert_eq!(
        event_status_and_attempts(&db.pool, "evt_remote_1").await,
        ("failed".to_owned(), 1)
    );

    db.cleanup().await;
}

#[tokio::test]
async fn outbox_relay_marks_exhausted_remote_event_handler_dead() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;

    let remote = spawn_remote(event_retryable_failure_router()).await;
    insert_outbox_event(&db.pool, "evt_remote_1", 1).await;

    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(remote_handler(&remote)));
    OutboxRelay::new(db.pool.clone(), "worker-remote")
        .relay_once(&registry, 10)
        .await
        .expect("relay should handle remote event failure");

    assert_eq!(
        event_status_and_attempts(&db.pool, "evt_remote_1").await,
        ("dead".to_owned(), 1)
    );

    db.cleanup().await;
}

async fn spawn_remote(router: Router) -> String {
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

fn event_success_router() -> Router {
    Router::new().route(
        "/events/handlers/sync_contact_on_user_registered/invoke",
        post(event_success),
    )
}

fn event_retryable_failure_router() -> Router {
    Router::new().route(
        "/events/handlers/sync_contact_on_user_registered/invoke",
        post(event_retryable_failure),
    )
}

fn remote_handler(base_url: &str) -> RemoteEventHandler {
    RemoteEventHandler::new(
        RemoteModuleConfig::new("remote-crm", base_url),
        "sync_contact_on_user_registered",
        "identity.user_registered.v1",
    )
    .expect("remote event handler")
}

async fn event_success(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({
        "accepted": true,
        "event_id": body["outbox_event_id"],
    }))
}

async fn event_retryable_failure() -> (StatusCode, Json<Value>) {
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

async fn apply_platform_migrations(db: &TestDatabase) {
    apply_migrations(&db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");
}

async fn insert_outbox_event(pool: &platform_core::DbPool, id: &str, max_attempts: i32) {
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
            headers,
            max_attempts
        )
        values (
            $1,
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_remote_event_1',
            'httpreq_1',
            now(),
            $2,
            $3,
            $4
        )
        "#,
    )
    .bind(id)
    .bind(json!({
        "user_id": "usr_1",
        "email": "ada@example.com"
    }))
    .bind(json!({
        "actor": {
            "kind": "user",
            "user_id": "usr_actor",
            "scopes": []
        },
        "trace": {
            "trace_id": "trace_remote_event_1",
            "span_id": "span_remote_event_1",
            "baggage": []
        }
    }))
    .bind(max_attempts)
    .execute(pool)
    .await
    .expect("outbox event should insert");
}

async fn event_status(pool: &platform_core::DbPool, id: &str) -> String {
    sqlx::query_scalar("select status from platform.outbox where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("event status should query")
}

async fn event_status_and_attempts(pool: &platform_core::DbPool, id: &str) -> (String, i32) {
    sqlx::query_as("select status, attempts from platform.outbox where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("event status should query")
}

async fn execution_log_bodies(pool: &platform_core::DbPool, id: &str) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        select body
        from platform.execution_logs
        where execution_id = $1
        order by occurred_at asc
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .expect("execution logs should query")
}
