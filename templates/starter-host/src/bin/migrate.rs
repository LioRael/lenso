use anyhow::Context as _;
use platform_core::{AppConfig, apply_migrations, connect_pool, telemetry};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::try_from_env().context("invalid application configuration")?;
    telemetry::init(&config.telemetry)?;

    let pool = connect_pool(&config.database).await?;
    let migrations = app_bootstrap::migrations_for_config(&config)?;
    info!(count = migrations.len(), "applying Lenso migrations");
    apply_migrations(&pool, &migrations).await?;

    Ok(())
}
