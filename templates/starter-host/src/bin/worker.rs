#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lenso_host::run_worker_from_env().await
}
