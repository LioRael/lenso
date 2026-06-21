use platform_core::apply_migrations;
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;

#[tokio::test]
async fn runtime_migrations_create_function_runs_summary_index() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, RUNTIME_MIGRATIONS)
        .await
        .expect("runtime migrations should apply");

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
        where index_namespace.nspname = 'runtime'
            and index_class.relname = 'function_runs_status_created_at_idx'
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
async fn runtime_migrations_create_function_runs_story_indexes() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, RUNTIME_MIGRATIONS)
        .await
        .expect("runtime migrations should apply");

    let indexes: Vec<String> = sqlx::query_scalar(
        r#"
        select indexname
        from pg_indexes
        where schemaname = 'runtime'
            and tablename = 'function_runs'
            and indexname in (
                'function_runs_story_correlation_idx',
                'function_runs_story_updated_idx'
            )
        order by indexname
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("story indexes should query");

    assert_eq!(
        indexes,
        [
            "function_runs_story_correlation_idx",
            "function_runs_story_updated_idx"
        ]
    );

    db.cleanup().await;
}
