use platform_core::{Migration, apply_migrations};
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

#[tokio::test]
async fn platform_migrations_create_outbox_summary_index() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");

    let indexed_columns: Vec<String> = sqlx::query_scalar(
        r#"
        select a.attname
        from pg_class index_class
        join pg_namespace index_namespace
            on index_namespace.oid = index_class.relnamespace
        join pg_index index_info
            on index_info.indexrelid = index_class.oid
        join pg_attribute a
            on a.attrelid = index_info.indrelid
            and a.attnum = any(index_info.indkey)
        where index_namespace.nspname = 'platform'
            and index_class.relname = 'outbox_status_created_at_idx'
        order by array_position(index_info.indkey::int[], a.attnum::int)
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("index columns should query");

    assert_eq!(indexed_columns, ["status", "created_at"]);

    db.cleanup().await;
}
