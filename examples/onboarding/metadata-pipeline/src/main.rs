use std::io::Write;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.iter().any(|value| {
        !matches!(
            value.as_str(),
            "--inspect" | "--without-normalizer" | "--without-summary"
        )
    }) {
        return Err("supported flags: --inspect --without-normalizer --without-summary".into());
    }
    let host = lenso_onboarding_metadata_pipeline::host(
        !arguments
            .iter()
            .any(|value| value == "--without-normalizer"),
        !arguments.iter().any(|value| value == "--without-summary"),
    );
    let plan = host.resolve_plan()?;
    if arguments.iter().any(|value| value == "--inspect") {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    tokio::task::LocalSet::new()
        .run_until(async move {
            let running = host.start().await?;
            println!("LISTENING http://{}", running.address());
            std::io::stdout().flush()?;
            tokio::signal::ctrl_c().await?;
            running.shutdown().await?;
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .await
}
