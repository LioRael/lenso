use chrono::Utc;
use identity::models::user::{User, UserId};
use identity::repositories::{PostgresUserRepository, UserRepository};
use platform_core::{ErrorCode, PLATFORM_MIGRATIONS, apply_migrations};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;

#[tokio::test]
async fn postgres_repository_creates_user_and_rejects_duplicate_email() {
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

    let repository = PostgresUserRepository::new(db.pool.clone());
    let now = Utc::now();
    let user = User {
        id: UserId("usr_test".to_owned()),
        email: "ada@example.com".to_owned(),
        display_name: Some("Ada".to_owned()),
        created_at: now,
        updated_at: now,
    };

    repository.insert(&user).await.expect("user should insert");

    let duplicate_error = repository
        .insert(&User {
            id: UserId("usr_duplicate".to_owned()),
            ..user.clone()
        })
        .await
        .expect_err("duplicate email should fail");

    assert_eq!(duplicate_error.code, ErrorCode::Conflict);

    let row_count: i64 = sqlx::query_scalar(
        r#"
        select count(*)
        from identity.users
        where email = $1
        "#,
    )
    .bind("ada@example.com")
    .fetch_one(&db.pool)
    .await
    .expect("row count query should succeed");

    assert_eq!(row_count, 1);

    db.cleanup().await;
}
