use anyhow::Context as _;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "remote_module_example=info".to_owned()),
        )
        .init();

    let address: SocketAddr = std::env::var("REMOTE_MODULE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:4100".to_owned())
        .parse()
        .context("invalid REMOTE_MODULE_ADDR")?;

    tracing::info!(%address, "starting remote module example");
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, remote_module_example::router()).await?;
    Ok(())
}
