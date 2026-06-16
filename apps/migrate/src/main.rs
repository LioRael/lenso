#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app_migrate::run_from_env().await
}
