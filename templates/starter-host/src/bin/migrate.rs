#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lenso_host::run_migrations_from_env().await
}
