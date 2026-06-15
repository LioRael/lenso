#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lenso_host::run_api_from_env().await
}
