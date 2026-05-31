use platform_core::{apply_migrations, Migration};
use platform_testing::TestDatabase;

#[tokio::test]
async fn applies_multi_statement_migrations() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    let migrations = [Migration {
        name: "test/0001_multi_statement_migration",
        sql: r#"
            create schema if not exists migration_test;

            create table if not exists migration_test.items (
                id integer primary key
            );

            create index if not exists items_id_idx
                on migration_test.items (id);
        "#,
    }];

    apply_migrations(&db.pool, &migrations)
        .await
        .expect("multi-statement migration should apply");

    let applied: bool =
        sqlx::query_scalar("select exists (select 1 from migration_test.items where id = 1)")
            .fetch_one(&db.pool)
            .await
            .expect("migration-created table should be queryable");
    assert!(!applied);

    db.cleanup().await;
}
