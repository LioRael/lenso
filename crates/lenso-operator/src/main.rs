use anyhow::Result;
use kube::CustomResourceExt;
use lenso_operator::{LensoAutonomousService, LensoServiceProvider, run, run_autonomous};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--print-crd") {
        println!("{}", serde_yaml::to_string(&LensoServiceProvider::crd())?);
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--print-autonomous-crd") {
        println!("{}", serde_yaml::to_string(&LensoAutonomousService::crd())?);
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let namespace = std::env::var("LENSO_OPERATOR_NAMESPACE")
        .ok()
        .filter(|namespace| !namespace.trim().is_empty());
    let client = kube::Client::try_default().await?;
    match std::env::var("LENSO_OPERATOR_CONTROLLERS")
        .unwrap_or_else(|_| "all".to_owned())
        .as_str()
    {
        "provider" => run(client, namespace).await?,
        "autonomous" => run_autonomous(client, namespace).await?,
        "all" => {
            tokio::try_join!(
                run(client.clone(), namespace.clone()),
                run_autonomous(client, namespace)
            )?;
        }
        value => anyhow::bail!(
            "LENSO_OPERATOR_CONTROLLERS must be one of `all`, `provider`, or `autonomous`, got `{value}`"
        ),
    }
    Ok(())
}
