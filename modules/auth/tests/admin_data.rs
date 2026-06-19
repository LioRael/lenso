use auth::admin::AuthAdminData;
use auth::models::{AuthUser, AuthUserId};
use auth::repositories::{AuthUserRepository, PostgresAuthUserRepository};
use chrono::{Duration, Utc};
use platform_core::{PLATFORM_MIGRATIONS, apply_migrations};
use platform_module::{AdminActionSource, AdminDataSource, AdminListQuery};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use std::sync::Arc;

async fn seed(repo: &PostgresAuthUserRepository, id: &str) {
    repo.insert(&AuthUser {
        id: AuthUserId(id.to_owned()),
        created_at: Utc::now(),
        disabled_at: None,
    })
    .await
    .expect("insert should succeed");
}

#[tokio::test]
async fn admin_data_lists_auth_users_with_cursor_pagination() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(auth::migrations::AUTH_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations apply");

    let repo = PostgresAuthUserRepository::new(db.pool.clone());
    seed(&repo, "usr_a").await;
    seed(&repo, "usr_b").await;
    seed(&repo, "usr_c").await;

    let admin = AuthAdminData::new(Arc::new(repo));
    let page1 = admin
        .list("users", &AdminListQuery::new(2, None))
        .await
        .expect("list page 1");
    assert_eq!(page1.records.len(), 2);
    assert_eq!(page1.records[0]["id"], "usr_a");
    assert_eq!(page1.records[1]["id"], "usr_b");

    let page2 = admin
        .list(
            "users",
            &AdminListQuery::new(2, Some(page1.next_cursor.expect("cursor"))),
        )
        .await
        .expect("list page 2");
    assert_eq!(page2.records.len(), 1);
    assert_eq!(page2.records[0]["id"], "usr_c");
    assert!(page2.next_cursor.is_none());

    let one = admin.get("users", "usr_a").await.expect("get");
    assert_eq!(one.expect("some")["id"], "usr_a");

    db.cleanup().await;
}

#[tokio::test]
async fn admin_data_lists_auth_sessions_without_token_hashes() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(auth::migrations::AUTH_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations apply");

    let repo = PostgresAuthUserRepository::new(db.pool.clone());
    let now = Utc::now();
    repo.create_dev_session(
        AuthUserId("usr_sessions".to_owned()),
        "sess_a".to_owned(),
        "token_a".to_owned(),
        now,
        now + Duration::hours(1),
    )
    .await
    .expect("session should be created");

    let admin = AuthAdminData::new(Arc::new(repo));
    let page = admin
        .list("sessions", &AdminListQuery::new(10, None))
        .await
        .expect("list sessions");
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0]["id"], "sess_a");
    assert_eq!(page.records[0]["user_id"], "usr_sessions");
    assert!(page.records[0].get("token_hash").is_none());

    let one = admin.get("sessions", "sess_a").await.expect("get session");
    assert_eq!(one.expect("some")["id"], "sess_a");

    db.cleanup().await;
}

#[tokio::test]
async fn admin_action_revokes_auth_session() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(auth::migrations::AUTH_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations apply");

    let repo = PostgresAuthUserRepository::new(db.pool.clone());
    let now = Utc::now();
    repo.create_dev_session(
        AuthUserId("usr_revoke".to_owned()),
        "sess_revoke".to_owned(),
        "token_revoke".to_owned(),
        now,
        now + Duration::hours(1),
    )
    .await
    .expect("session should be created");

    let admin = AuthAdminData::new(Arc::new(repo));
    let result = admin
        .invoke(
            "revoke_session",
            serde_json::json!({"session_id": "sess_revoke"}),
        )
        .await
        .expect("revoke session");
    assert_eq!(result["revoked"], true);

    let one = admin
        .get("sessions", "sess_revoke")
        .await
        .expect("get session")
        .expect("session");
    assert!(one["revoked_at"].as_str().is_some());
    assert!(one.get("token_hash").is_none());

    db.cleanup().await;
}
