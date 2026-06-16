use platform_core::runtime_config::store::{load_audit, upsert_value};
use platform_core::{
    PLATFORM_MIGRATIONS, PostgresRuntimeConfigProvider, RuntimeConfigDescriptor,
    RuntimeConfigGeneratedValue, RuntimeConfigProvider, RuntimeConfigRegistry, RuntimeConfigScope,
    RuntimeConfigType, RuntimeConfigVisibilityCondition, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

fn registry() -> RuntimeConfigRegistry {
    RuntimeConfigRegistry::try_new(vec![RuntimeConfigDescriptor {
        key: "demo.ttl_minutes".to_owned(),
        scope: RuntimeConfigScope::Shared,
        group: None,
        section: None,
        order: 0,
        visible_when: None,
        generated: None,
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

fn generated_registry() -> RuntimeConfigRegistry {
    RuntimeConfigRegistry::try_new(vec![
        RuntimeConfigDescriptor {
            key: "demo.mode".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: None,
            section: None,
            order: 0,
            visible_when: None,
            generated: None,
            value_type: RuntimeConfigType::Enum(&["basic", "secret"]),
            default: json!("basic"),
            editable: true,
            restart_only: true,
            description: "mode",
        },
        RuntimeConfigDescriptor {
            key: "demo.secret".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: None,
            section: None,
            order: 1,
            visible_when: Some(RuntimeConfigVisibilityCondition::Equals {
                service: "*",
                key: "demo.mode",
                value: json!("secret"),
            }),
            generated: Some(RuntimeConfigGeneratedValue::Secret {
                bytes: 32,
                when: RuntimeConfigVisibilityCondition::Equals {
                    service: "*",
                    key: "demo.mode",
                    value: json!("secret"),
                },
            }),
            value_type: RuntimeConfigType::String,
            default: json!(null),
            editable: true,
            restart_only: false,
            description: "secret",
        },
    ])
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

#[tokio::test]
async fn connect_generates_empty_generated_value_when_condition_is_true() {
    let Some(test_db) = TestDatabase::create().await else {
        return; // DATABASE_URL not set; skip.
    };
    apply_migrations(&test_db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    upsert_value(
        &test_db.pool,
        "*",
        "demo.mode",
        &json!("secret"),
        Some("test"),
    )
    .await
    .expect("upsert mode");
    upsert_value(&test_db.pool, "*", "demo.secret", &json!(""), Some("test"))
        .await
        .expect("upsert empty secret");

    let provider = PostgresRuntimeConfigProvider::connect(
        test_db.pool.clone(),
        Arc::new(generated_registry()),
        "api",
    )
    .await
    .expect("connect provider");

    let snapshot = provider.snapshot();
    let generated = snapshot
        .raw("demo.secret")
        .and_then(serde_json::Value::as_str)
        .expect("generated secret");
    assert_eq!(generated.len(), 64);

    let audit = load_audit(&test_db.pool, "*", "demo.secret", 10)
        .await
        .expect("audit");
    assert_eq!(audit.len(), 2);

    provider.refresh().await.expect("refresh");
    let audit = load_audit(&test_db.pool, "*", "demo.secret", 10)
        .await
        .expect("audit after refresh");
    assert_eq!(audit.len(), 2, "refresh should not regenerate");

    test_db.cleanup().await;
}
