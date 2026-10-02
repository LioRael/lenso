use lenso_onboarding_background_jobs::{JobRuntime, Plugin, health};
use lenso_web_host::NativeWebHost;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut without_jobs = false;
            let mut snapshot = None;
            let mut arguments = std::env::args().skip(1);
            while let Some(argument) = arguments.next() {
                match argument.as_str() {
                    "--without-jobs" => without_jobs = true,
                    "--state-file" => {
                        snapshot = Some(std::path::PathBuf::from(
                            arguments
                                .next()
                                .ok_or("--state-file requires a local path")?,
                        ));
                    }
                    _ => return Err(format!("unknown argument: {argument}").into()),
                }
            }
            let mut host = NativeWebHost::new()
                .plugin::<health::Plugin>()
                .bind("127.0.0.1:0".parse()?);
            if !without_jobs {
                host = host.configured_plugin::<Plugin, _>(move |plugin| {
                    plugin.configure(JobRuntime::default(), snapshot.clone())
                });
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
