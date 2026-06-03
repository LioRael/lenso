use anyhow::Context as _;
use app_api::build_router;
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PostgresRuntimeConfigProvider,
    RuntimeConfigRegistry, connect_pool, telemetry,
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

    // Build the editable runtime-config registry from every domain and install it for
    // the console handlers and the API's own reads.
    let descriptors = app_bootstrap::runtime_config_descriptors(&ctx);
    let registry = RuntimeConfigRegistry::try_new(descriptors)
        .context("duplicate runtime-config descriptor registered")?;
    platform_admin::install_runtime_config_registry(registry.clone());
    let admin_modules = app_bootstrap::load_admin_modules(&ctx)
        .await
        .context("failed to load admin modules")?;
    platform_admin_data::install_admin_modules(admin_modules);

    let runtime_config =
        PostgresRuntimeConfigProvider::connect(ctx.db.clone(), Arc::new(registry), "api")
            .await
            .context("failed to load runtime-config snapshot")?;
    runtime_config.spawn_listener();
    ctx = ctx.with_runtime_config_provider(runtime_config);
    let admin_refresh_ctx = ctx.clone();
    platform_admin_data::install_admin_module_refresh_fn(move || {
        let ctx = admin_refresh_ctx.clone();
        async move { app_bootstrap::load_admin_modules(&ctx).await }
    });

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
