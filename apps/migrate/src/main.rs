use platform_core::{AppConfig, PLATFORM_MIGRATIONS, apply_migrations, connect_pool, telemetry};
use platform_runtime::RUNTIME_MIGRATIONS;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    telemetry::init(&config.telemetry)?;

    let pool = connect_pool(&config.database).await?;
    let migrations = collect_migrations();
    info!(count = migrations.len(), "applying migrations");
    apply_migrations(&pool, &migrations).await?;

    Ok(())
}

fn collect_migrations() -> Vec<platform_core::Migration> {
    PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .chain(identity::migrations::IDENTITY_MIGRATIONS)
        .chain(notifications::migrations::NOTIFICATIONS_MIGRATIONS)
        .copied()
        .collect()
}
