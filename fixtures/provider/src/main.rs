use anyhow::Context as _;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "provider_fixture=info".to_owned()),
        )
        .init();

    let address: SocketAddr = std::env::var("PROVIDER_MODULE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:4100".to_owned())
        .parse()
        .context("invalid PROVIDER_MODULE_ADDR")?;

    if std::env::args().any(|arg| arg == "--grpc") {
        provider_fixture::grpc::serve_grpc(address).await?;
        return Ok(());
    }

    tracing::info!(%address, "starting Provider Service fixture");
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, provider_fixture::app()).await?;
    Ok(())
}
