#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lenso_worker::run_from_env().await
}
