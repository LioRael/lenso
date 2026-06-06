use platform_core::{
    AppResult, ClaimedOutboxEvent, EventHandler, EventHandlerRegistry, OutboxRelay,
    PLATFORM_MIGRATIONS, apply_migrations,
};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use serde_json::{Value, json};

#[tokio::test]
async fn user_registered_event_enqueues_welcome_email_function() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_notifications_stack_migrations(&db).await;
    insert_user_registered_outbox_event(&db.pool).await;

    let ctx = platform_core::AppContext::new(
        platform_core::AppConfig::from_env(),
        db.pool.clone(),
        std::sync::Arc::new(platform_core::LoggingEventPublisher),
    );
    let module = notifications::module::module(&ctx);
    let mut registry = EventHandlerRegistry::new();
    module.binding.register_event_handlers(&mut registry);

    let relay = OutboxRelay::new(db.pool.clone(), "worker-a");
    relay
        .relay_once(&registry, 10)
        .await
        .expect("outbox relay should dispatch user registered event");

    let row = welcome_email_function_run(&db.pool).await;
    assert_eq!(row.function_name, "notifications.send_welcome_email.v1");
    assert_eq!(row.status, "pending");
    assert_eq!(row.correlation_id, "corr_1");
    assert_eq!(row.input_json["user_id"], "usr_1");
    assert_eq!(row.input_json["email"], "ada@example.com");
    assert_eq!(row.input_json["_lenso_runtime"]["causation_id"], "evt_1");
    assert_eq!(
        row.input_json["_lenso_runtime"]["trace"]["trace_id"],
        "trace_1"
    );
    assert_eq!(
        row.input_json["_lenso_runtime"]["trace"]["span_id"],
        "span_1"
    );
    assert_eq!(row.actor["kind"], "user");
    assert_eq!(row.actor["user_id"], "usr_actor");

    db.cleanup().await;
}

#[tokio::test]
async fn user_registered_handler_failure_causes_outbox_retry_when_enqueue_fails() {
    let event = claimed_user_registered_event();
    let handler = notifications::events::WelcomeEmailRequestedHandler::new(FailingRuntimeClient);

    let error = handler
        .handle(&event)
        .await
        .expect_err("runtime enqueue failure should bubble up");

    assert!(error.retryable);
}

#[derive(Debug)]
struct FailingRuntimeClient;

#[async_trait::async_trait]
impl notifications::events::RuntimeEnqueuer for FailingRuntimeClient {
    async fn enqueue_welcome_email(
        &self,
        _event: &ClaimedOutboxEvent,
        _payload: Value,
    ) -> AppResult<String> {
        Err(platform_core::AppError::new(
            platform_core::ErrorCode::ExternalDependency,
            "runtime unavailable",
        )
        .retryable())
    }
}

#[derive(Debug)]
struct FunctionRunRow {
    function_name: String,
    input_json: Value,
    status: String,
    correlation_id: String,
    actor: Value,
}

async fn apply_notifications_stack_migrations(db: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(notifications::migrations::NOTIFICATIONS_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations should apply");
}

async fn insert_user_registered_outbox_event(pool: &platform_core::DbPool) {
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
    .bind(json!({
        "user_id": "usr_1",
        "email": "ada@example.com",
        "display_name": "Ada",
        "registered_at": "2026-05-31T00:00:00Z"
    }))
    .bind(json!({
        "actor": {
            "kind": "user",
            "user_id": "usr_actor",
            "scopes": []
        },
        "trace": {
            "trace_id": "trace_1",
            "span_id": "span_1",
            "baggage": []
        }
    }))
    .execute(pool)
    .await
    .expect("outbox event should insert");
}

async fn welcome_email_function_run(pool: &platform_core::DbPool) -> FunctionRunRow {
    let (function_name, input_json, status, correlation_id, actor) =
        sqlx::query_as::<_, (String, Value, String, String, Value)>(
            r#"
            select function_name, input_json, status, correlation_id, actor
            from runtime.function_runs
            where function_name = 'notifications.send_welcome_email.v1'
            "#,
        )
        .fetch_one(pool)
        .await
        .expect("welcome email function run should exist");

    FunctionRunRow {
        function_name,
        input_json,
        status,
        correlation_id,
        actor,
    }
}

fn claimed_user_registered_event() -> ClaimedOutboxEvent {
    ClaimedOutboxEvent {
        id: "evt_1".to_owned(),
        event_name: "identity.user_registered.v1".to_owned(),
        event_version: 1,
        source_module: "identity".to_owned(),
        aggregate_type: "user".to_owned(),
        aggregate_id: "usr_1".to_owned(),
        correlation_id: "corr_1".to_owned(),
        causation_id: Some("req_1".to_owned()),
        occurred_at: "2026-05-31T00:00:00Z"
            .parse()
            .expect("timestamp should parse"),
        payload: json!({
            "user_id": "usr_1",
            "email": "ada@example.com",
            "display_name": "Ada",
            "registered_at": "2026-05-31T00:00:00Z"
        }),
        headers: json!({
            "actor": {
                "kind": "user",
                "user_id": "usr_actor",
                "scopes": []
            },
            "trace": {
                "trace_id": "trace_1",
                "span_id": "span_1",
                "baggage": []
            }
        }),
        attempts: 0,
        max_attempts: 3,
    }
}
