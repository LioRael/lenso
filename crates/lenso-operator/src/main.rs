use anyhow::Result;
use kube::CustomResourceExt;
use lenso_operator::{LensoServiceProvider, run};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--print-crd") {
        println!("{}", serde_yaml::to_string(&LensoServiceProvider::crd())?);
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let namespace = std::env::var("LENSO_OPERATOR_NAMESPACE")
        .ok()
        .filter(|namespace| !namespace.trim().is_empty());
    let client = kube::Client::try_default().await?;
    run(client, namespace).await
}
