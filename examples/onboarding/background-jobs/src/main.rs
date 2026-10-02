use lenso_onboarding_background_jobs::{Plugin, health};
use lenso_web_host::NativeWebHost;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::task::LocalSet::new()
        .run_until(async {
            let without_jobs = std::env::args().any(|arg| arg == "--without-jobs");
            let mut host = NativeWebHost::new()
                .plugin::<health::Plugin>()
                .bind("127.0.0.1:0".parse()?);
            if !without_jobs {
                host = host.plugin::<Plugin>();
            }
            let running = host.start().await?;
            println!("Listening on http://{}", running.address());
            tokio::signal::ctrl_c().await?;
            running.shutdown().await?;
            println!(
                "{}",
                serde_json::json!({"event": "shutdown", "outcome": "clean"})
            );
            Ok(())
        })
        .await
}
