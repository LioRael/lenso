use chrono::Utc;
use platform_core::{
    IdempotencyClaim, IdempotencyKey, OutboxEvent, OutboxPublisher, apply_migrations,
    claim_idempotency_key_in_tx,
};
use serde_json::json;

mod support;
use support::TestDatabase;

#[tokio::test]
async fn idempotency_business_write_and_outbox_share_one_transaction() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .unwrap();
    sqlx::query("create table app_runs (id text primary key, state text not null)")
        .execute(&db.pool)
        .await
        .unwrap();

    let key = IdempotencyKey::parse("echo-void:create-run", "request-1").unwrap();
    let first_event = event("event-1", "run-1");
    let mut first = db.pool.begin().await.unwrap();
    assert_eq!(
        claim_idempotency_key_in_tx(&mut first, &key).await.unwrap(),
        IdempotencyClaim::Acquired
    );
    sqlx::query("insert into app_runs (id, state) values ($1, $2)")
        .bind("run-1")
        .bind("allocated")
        .execute(&mut *first)
        .await
        .unwrap();
    OutboxPublisher
        .publish_in_tx(&mut first, &first_event)
        .await
        .unwrap();
    first.commit().await.unwrap();

    let mut replay = db.pool.begin().await.unwrap();
    assert_eq!(
        claim_idempotency_key_in_tx(&mut replay, &key)
            .await
            .unwrap(),
        IdempotencyClaim::Existing
    );
    replay.commit().await.unwrap();

    assert_eq!(count(&db.pool, "app_runs").await, 1);
    assert_eq!(count(&db.pool, "platform.outbox").await, 1);
    assert_eq!(count(&db.pool, "platform.idempotency_claims").await, 1);

    let rolled_back_key = IdempotencyKey::parse("echo-void:create-run", "request-2").unwrap();
    let mut rolled_back = db.pool.begin().await.unwrap();
    assert_eq!(
        claim_idempotency_key_in_tx(&mut rolled_back, &rolled_back_key)
            .await
            .unwrap(),
        IdempotencyClaim::Acquired
    );
    sqlx::query("insert into app_runs (id, state) values ($1, $2)")
        .bind("run-2")
        .bind("allocated")
        .execute(&mut *rolled_back)
        .await
        .unwrap();
    OutboxPublisher
        .publish_in_tx(&mut rolled_back, &event("event-2", "run-2"))
        .await
        .unwrap();
    rolled_back.rollback().await.unwrap();

    assert_eq!(count(&db.pool, "app_runs").await, 1);
    assert_eq!(count(&db.pool, "platform.outbox").await, 1);
    assert_eq!(count(&db.pool, "platform.idempotency_claims").await, 1);

    db.cleanup().await;
}

fn event(id: &str, aggregate_id: &str) -> OutboxEvent {
    OutboxEvent {
        id: id.into(),
        event_name: "run.allocated".into(),
        event_version: 1,
        source_module: "echo-void".into(),
        aggregate_type: "run".into(),
        aggregate_id: aggregate_id.into(),
        correlation_id: "first-shot".into(),
        causation_id: None,
        occurred_at: Utc::now(),
        payload: json!({ "run_id": aggregate_id }),
        headers: json!({}),
    }
}

async fn count(pool: &platform_core::DbPool, table: &str) -> i64 {
    let query = format!("select count(*) from {table}");
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await
        .unwrap()
}
