use chrono::Utc;
use lenso::host::outbox::{
    AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventDispatcher, OutboxRelay,
};
use lenso::host::transaction::{DbPool, LinkedTransaction, OutboxEvent};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PLATFORM_MIGRATIONS: &[&str] = &[
    include_str!("../../platform-core/migrations/0001_create_platform_schema.sql"),
    include_str!("../../platform-core/migrations/0002_create_outbox.sql"),
    include_str!("../../platform-core/migrations/0003_extend_outbox_delivery_fields.sql"),
    include_str!("../../platform-core/migrations/0004_add_outbox_summary_index.sql"),
    include_str!("../../platform-core/migrations/0005_create_execution_logs.sql"),
    include_str!("../../platform-core/migrations/0006_create_story_events.sql"),
    include_str!("../../platform-core/migrations/0007_create_config_schema.sql"),
    include_str!("../../platform-core/migrations/0008_create_remote_http_proxy_calls.sql"),
    include_str!("../../platform-core/migrations/0009_add_story_query_indexes.sql"),
    include_str!("../../platform-core/migrations/0010_create_idempotency_claims.sql"),
    include_str!("../../platform-core/migrations/0011_create_extraction_artifacts.sql"),
    include_str!("../../platform-core/migrations/0012_create_delivery_artifacts.sql"),
];

#[tokio::test]
async fn public_host_facade_redelivers_the_same_event_after_retryable_failure() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    database.apply_platform_migrations().await;
    sqlx::query("create table settlement_receipts (event_id text primary key)")
        .execute(&database.pool)
        .await
        .expect("consumer table should be created");

    let event = OutboxEvent {
        id: "evt_settlement_1".to_owned(),
        event_name: "echo_void.run_settled.v1".to_owned(),
        event_version: 1,
        source_module: "echo-void".to_owned(),
        aggregate_type: "run".to_owned(),
        aggregate_id: "run_1".to_owned(),
        correlation_id: "settlement_1".to_owned(),
        causation_id: None,
        occurred_at: Utc::now(),
        payload: json!({ "run_id": "run_1", "score": 42 }),
        headers: json!({}),
    };
    let mut transaction = LinkedTransaction::begin(&database.pool)
        .await
        .expect("transaction should begin");
    sqlx::query("insert into settlement_receipts (event_id) values ($1)")
        .bind(&event.id)
        .execute(&mut **transaction.sql())
        .await
        .expect("business write should succeed");
    transaction
        .publish_outbox(&event)
        .await
        .expect("Outbox event should publish");
    transaction
        .commit()
        .await
        .expect("transaction should commit");
    let receipt_count: i64 =
        sqlx::query_scalar("select count(*) from settlement_receipts where event_id = $1")
            .bind(&event.id)
            .fetch_one(&database.pool)
            .await
            .expect("business receipt should query");
    assert_eq!(receipt_count, 1);

    let dispatcher = RetryOnceDispatcher::default();
    let relay = OutboxRelay::new(database.pool.clone(), "public-facade-test");
    assert_eq!(
        relay
            .relay_once(&dispatcher, 10)
            .await
            .expect("first relay should handle the consumer failure"),
        1
    );

    tokio::time::sleep(Duration::from_millis(5_100)).await;

    assert_eq!(
        relay
            .relay_once(&dispatcher, 10)
            .await
            .expect("later relay should redeliver the event"),
        1
    );
    assert_eq!(
        dispatcher.observed(),
        vec![
            (event.id.clone(), event.payload.clone()),
            (event.id, event.payload),
        ]
    );

    database.cleanup().await;
}

#[derive(Debug, Default)]
struct RetryOnceDispatcher {
    observed: Mutex<Vec<(String, Value)>>,
}

impl RetryOnceDispatcher {
    fn observed(&self) -> Vec<(String, Value)> {
        self.observed
            .lock()
            .expect("observed event lock should not be poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl EventDispatcher for RetryOnceDispatcher {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        let mut observed = self
            .observed
            .lock()
            .expect("observed event lock should not be poisoned");
        observed.push((event.id.clone(), event.payload.clone()));
        if observed.len() == 1 {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                "consumer temporarily unavailable",
            )
            .retryable());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct TestDatabase {
    admin_url: String,
    name: String,
    pool: DbPool,
}

impl TestDatabase {
    async fn create() -> Option<Self> {
        let Ok(admin_url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping Postgres integration test: DATABASE_URL is not set");
            return None;
        };
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should follow Unix epoch")
            .as_nanos();
        let name = format!("lenso_public_outbox_{}_{suffix}", std::process::id());
        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&admin_url)
            .await
            .ok()?;
        let create_sql = format!(r#"create database "{name}""#);
        sqlx::query(sqlx::AssertSqlSafe(create_sql))
            .execute(&admin_pool)
            .await
            .ok()?;
        admin_pool.close().await;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url_with_name(&admin_url, &name))
            .await
            .ok()?;
        Some(Self {
            admin_url,
            name,
            pool,
        })
    }

    async fn apply_platform_migrations(&self) {
        for migration in PLATFORM_MIGRATIONS {
            sqlx::raw_sql(*migration)
                .execute(&self.pool)
                .await
                .expect("platform migration should apply");
        }
    }

    async fn cleanup(self) {
        self.pool.close().await;
        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.admin_url)
            .await
            .expect("admin database should reconnect");
        sqlx::query("select pg_terminate_backend(pid) from pg_stat_activity where datname = $1")
            .bind(&self.name)
            .execute(&admin_pool)
            .await
            .expect("test database connections should terminate");
        let drop_sql = format!(r#"drop database if exists "{}""#, self.name);
        sqlx::query(sqlx::AssertSqlSafe(drop_sql))
            .execute(&admin_pool)
            .await
            .expect("test database should be dropped");
        admin_pool.close().await;
    }
}

fn database_url_with_name(url: &str, name: &str) -> String {
    let without_query = url.split_once('?').map_or(url, |(base, _)| base);
    let slash = without_query
        .rfind('/')
        .expect("DATABASE_URL must include a database name");
    format!("{}/{name}", &without_query[..slash])
}
