use identity::commands::create_user::IdentityCommands;
use identity::public::{CreateUserCommand, IdentityService};
use identity::repositories::PostgresUserRepository;
use platform_core::{
    apply_migrations, CorrelationId, ErrorCode, LoggingEventPublisher, RequestContext, RequestId,
    PLATFORM_MIGRATIONS,
};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::{FixedClock, SequentialIdGenerator, TestDatabase};
use serde_json::Value;
use std::sync::Arc;

#[tokio::test]
async fn create_user_commits_user_and_outbox_event_atomically() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_identity_stack_migrations(&db).await;

    let command = IdentityCommands::new(
        Arc::new(PostgresUserRepository::new(db.pool.clone())),
        Arc::new(LoggingEventPublisher),
        Arc::new(fixed_clock()),
        Arc::new(SequentialIdGenerator::default()),
    );
    let ctx = request_context();

    let user = command
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "ada@example.com".to_owned(),
                display_name: Some("Ada".to_owned()),
            },
        )
        .await
        .expect("user should be created");

    let user_count: i64 = sqlx::query_scalar("select count(*) from identity.users")
        .fetch_one(&db.pool)
        .await
        .expect("user count should query");
    assert_eq!(user_count, 1);

    let outbox = fetch_outbox_row(&db.pool).await;
    assert_eq!(outbox.event_name, "identity.user_registered.v1");
    assert_eq!(outbox.event_version, 1);
    assert_eq!(outbox.aggregate_id, user.id.0);
    assert_eq!(outbox.correlation_id, "corr_1");
    assert_eq!(outbox.payload["user_id"], user.id.0);
    assert_eq!(outbox.payload["email"], "ada@example.com");

    db.cleanup().await;
}

#[tokio::test]
async fn duplicate_user_writes_no_new_user_or_outbox_event() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_identity_stack_migrations(&db).await;

    let command = IdentityCommands::new(
        Arc::new(PostgresUserRepository::new(db.pool.clone())),
        Arc::new(LoggingEventPublisher),
        Arc::new(fixed_clock()),
        Arc::new(SequentialIdGenerator::default()),
    );
    let ctx = request_context();

    command
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "ada@example.com".to_owned(),
                display_name: None,
            },
        )
        .await
        .expect("first user should be created");

    let duplicate_error = command
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "ada@example.com".to_owned(),
                display_name: None,
            },
        )
        .await
        .expect_err("duplicate user should fail");

    assert_eq!(duplicate_error.code, ErrorCode::Conflict);

    let user_count: i64 = sqlx::query_scalar("select count(*) from identity.users")
        .fetch_one(&db.pool)
        .await
        .expect("user count should query");
    let outbox_count: i64 = sqlx::query_scalar("select count(*) from platform.outbox")
        .fetch_one(&db.pool)
        .await
        .expect("outbox count should query");

    assert_eq!(user_count, 1);
    assert_eq!(outbox_count, 1);

    db.cleanup().await;
}

#[derive(Debug)]
struct OutboxRow {
    event_name: String,
    event_version: i32,
    aggregate_id: String,
    correlation_id: String,
    payload: Value,
}

async fn fetch_outbox_row(pool: &platform_core::DbPool) -> OutboxRow {
    let (event_name, event_version, aggregate_id, correlation_id, payload) =
        sqlx::query_as::<_, (String, i32, String, String, Value)>(
            r#"
            select event_name, event_version, aggregate_id, correlation_id, payload
            from platform.outbox
            limit 1
            "#,
        )
        .fetch_one(pool)
        .await
        .expect("outbox row should exist");

    OutboxRow {
        event_name,
        event_version,
        aggregate_id,
        correlation_id,
        payload,
    }
}

async fn apply_identity_stack_migrations(db: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(identity::migrations::IDENTITY_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations should apply");
}

fn request_context() -> RequestContext {
    RequestContext::new(RequestId::new("req_1"), CorrelationId::new("corr_1"))
}

fn fixed_clock() -> FixedClock {
    FixedClock::new(
        "2026-05-31T00:00:00Z"
            .parse()
            .expect("fixed timestamp should parse"),
    )
}
