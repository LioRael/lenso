use axum::http::StatusCode;
use axum::{Json, Router, routing::post};
use platform_core::{
    EventHandlerRegistry, ExecutionContext, OutboxRelay, PLATFORM_MIGRATIONS, apply_migrations,
};
use platform_module_remote::{RemoteEventHandler, RemoteEventHostActionRunner, RemoteModuleConfig};
use platform_runtime::{
    FunctionDefinition, FunctionHandler, FunctionRegistry, RUNTIME_MIGRATIONS, RetryPolicy,
    RuntimeClient,
};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
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
async fn outbox_relay_runs_remote_event_handler_enqueue_action() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_remote_event_stack_migrations(&db).await;

    let remote = spawn_remote(event_enqueue_action_router()).await;
    insert_outbox_event(&db.pool, "evt_remote_1", 3).await;

    let function_registry = Arc::new(remote_function_registry());
    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(remote_handler(&remote).with_host_action_runner(
        RemoteEventHostActionRunner::new(
            RuntimeClient::new(db.pool.clone()),
            function_registry,
            ["remote_crm.sync_contact.v1".to_owned()],
        ),
    )));
    let relay = OutboxRelay::new(db.pool.clone(), "worker-remote");
    let count = relay
        .relay_once(&registry, 10)
        .await
        .expect("remote event handler action should dispatch");

    assert_eq!(count, 1);
    assert_eq!(event_status(&db.pool, "evt_remote_1").await, "published");
    let run = function_run(&db.pool, "remote_crm.sync_contact.v1").await;
    assert_eq!(run.status, "pending");
    assert_eq!(run.max_attempts, 5);
    assert_eq!(run.correlation_id, "corr_remote_event_1");
    assert_eq!(run.input_json["contact_id"], "usr_1");
    assert_eq!(
        run.input_json["_lenso_runtime"]["causation_id"],
        "remote_event_handler:evt_remote_1:sync_contact_on_user_registered:0"
    );
    assert_eq!(run.actor["kind"], "user");
    assert_eq!(run.actor["user_id"], "usr_actor");

    db.cleanup().await;
}

#[tokio::test]
async fn outbox_relay_rejects_remote_event_handler_undeclared_enqueue_action() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_remote_event_stack_migrations(&db).await;

    let remote = spawn_remote(event_undeclared_enqueue_action_router()).await;
    insert_outbox_event(&db.pool, "evt_remote_1", 3).await;

    let function_registry = Arc::new(remote_function_registry());
    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(remote_handler(&remote).with_host_action_runner(
        RemoteEventHostActionRunner::new(
            RuntimeClient::new(db.pool.clone()),
            function_registry,
            ["remote_crm.sync_contact.v1".to_owned()],
        ),
    )));
    OutboxRelay::new(db.pool.clone(), "worker-remote")
        .relay_once(&registry, 10)
        .await
        .expect("relay should handle invalid remote event action");

    assert_eq!(
        event_status_and_attempts(&db.pool, "evt_remote_1").await,
        ("dead".to_owned(), 1)
    );
    assert_eq!(function_run_count(&db.pool).await, 0);

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

fn event_enqueue_action_router() -> Router {
    Router::new().route(
        "/events/handlers/sync_contact_on_user_registered/invoke",
        post(event_enqueue_action),
    )
}

fn event_undeclared_enqueue_action_router() -> Router {
    Router::new().route(
        "/events/handlers/sync_contact_on_user_registered/invoke",
        post(event_undeclared_enqueue_action),
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

fn remote_function_registry() -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    registry.register(FunctionDefinition {
        name: "remote_crm.sync_contact.v1".to_owned(),
        version: 1,
        queue: "remote-crm".to_owned(),
        retry_policy: RetryPolicy::fixed(5, Duration::from_millis(250)),
        handler: Arc::new(NoopFunction),
    });
    registry
}

async fn event_success(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({
        "accepted": true,
        "event_id": body["outbox_event_id"],
    }))
}

async fn event_enqueue_action(Json(body): Json<Value>) -> Json<Value> {
    Json(json!({
        "actions": [{
            "type": "enqueue_function",
            "function_name": "remote_crm.sync_contact.v1",
            "input": {
                "contact_id": body["aggregate_id"],
                "email": body["payload"]["email"]
            }
        }]
    }))
}

async fn event_undeclared_enqueue_action() -> Json<Value> {
    Json(json!({
        "actions": [{
            "type": "enqueue_function",
            "function_name": "identity.cleanup_expired_sessions.v1",
            "input": {}
        }]
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

async fn apply_remote_event_stack_migrations(db: &TestDatabase) {
    apply_migrations(&db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");
    apply_migrations(&db.pool, RUNTIME_MIGRATIONS)
        .await
        .expect("runtime migrations should apply");
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

#[derive(Debug)]
struct NoopFunction;

#[async_trait::async_trait]
impl FunctionHandler for NoopFunction {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> platform_core::AppResult<Value> {
        Ok(Value::Null)
    }
}

struct FunctionRunRow {
    status: String,
    max_attempts: i32,
    correlation_id: String,
    input_json: Value,
    actor: Value,
}

async fn function_run(pool: &platform_core::DbPool, function_name: &str) -> FunctionRunRow {
    let row: (String, i32, String, Value, Value) = sqlx::query_as(
        r#"
        select status, max_attempts, correlation_id, input_json, actor
        from runtime.function_runs
        where function_name = $1
        "#,
    )
    .bind(function_name)
    .fetch_one(pool)
    .await
    .expect("function run should query");

    FunctionRunRow {
        status: row.0,
        max_attempts: row.1,
        correlation_id: row.2,
        input_json: row.3,
        actor: row.4,
    }
}

async fn function_run_count(pool: &platform_core::DbPool) -> i64 {
    sqlx::query_scalar("select count(*) from runtime.function_runs")
        .fetch_one(pool)
        .await
        .expect("function run count should query")
}
