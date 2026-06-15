#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app_worker::run_from_env().await
}
