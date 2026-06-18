#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lenso_migrate::run_from_env().await
}
