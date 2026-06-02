use anyhow::Context as _;
use app_api::build_router;
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PostgresSettingsProvider, SettingsRegistry,
    connect_pool, telemetry,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let mut ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

    // Build the editable-settings registry from every domain and install it for
    // the console handlers and the API's own reads.
    let descriptors = app_bootstrap::setting_descriptors(&ctx);
    let registry = SettingsRegistry::try_new(descriptors)
        .context("duplicate setting descriptor registered")?;
    platform_admin::install_settings_registry(registry.clone());

    let settings = PostgresSettingsProvider::connect(ctx.db.clone(), Arc::new(registry), "api")
        .await
        .context("failed to load settings snapshot")?;
    settings.spawn_listener();
    ctx = ctx.with_settings_provider(settings);

    let app = build_router(ctx.clone());
    let address: SocketAddr = format!("{}:{}", ctx.config.http.host, ctx.config.http.port)
        .parse()
        .context("invalid HTTP bind address")?;

    info!(%address, "starting API server");
    let listener = tokio::net::TcpListener::bind(address).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            platform_core::Shutdown::wait_for_signal().await;
        })
        .await?;

    Ok(())
}
