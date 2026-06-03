use platform_core::{
    AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventDispatcher, OutboxRelay,
    PLATFORM_MIGRATIONS, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;

#[tokio::test]
async fn claim_does_not_double_claim_events() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;
    insert_outbox_event(&db.pool, "evt_1", 3).await;

    let first = OutboxRelay::new(db.pool.clone(), "worker-a")
        .claim_batch(10)
        .await
        .expect("first claim should succeed");
    let second = OutboxRelay::new(db.pool.clone(), "worker-b")
        .claim_batch(10)
        .await
        .expect("second claim should succeed");

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 0);

    db.cleanup().await;
}

#[tokio::test]
async fn relay_success_marks_event_published() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;
    insert_outbox_event(&db.pool, "evt_1", 3).await;

    let relay = OutboxRelay::new(db.pool.clone(), "worker-a");
    let count = relay
        .relay_once(&AlwaysSucceeds, 10)
        .await
        .expect("relay should succeed");

    assert_eq!(count, 1);
    assert_eq!(event_status(&db.pool, "evt_1").await, "published");
    assert_eq!(
        execution_log_bodies(&db.pool, "evt_1").await,
        vec![
            "Outbox event claimed".to_owned(),
            "Outbox event dispatch started".to_owned(),
            "Outbox event published".to_owned()
        ]
    );

    db.cleanup().await;
}

#[tokio::test]
async fn retryable_failure_increments_attempts_and_marks_failed_for_retry() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;
    insert_outbox_event(&db.pool, "evt_1", 3).await;

    let relay = OutboxRelay::new(db.pool.clone(), "worker-a");
    relay
        .relay_once(&AlwaysRetryableFailure, 10)
        .await
        .expect("relay should handle dispatcher failure");

    let (status, attempts) = event_status_and_attempts(&db.pool, "evt_1").await;
    assert_eq!(status, "failed");
    assert_eq!(attempts, 1);
    assert!(
        execution_log_bodies(&db.pool, "evt_1")
            .await
            .contains(&"Outbox event failed".to_owned())
    );

    db.cleanup().await;
}

#[tokio::test]
async fn exhausted_attempts_marks_event_dead() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_platform_migrations(&db).await;
    insert_outbox_event(&db.pool, "evt_1", 1).await;

    let relay = OutboxRelay::new(db.pool.clone(), "worker-a");
    relay
        .relay_once(&AlwaysRetryableFailure, 10)
        .await
        .expect("relay should handle dispatcher failure");

    let (status, attempts) = event_status_and_attempts(&db.pool, "evt_1").await;
    assert_eq!(status, "dead");
    assert_eq!(attempts, 1);

    db.cleanup().await;
}

#[derive(Debug)]
struct AlwaysSucceeds;

#[async_trait::async_trait]
impl EventDispatcher for AlwaysSucceeds {
    async fn dispatch(&self, _event: &ClaimedOutboxEvent) -> AppResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
struct AlwaysRetryableFailure;

#[async_trait::async_trait]
impl EventDispatcher for AlwaysRetryableFailure {
    async fn dispatch(&self, _event: &ClaimedOutboxEvent) -> AppResult<()> {
        Err(AppError::new(ErrorCode::ExternalDependency, "temporary failure").retryable())
    }
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
            occurred_at,
            payload,
            headers,
            max_attempts
        )
        values ($1, 'identity.user_registered.v1', 1, 'identity', 'user', 'usr_1', 'corr_1', now(), $2, '{}'::jsonb, $3)
        "#,
    )
    .bind(id)
    .bind(json!({ "user_id": "usr_1" }))
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
        .expect("status should query")
}

async fn event_status_and_attempts(pool: &platform_core::DbPool, id: &str) -> (String, i32) {
    sqlx::query_as("select status, attempts from platform.outbox where id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("status and attempts should query")
}

async fn execution_log_bodies(pool: &platform_core::DbPool, id: &str) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        select body
        from platform.execution_logs
        where execution_id = $1
        order by occurred_at asc, id asc
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .expect("execution log bodies should query")
}
