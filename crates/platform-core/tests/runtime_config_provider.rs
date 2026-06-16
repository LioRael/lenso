use platform_core::runtime_config::store::upsert_value;
use platform_core::{
    PLATFORM_MIGRATIONS, PostgresRuntimeConfigProvider, RuntimeConfigDescriptor,
    RuntimeConfigProvider, RuntimeConfigRegistry, RuntimeConfigScope, RuntimeConfigType,
    apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

fn registry() -> RuntimeConfigRegistry {
    RuntimeConfigRegistry::try_new(vec![RuntimeConfigDescriptor {
        key: "demo.ttl_minutes".to_owned(),
        scope: RuntimeConfigScope::Shared,
        group: None,
        order: 0,
        value_type: RuntimeConfigType::Int {
            min: Some(1),
            max: Some(1000),
        },
        default: json!(30),
        editable: true,
        restart_only: false,
        description: "ttl",
    }])
    .unwrap()
}

#[tokio::test]
async fn refresh_picks_up_written_value() {
    let Some(test_db) = TestDatabase::create().await else {
        return; // DATABASE_URL not set; skip.
    };
    apply_migrations(&test_db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    let provider =
        PostgresRuntimeConfigProvider::connect(test_db.pool.clone(), Arc::new(registry()), "api")
            .await
            .expect("connect provider");

    // Default before any write.
    assert_eq!(
        provider.snapshot().raw("demo.ttl_minutes"),
        Some(&json!(30))
    );

    upsert_value(
        &test_db.pool,
        "*",
        "demo.ttl_minutes",
        &json!(90),
        Some("test"),
    )
    .await
    .expect("upsert");

    provider.refresh().await.expect("refresh");
    assert_eq!(
        provider.snapshot().raw("demo.ttl_minutes"),
        Some(&json!(90))
    );

    test_db.cleanup().await;
}
