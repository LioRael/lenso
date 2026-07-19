use platform_core::{Migration, apply_migrations};

mod support;
use support::TestDatabase;

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

#[tokio::test]
async fn platform_migrations_create_outbox_story_indexes() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");

    let indexes: Vec<String> = sqlx::query_scalar(
        r#"
        select indexname
        from pg_indexes
        where schemaname = 'platform'
            and tablename = 'outbox'
            and indexname in (
                'outbox_story_correlation_idx',
                'outbox_story_updated_idx'
            )
        order by indexname
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("story indexes should query");

    assert_eq!(
        indexes,
        ["outbox_story_correlation_idx", "outbox_story_updated_idx"]
    );

    db.cleanup().await;
}

#[tokio::test]
async fn platform_migrations_create_remote_http_proxy_calls_table() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");

    let columns: Vec<String> = sqlx::query_scalar(
        r#"
        select column_name
        from information_schema.columns
        where table_schema = 'platform'
            and table_name = 'remote_http_proxy_calls'
            and column_name in (
                'module_name',
                'method',
                'declared_path',
                'remote_path',
                'remote_status',
                'success',
                'error_code',
                'request_id',
                'correlation_id'
            )
        order by column_name
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("remote proxy call columns should query");

    assert_eq!(
        columns,
        [
            "correlation_id",
            "declared_path",
            "error_code",
            "method",
            "module_name",
            "remote_path",
            "remote_status",
            "request_id",
            "success"
        ]
    );

    db.cleanup().await;
}

#[tokio::test]
async fn platform_migrations_create_idempotency_claims_table() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");

    let primary_key_columns: Vec<String> = sqlx::query_scalar(
        r#"
        select a.attname
        from pg_index i
        join pg_class table_class on table_class.oid = i.indrelid
        join pg_namespace namespace on namespace.oid = table_class.relnamespace
        join pg_attribute a
            on a.attrelid = table_class.oid
            and a.attnum = any(i.indkey)
        where namespace.nspname = 'platform'
            and table_class.relname = 'idempotency_claims'
            and i.indisprimary
        order by array_position(i.indkey::int[], a.attnum::int)
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("idempotency primary key should query");

    assert_eq!(primary_key_columns, ["scope", "key"]);
    db.cleanup().await;
}
