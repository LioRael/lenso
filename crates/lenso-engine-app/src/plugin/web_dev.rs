use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, bail};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;
use tokio::process::{Child, Command as TokioCommand};

use crate::watch::SourceWatcher;

use super::{
    CargoPackage, DevImplementationArg, PluginDevArgs, cargo_target_directory, project_root,
    read_package,
    scaffold::{LENSO_CORE_REVISION, LENSO_NATIVE_REVISION, LENSO_WEB_REVISION},
};

const HOST_SOURCE: &str = r#"
use lenso_kernel::RuntimeFailure;
use lenso_web_host::{
    NativeWebHost, TowerMiddlewareOutcome, WebIngressDiagnostics, WebIngressEndpointFailure,
};
use tokio::task::LocalSet;

#[derive(Debug)]
struct DevDiagnostics;

impl WebIngressDiagnostics for DevDiagnostics {
    fn endpoint_runtime_failure(&self, event: WebIngressEndpointFailure<'_>) {
        if std::env::var_os("LENSO_WEB_DEV_JSON").is_some() {
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": 1,
                    "kind": "lenso.web-endpoint-failure",
                    "request_id": event.request_id(),
                    "route_id": event.route_id(),
                    "provider_index": event.provider_index(),
                    "failure": format!("{:?}", event.failure()),
                })
            );
        } else {
            println!(
                "Endpoint {} failed for request {} on provider {}: {:?}",
                event.route_id(),
                event.request_id(),
                event.provider_index(),
                event.failure(),
            );
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> {
    LocalSet::new().run_until(run()).await
}

async fn run() -> Result<(), String> {
    plugin::link();
    let running = NativeWebHost::new()
        .bind(
            "127.0.0.1:0"
                .parse()
                .map_err(|error| format!("parse development listener address: {error}"))?,
        )
        .with_diagnostics(DevDiagnostics)
        // The Host owns the only ingress middleware adapter. The policy
        // service intentionally passes requests through so Dev still exercises
        // the real normalization/credential/route path without wrapping it.
        .with_tower_middleware(
            "lenso.web-dev.pass-through@1",
            tower::service_fn(|_request| async {
                Ok::<_, RuntimeFailure>(TowerMiddlewareOutcome::Continue)
            }),
        )
        .enable(plugin::PACKAGE_ID)
        .start()
        .await
        .map_err(|error| format!("start Web development App: {error}"))?;
    let address = running.address();
    let routes = running
        .route_manifest()
        .ok_or_else(|| "Web Ingress did not publish its route manifest".to_owned())?;

    if std::env::var_os("LENSO_WEB_DEV_JSON").is_some() {
        println!(
            "{}",
            serde_json::json!({
                "schema_version": 1,
                "kind": "lenso.web-dev-ready",
                "address": format!("http://{address}"),
                "routes": routes.routes().iter().map(|route| serde_json::json!({
                    "method": route.method,
                    "path": route.path,
                    "route_id": route.route_id,
                })).collect::<Vec<_>>(),
            })
        );
    } else {
        println!("Lenso Web Plugin development server");
        println!("Listening on http://{address}");
        for route in routes.routes() {
            println!("  {:<7} {:<32} {}", route.method, route.path, route.route_id);
        }
        println!("Press Ctrl-C to stop.");
    }

    tokio::signal::ctrl_c()
        .await
        .map_err(|error| format!("listen for Ctrl-C: {error}"))?;
    running
        .shutdown()
        .await
        .map_err(|error| format!("shut down Web development App: {error}"))
}
"#;

pub(super) fn is_web_plugin(root: &Path) -> anyhow::Result<bool> {
    if root.join("package.json").is_file() {
        return Ok(false);
    }
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(false);
    }
    Ok(read_package(&manifest)?.metadata.lenso.root_slot == "web")
}

pub(super) async fn run(args: PluginDevArgs) -> anyhow::Result<()> {
    validate_args(&args)?;
    let root = project_root(args.repo_root.clone())?;
    let package = read_package(&root.join("Cargo.toml"))?;
    let host = DevHost::prepare(&root, &package)?;
    let mut watcher = args.watch.then(|| SourceWatcher::new(&root)).transpose()?;

    loop {
        host.build()?;
        let mut child = host.spawn(args.json)?;
        if let Some(watcher) = watcher.as_mut() {
            tokio::select! {
                result = child.wait() => {
                    let status = result.context("wait for Web development Host")?;
                    if !status.success() {
                        bail!("Web development Host exited with {status}");
                    }
                    return Ok(());
                }
                result = watcher.changed() => {
                    result?;
                    stop(&mut child).await;
                    if !args.json {
                        println!("Rebuilding Web Plugin after source changes.");
                    }
                }
                result = tokio::signal::ctrl_c() => {
                    result.context("listen for Ctrl-C")?;
                    wait_for_signal_shutdown(&mut child).await;
                    return Ok(());
                }
            }
        } else {
            tokio::select! {
                result = child.wait() => {
                    let status = result.context("wait for Web development Host")?;
                    if !status.success() {
                        bail!("Web development Host exited with {status}");
                    }
                    return Ok(());
                }
                result = tokio::signal::ctrl_c() => {
                    result.context("listen for Ctrl-C")?;
                    wait_for_signal_shutdown(&mut child).await;
                    return Ok(());
                }
            }
        }
    }
}

fn validate_args(args: &PluginDevArgs) -> anyhow::Result<()> {
    if args.operation.is_some() || args.request_json != "{}" {
        bail!(
            "Web Plugin development serves real HTTP; remove `--operation` and `--request-json`, then send requests to the printed listener address"
        );
    }
    if args.implementation != DevImplementationArg::Auto {
        bail!("Web Plugins use the linked native implementation; remove `--implementation`");
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct DevHost {
    project: TempDir,
    executable: PathBuf,
    project_root: PathBuf,
    target_directory: PathBuf,
}

impl DevHost {
    pub(super) fn prepare(root: &Path, package: &CargoPackage) -> anyhow::Result<Self> {
        let project = tempfile::tempdir().context("create Web development Host directory")?;
        let target_directory = cargo_target_directory(root)?;
        let package_name = host_package_name(root, &package.name);
        let manifest = host_manifest(root, package, &package_name);
        fs::write(project.path().join("Cargo.toml"), manifest)
            .context("write Web development Host manifest")?;
        fs::create_dir(project.path().join("src"))
            .context("create Web development Host source directory")?;
        fs::write(project.path().join("src/main.rs"), HOST_SOURCE)
            .context("write Web development Host source")?;
        run_cargo(
            project.path(),
            &target_directory,
            ["generate-lockfile"],
            "lock Web development Host",
        )?;
        let executable = target_directory.join("debug").join(if cfg!(windows) {
            format!("{package_name}.exe")
        } else {
            package_name
        });
        Ok(Self {
            project,
            executable,
            project_root: root.to_path_buf(),
            target_directory,
        })
    }

    pub(super) fn build(&self) -> anyhow::Result<()> {
        run_cargo(
            self.project.path(),
            &self.target_directory,
            ["build", "--locked"],
            "build Web development Host",
        )
    }

    fn spawn(&self, json: bool) -> anyhow::Result<Child> {
        let mut command = TokioCommand::new(&self.executable);
        command.current_dir(&self.project_root).kill_on_drop(true);
        if json {
            command.env("LENSO_WEB_DEV_JSON", "1");
        }
        command
            .spawn()
            .with_context(|| format!("start Web development Host `{}`", self.executable.display()))
    }
}

fn host_package_name(root: &Path, package_name: &str) -> String {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    let mut suffix = String::with_capacity(8);
    for byte in &digest[..4] {
        write!(&mut suffix, "{byte:02x}").expect("writing to a String cannot fail");
    }
    format!(
        "lenso-web-dev-{}-{}",
        package_name.replace('_', "-"),
        suffix
    )
}

fn host_manifest(root: &Path, package: &CargoPackage, host_package_name: &str) -> String {
    let plugin_path =
        serde_json::to_string(&root.to_string_lossy()).expect("serialize Plugin path");
    format!(
        r#"[package]
name = "{host_package_name}"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
futures = "0.3"
lenso-app-plan = {{ version = "=0.4.4", git = "https://github.com/LioRael/lenso", rev = "{LENSO_CORE_REVISION}" }}
lenso-kernel = {{ version = "=0.3.10", git = "https://github.com/LioRael/lenso", rev = "{LENSO_CORE_REVISION}" }}
lenso-web-host = {{ version = "0.2.1", git = "https://github.com/LioRael/lenso-web", rev = "{LENSO_WEB_REVISION}" }}
plugin = {{ package = "{}", path = {plugin_path} }}
serde_json = "1"
tokio = {{ version = "1.52", features = ["macros", "rt", "signal"] }}
tower = "0.5"

[patch.crates-io]
lenso = {{ git = "https://github.com/LioRael/lenso-runtime-rust", rev = "{LENSO_NATIVE_REVISION}" }}
lenso-app-plan = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_CORE_REVISION}" }}
lenso-kernel = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_CORE_REVISION}" }}
lenso-native-adapter = {{ git = "https://github.com/LioRael/lenso-runtime-rust", rev = "{LENSO_NATIVE_REVISION}" }}

[workspace]
"#,
        package.name,
    )
}

fn run_cargo<const N: usize>(
    root: &Path,
    target_directory: &Path,
    args: [&str; N],
    action: &str,
) -> anyhow::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(args)
        .env("CARGO_TARGET_DIR", target_directory)
        .current_dir(root)
        .status()
        .with_context(|| action.to_owned())?;
    if !status.success() {
        bail!("{action} failed with {status}");
    }
    Ok(())
}

async fn wait_for_signal_shutdown(child: &mut Child) {
    if tokio::time::timeout(Duration::from_secs(4), child.wait())
        .await
        .is_err()
    {
        stop(child).await;
    }
}

async fn stop(child: &mut Child) {
    #[cfg(unix)]
    if let Some(id) = child.id()
        && let Ok(raw) = i32::try_from(id)
    {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(raw),
            nix::sys::signal::Signal::SIGINT,
        );
    }
    #[cfg(not(unix))]
    let _ = child.start_kill();

    if tokio::time::timeout(Duration::from_secs(4), child.wait())
        .await
        .is_err()
    {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LENSO_CORE_REVISION, LENSO_NATIVE_REVISION, LENSO_WEB_REVISION, host_manifest,
        host_package_name,
    };
    use crate::plugin::{CargoMetadata, CargoPackage, LensoMetadata};
    use std::path::Path;

    #[test]
    fn generated_host_uses_the_real_native_web_path() {
        let root = Path::new("/tmp/company.greetings-http");
        let package = CargoPackage {
            name: "company-greetings-http".to_owned(),
            version: "0.1.0".to_owned(),
            metadata: CargoMetadata {
                lenso: LensoMetadata {
                    plugin_id: "company.greetings-http".to_owned(),
                    root_slot: "web".to_owned(),
                },
                lenso_cli: None,
            },
        };
        let name = host_package_name(root, &package.name);
        let manifest = host_manifest(root, &package, &name);

        assert!(name.starts_with("lenso-web-dev-company-greetings-http-"));
        assert!(manifest.contains("lenso-web-host"));
        assert!(manifest.contains("version = \"0.2.1\""));
        assert!(manifest.contains(LENSO_WEB_REVISION));
        assert!(manifest.contains("lenso-app-plan = { version = \"=0.4.4\""));
        assert!(manifest.contains("lenso-kernel = { version = \"=0.3.10\""));
        assert!(manifest.contains(LENSO_CORE_REVISION));
        assert!(manifest.contains(LENSO_NATIVE_REVISION));
        assert!(manifest.contains("[patch.crates-io]"));
        assert!(manifest.contains("lenso-native-adapter"));
        assert!(manifest.contains("tower = \"0.5\""));
        assert!(!manifest.contains("lenso-web-ingress-plugin"));
        assert!(manifest.contains("plugin = { package = \"company-greetings-http\""));
    }
}
