use platform_core::settings::store::upsert_value;
use platform_core::{
    PLATFORM_MIGRATIONS, PostgresSettingsProvider, SettingDescriptor, SettingScope, SettingType,
    SettingsProvider, SettingsRegistry, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

fn registry() -> SettingsRegistry {
    SettingsRegistry::try_new(vec![SettingDescriptor {
        key: "demo.ttl_minutes",
        scope: SettingScope::Shared,
        value_type: SettingType::Int { min: Some(1), max: Some(1000) },
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
        PostgresSettingsProvider::connect(test_db.pool.clone(), Arc::new(registry()), "api")
            .await
            .expect("connect provider");

    // Default before any write.
    assert_eq!(provider.snapshot().raw("demo.ttl_minutes"), Some(&json!(30)));

    upsert_value(&test_db.pool, "*", "demo.ttl_minutes", &json!(90), Some("test"))
        .await
        .expect("upsert");

    provider.refresh().await.expect("refresh");
    assert_eq!(provider.snapshot().raw("demo.ttl_minutes"), Some(&json!(90)));

    test_db.cleanup().await;
}
