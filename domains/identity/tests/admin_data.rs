use chrono::Utc;
use identity::admin::IdentityAdminData;
use identity::models::user::{User, UserId};
use identity::repositories::{PostgresUserRepository, UserRepository};
use platform_core::{PLATFORM_MIGRATIONS, apply_migrations};
use platform_module::{AdminDataSource, AdminListQuery};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use std::sync::Arc;

async fn seed(repo: &PostgresUserRepository, id: &str, email: &str) {
    let now = Utc::now();
    repo.insert(&User {
        id: UserId(id.to_owned()),
        email: email.to_owned(),
        display_name: Some("Test".to_owned()),
        created_at: now,
        updated_at: now,
    })
    .await
    .expect("insert should succeed");
}

#[tokio::test]
async fn admin_data_lists_users_with_cursor_pagination() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(identity::migrations::IDENTITY_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations).await.expect("migrations apply");

    let repo = PostgresUserRepository::new(db.pool.clone());
    seed(&repo, "usr_a", "a@example.com").await;
    seed(&repo, "usr_b", "b@example.com").await;
    seed(&repo, "usr_c", "c@example.com").await;

    let admin = IdentityAdminData::new(Arc::new(repo));

    let page1 = admin.list("users", &AdminListQuery::new(2, None)).await.expect("list page 1");
    assert_eq!(page1.records.len(), 2);
    assert_eq!(page1.records[0]["id"], "usr_a");
    assert_eq!(page1.records[1]["id"], "usr_b");
    let cursor = page1.next_cursor.clone().expect("should have next cursor");
    assert_eq!(cursor, "usr_b");

    let page2 = admin.list("users", &AdminListQuery::new(2, Some(cursor))).await.expect("list page 2");
    assert_eq!(page2.records.len(), 1);
    assert_eq!(page2.records[0]["id"], "usr_c");
    assert!(page2.next_cursor.is_none());

    let one = admin.get("users", "usr_a").await.expect("get");
    assert_eq!(one.expect("some")["email"], "a@example.com");
    assert!(admin.get("users", "nope").await.expect("get none").is_none());

    assert!(admin.list("widgets", &AdminListQuery::new(10, None)).await.is_err());

    db.cleanup().await;
}
