#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app_api::run_from_env().await
}
