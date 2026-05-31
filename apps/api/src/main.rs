use anyhow::Context as _;
use app_api::build_router;
use platform_core::{connect_pool, telemetry, AppConfig, AppContext, LoggingEventPublisher};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

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
