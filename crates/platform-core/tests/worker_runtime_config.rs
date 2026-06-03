use platform_core::runtime_config::store::upsert_value;
use platform_core::worker_runtime_config::RUNTIME_CONFIG;
use platform_core::{
    PLATFORM_MIGRATIONS, PostgresRuntimeConfigProvider, RuntimeConfigProvider, RuntimeConfigRegistry,
    WorkerRuntimeConfig, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn worker_config_round_trips_through_postgres() {
    let Some(test_db) = TestDatabase::create().await else {
        return; // DATABASE_URL not set; skip.
    };
    apply_migrations(&test_db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).expect("registry");
    let provider =
        PostgresRuntimeConfigProvider::connect(test_db.pool.clone(), Arc::new(registry), "worker")
            .await
            .expect("connect provider");

    // Defaults before any write.
    let cfg: WorkerRuntimeConfig = provider.snapshot().get("worker").expect("worker config");
    assert_eq!(cfg.poll_interval_ms, 500);
    assert_eq!(cfg.batch_size, 25);

    // Write a worker-scoped override and refresh.
    upsert_value(&test_db.pool, "worker", "worker.batch_size", &json!(100), Some("test"))
        .await
        .expect("upsert");
    provider.refresh().await.expect("refresh");

    let cfg: WorkerRuntimeConfig = provider.snapshot().get("worker").expect("worker config");
    assert_eq!(cfg.batch_size, 100);
    assert_eq!(cfg.poll_interval_ms, 500); // untouched key keeps default

    test_db.cleanup().await;
}
