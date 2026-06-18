use anyhow::Context as _;
use platform_core::{AppConfig, apply_migrations, connect_pool, telemetry};
use tracing::info;

pub async fn run_from_env() -> anyhow::Result<()> {
    run_from_env_with_composition(lenso_bootstrap::HostComposition::default()).await
}

pub async fn run_from_env_with_composition(
    composition: lenso_bootstrap::HostComposition,
) -> anyhow::Result<()> {
    let config = AppConfig::try_from_env().context("invalid application configuration")?;
    telemetry::init(&config.telemetry)?;

    let pool = connect_pool(&config.database).await?;
    let migrations = collect_migrations(&config, &composition)?;
    info!(count = migrations.len(), "applying migrations");
    apply_migrations(&pool, &migrations).await?;

    Ok(())
}

fn collect_migrations(
    config: &AppConfig,
    composition: &lenso_bootstrap::HostComposition,
) -> platform_core::AppResult<Vec<platform_core::Migration>> {
    lenso_bootstrap::migrations_for_config_with_composition(config, composition)
}
