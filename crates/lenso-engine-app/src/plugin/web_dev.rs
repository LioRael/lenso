use std::{
    env,
    fmt::Write as _,
    fs,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;
use tokio::process::{Child, Command as TokioCommand};

use crate::watch::SourceWatcher;

use super::scaffold::WEB_HTTP_ENDPOINT_VERSION;
use super::{
    CargoPackage, DevImplementationArg, PluginDevArgs, cargo_target_directory, project_root,
    read_package,
};

mod build_process;
mod diagnostics;

const HOST_SOURCE: &str = r#"
use lenso_kernel::RuntimeFailure;
use lenso_native_adapter::NativePluginRegistry;
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
    if std::env::args().skip(1).collect::<Vec<_>>() == ["--describe"] {
        plugin::link();
        let catalog = NativePluginRegistry::host_catalog([], [])
            .map_err(|error| format!("describe linked Plugin: {error:?}"))?;
        println!("{}", serde_json::to_string(&catalog).map_err(|error| error.to_string())?);
        return Ok(());
    }
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
    let package = read_package(&manifest)?;
    Ok(package.metadata.lenso.root_slot == "web" && package.metadata.lenso_cli.is_none())
}

pub(super) async fn run(args: PluginDevArgs) -> anyhow::Result<()> {
    validate_args(&args)?;
    let root = project_root(args.repo_root.clone())?;
    let mut watcher = args.watch.then(|| SourceWatcher::new(&root)).transpose()?;
    let mut shutdown = Box::pin(tokio::signal::ctrl_c());
    // Register before staging or starting Cargo; keep the same listener across
    // build, failure and serving states so signals between states are retained.
    if let std::task::Poll::Ready(result) = futures::poll!(&mut shutdown) {
        result.context("listen for Ctrl-C")?;
        return Ok(());
    }

    loop {
        // A preset stages immutable source. Reprepare after a watch event so
        // the next Host points to the new generation and rechecks its cohort.
        let attempt = async {
            let package = read_package(&root.join("Cargo.toml"))?;
            let Some(target_directory) =
                build_process::target_directory(&root, &mut shutdown).await?
            else {
                return Ok(None);
            };
            let host = DevHost::stage(&root, &package, target_directory)?;
            let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
            let mut lock = crate::app::build_command(cargo);
            lock.arg("generate-lockfile")
                .env("CARGO_TARGET_DIR", &host.target_directory)
                .current_dir(host.project.path());
            if !build_process::run(lock, "lock Web development Host", &mut shutdown, |line| {
                eprintln!("{line}");
            })
            .await?
                || !diagnostics::build_for_dev(
                    host.project.path(),
                    &host.target_directory,
                    &host.project_root,
                    &host.plugin_manifest,
                    &mut shutdown,
                )
                .await?
            {
                return Ok(None);
            }
            let child = host.spawn(args.json)?;
            Ok::<_, anyhow::Error>(Some((host, child)))
        }
        .await;
        let (_host, mut child) = match attempt {
            Ok(Some(running)) => running,
            Ok(None) => return Ok(()),
            Err(error) => {
                let Some(watcher) = watcher.as_mut() else {
                    return Err(error);
                };
                if wait_after_failure(watcher, &error, &mut shutdown).await? {
                    continue;
                }
                return Ok(());
            }
        };
        if let Some(watcher) = watcher.as_mut() {
            tokio::select! {
                result = child.wait() => {
                    let status = result.context("wait for Web development Host")?;
                    if !status.success() {
                        let error = anyhow::anyhow!("Web development Host exited with {status}");
                        if wait_after_failure(watcher, &error, &mut shutdown).await? {
                            continue;
                        }
                    }
                    return Ok(());
                }
                result = watcher.changed() => {
                    stop(&mut child).await;
                    result?;
                    if !args.json {
                        println!("Rebuilding Web Plugin after source changes.");
                    }
                }
                result = &mut shutdown => {
                    result.context("listen for Ctrl-C")?;
                    stop(&mut child).await;
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
                result = &mut shutdown => {
                    result.context("listen for Ctrl-C")?;
                    stop(&mut child).await;
                    return Ok(());
                }
            }
        }
    }
}

async fn wait_after_failure(
    watcher: &mut SourceWatcher,
    error: &anyhow::Error,
    shutdown: &mut (impl Future<Output = std::io::Result<()>> + Unpin),
) -> anyhow::Result<bool> {
    eprintln!(
        "Web Plugin development failed: {error:#}\nWaiting for source changes; press Ctrl-C to stop."
    );
    tokio::select! {
        result = watcher.changed() => {
            result?;
            Ok(true)
        }
        result = shutdown => {
            result.context("listen for Ctrl-C")?;
            Ok(false)
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
    plugin_manifest: PathBuf,
    target_directory: PathBuf,
}

impl DevHost {
    pub(super) fn prepare(root: &Path, package: &CargoPackage) -> anyhow::Result<Self> {
        let host = Self::stage(root, package, cargo_target_directory(root)?)?;
        run_cargo(
            host.project.path(),
            &host.target_directory,
            ["generate-lockfile"],
            "lock Web development Host",
        )?;
        Ok(host)
    }

    fn stage(
        root: &Path,
        package: &CargoPackage,
        target_directory: PathBuf,
    ) -> anyhow::Result<Self> {
        let framework = framework_source(root)?;
        let project = tempfile::tempdir().context("create Web development Host directory")?;
        let package_name = host_package_name(root, &package.name);
        let prepared_source =
            crate::app::prepare_web_source(root, &project.path().join("plugin-source"))?;
        let manifest = host_manifest(&prepared_source, package, &package_name, &framework);
        fs::write(project.path().join("Cargo.toml"), manifest)
            .context("write Web development Host manifest")?;
        fs::create_dir(project.path().join("src"))
            .context("create Web development Host source directory")?;
        fs::write(
            project.path().join("src/main.rs"),
            host_source(root, &prepared_source)?,
        )
        .context("write Web development Host source")?;
        let executable = target_directory.join("debug").join(if cfg!(windows) {
            format!("{package_name}.exe")
        } else {
            package_name
        });
        Ok(Self {
            project,
            executable,
            project_root: root.to_path_buf(),
            plugin_manifest: fs::canonicalize(prepared_source.join("Cargo.toml"))
                .context("resolve built Web Plugin manifest")?,
            target_directory,
        })
    }

    pub(super) fn build(&self) -> anyhow::Result<()> {
        diagnostics::build(
            self.project.path(),
            &self.target_directory,
            &self.project_root,
            &self.plugin_manifest,
        )
    }

    pub(super) fn describe(&self) -> anyhow::Result<lenso_app_plan::authoring::HostCatalog> {
        let output = crate::app::build_command(&self.executable)
            .arg("--describe")
            .current_dir(&self.project_root)
            .output()
            .context("describe exact linked Web Plugin Host")?;
        if !output.status.success() {
            bail!(
                "describe exact linked Web Plugin Host failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        serde_json::from_slice(&output.stdout).context("parse exact linked Web Plugin Catalog")
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

fn host_source(root: &Path, prepared: &Path) -> anyhow::Result<String> {
    if prepared == root {
        return Ok(HOST_SOURCE.into());
    }
    let report = lenso_app_authoring::discovery::discover(root)?;
    let canonical_root = fs::canonicalize(root)?;
    let candidates = report
        .candidates
        .iter()
        .filter(|candidate| candidate.project == canonical_root && candidate.format == "cargo")
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        bail!("Web development preset requires one source-declared Plugin linkage anchor");
    }
    let link = candidates[0]
        .native_link
        .as_deref()
        .unwrap_or("link_plugin");
    Ok(HOST_SOURCE.replace("plugin::link();", &format!("plugin::{link}();")))
}

#[derive(Debug, Eq, PartialEq)]
enum FrameworkSource {
    Registry,
    Git { url: String, rev: String },
}

fn framework_source(root: &Path) -> anyhow::Result<FrameworkSource> {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .context("read Web Plugin manifest for framework source")?;
    let manifest: toml::Value =
        toml::from_str(&manifest).context("parse Web Plugin manifest for framework source")?;
    source_from_manifest(&manifest)
}

fn source_from_manifest(manifest: &toml::Value) -> anyhow::Result<FrameworkSource> {
    let dependencies = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .context("Web Plugin has no direct dependencies")?;
    let mut source: Option<FrameworkSource> = None;
    for (alias, dependency) in dependencies {
        let name = dependency
            .get("package")
            .and_then(toml::Value::as_str)
            .unwrap_or(alias);
        let Some(expected) = registry_framework_version(name) else {
            continue;
        };
        let version = dependency
            .as_str()
            .or_else(|| dependency.get("version").and_then(toml::Value::as_str));
        let candidate = if let Some(url) = dependency.get("git").and_then(toml::Value::as_str) {
            let rev = dependency
                .get("rev")
                .and_then(toml::Value::as_str)
                .with_context(|| format!("{name} must pin a full Git commit with `rev`"))?;
            if rev.len() != 40 || !rev.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("{name} must pin a full 40-character Git commit with `rev`");
            }
            if dependency.get("path").is_some()
                || dependency.get("branch").is_some()
                || dependency.get("tag").is_some()
                || dependency.get("registry").is_some()
                || dependency.get("workspace").is_some()
            {
                bail!("{name} has an unsupported mixed Cargo source; use one exact Git revision");
            }
            if matches!(name, "lenso-app-plan" | "lenso-kernel" | "lenso-web-host")
                && version.is_some_and(|version| version.trim_start_matches('=') != expected)
            {
                bail!("Web development Host requires {name}@{expected} from this Git source");
            }
            FrameworkSource::Git {
                url: url.to_owned(),
                rev: rev.to_owned(),
            }
        } else {
            if dependency.get("path").is_some()
                || dependency.get("workspace").is_some()
                || dependency.get("registry").is_some()
            {
                bail!("{name} uses an unsupported local or alternate-registry Lenso source");
            }
            let actual = version.with_context(|| format!("{name} needs an exact version"))?;
            if actual != format!("={expected}") {
                bail!(
                    "Web development Host requires {name}@{expected} from crates.io, pinned as `={expected}`"
                );
            }
            FrameworkSource::Registry
        };
        if let Some(previous) = &source
            && previous != &candidate
        {
            bail!(
                "Web Plugin mixes crates.io and Git Lenso sources or multiple Lenso Git revisions"
            );
        }
        source = Some(candidate);
    }
    let source = source.context("Web Plugin has no direct Lenso framework dependency")?;
    if let Some(patches) = manifest
        .get("patch")
        .and_then(|patch| patch.get("crates-io"))
        && let Some(patches) = patches.as_table()
    {
        for (alias, patch) in patches {
            let name = patch
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(alias);
            if registry_framework_version(name).is_none() {
                continue;
            }
            match &source {
                FrameworkSource::Git { url, rev }
                    if patch.get("git").and_then(toml::Value::as_str) == Some(url)
                        && patch.get("rev").and_then(toml::Value::as_str) == Some(rev) => {}
                _ => bail!("Web Plugin patch for {name} does not match its Lenso framework source"),
            }
        }
    }
    Ok(source)
}

fn registry_framework_version(name: &str) -> Option<&'static str> {
    Some(match name {
        "lenso" => "0.5.29",
        "lenso-app-plan" => "0.4.6",
        "lenso-kernel" => "0.3.12",
        "lenso-native-adapter" => "0.3.20",
        "lenso-runner" => "0.2.20",
        "lenso-contract-runtime" => "0.2.0",
        "lenso-capability-http-endpoint" => WEB_HTTP_ENDPOINT_VERSION,
        "lenso-capability-http-stream-endpoint" => "0.1.4",
        "lenso-capability-websocket-endpoint" => "0.1.4",
        "lenso-web-host" => "0.2.6",
        "lenso-web-ingress-plugin" => "0.4.11",
        _ => return None,
    })
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

fn host_manifest(
    root: &Path,
    package: &CargoPackage,
    host_package_name: &str,
    framework: &FrameworkSource,
) -> String {
    let plugin_path =
        serde_json::to_string(&root.to_string_lossy()).expect("serialize Plugin path");
    let (app_plan, kernel, native_adapter, web_host, patches) = match framework {
        FrameworkSource::Registry => (
            "\"=0.4.6\"".to_owned(),
            "\"=0.3.12\"".to_owned(),
            "\"=0.3.20\"".to_owned(),
            "\"=0.2.6\"".to_owned(),
            String::new(),
        ),
        FrameworkSource::Git { url, rev } => {
            let url = toml::Value::String(url.clone()).to_string();
            let rev = toml::Value::String(rev.clone()).to_string();
            let dependency =
                |version: &str| format!("{{ version = \"={version}\", git = {url}, rev = {rev} }}");
            let mut patches = String::from("[patch.crates-io]\n");
            for name in [
                "lenso",
                "lenso-app-plan",
                "lenso-kernel",
                "lenso-native-adapter",
                "lenso-capability-http-endpoint",
                "lenso-capability-http-stream-endpoint",
                "lenso-capability-websocket-endpoint",
                "lenso-web-ingress-plugin",
                "lenso-runner",
                "lenso-contract-runtime",
            ] {
                patches.push_str(&format!("{name} = {{ git = {url}, rev = {rev} }}\n"));
            }
            (
                dependency("0.4.6"),
                dependency("0.3.12"),
                dependency("0.3.20"),
                dependency("0.2.6"),
                patches,
            )
        }
    };
    format!(
        r#"[package]
name = "{host_package_name}"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
futures = "0.3"
lenso-app-plan = {app_plan}
lenso-kernel = {kernel}
lenso-native-adapter = {native_adapter}
lenso-web-host = {web_host}
plugin = {{ package = "{}", path = {plugin_path} }}
serde_json = "1"
tokio = {{ version = "1.52", features = ["macros", "rt", "signal"] }}
tower = "0.5"

{patches}
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
    let status = crate::app::build_command(cargo)
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
        FrameworkSource, framework_source, host_manifest, host_package_name, source_from_manifest,
    };
    use crate::plugin::{CargoMetadata, CargoPackage, LensoMetadata, web_plugin_scaffold};
    use std::{fs, path::Path};

    fn package() -> CargoPackage {
        CargoPackage {
            name: "company-greetings-http".to_owned(),
            version: "0.1.0".to_owned(),
            metadata: CargoMetadata {
                lenso: LensoMetadata {
                    plugin_id: "company.greetings-http".to_owned(),
                    root_slot: "web".to_owned(),
                },
                lenso_cli: None,
            },
        }
    }

    fn source(manifest: &str) -> anyhow::Result<FrameworkSource> {
        source_from_manifest(&toml::from_str(manifest).unwrap())
    }

    #[test]
    fn preset_without_empty_link_stub_builds_and_describes_the_real_dev_host() {
        let source = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir().unwrap();
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for (relative, text) in web_plugin_scaffold("company.greetings-http") {
            let path = source.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        crate::app::prepare_web_starter(source.path(), true).unwrap();
        assert!(
            !fs::read_to_string(source.path().join("src/lib.rs"))
                .unwrap()
                .contains("fn link()")
        );
        let manifest_path = source.path().join("Cargo.toml");
        let mut plugin_manifest: toml::Value =
            toml::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        // Test the real scaffold and lowering against the local owning cohort;
        // publication/registry availability is a separate release fact.
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(dependencies) = plugin_manifest
                .get_mut(section)
                .and_then(toml::Value::as_table_mut)
            {
                for (name, dependency) in dependencies {
                    if crates.join(name).join("Cargo.toml").is_file() {
                        *dependency =
                            toml::Value::try_from(serde_json::json!({"path":crates.join(name)}))
                                .unwrap();
                    }
                }
            }
        }
        fs::write(manifest_path, toml::to_string(&plugin_manifest).unwrap()).unwrap();
        let prepared =
            crate::app::prepare_web_source(source.path(), &host.path().join("plugin-source"))
                .unwrap();
        let code = super::host_source(source.path(), &prepared).unwrap();
        assert!(code.contains("plugin::link_plugin();"));
        let mut manifest: toml::Value = toml::from_str(&host_manifest(
            &prepared,
            &package(),
            "preset-dev-regression",
            &FrameworkSource::Registry,
        ))
        .unwrap();
        for name in [
            "lenso-app-plan",
            "lenso-kernel",
            "lenso-native-adapter",
            "lenso-web-host",
        ] {
            manifest["dependencies"].as_table_mut().unwrap().insert(
                name.into(),
                toml::Value::try_from(serde_json::json!({"path":crates.join(name)})).unwrap(),
            );
        }
        fs::write(
            host.path().join("Cargo.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        fs::create_dir(host.path().join("src")).unwrap();
        fs::write(host.path().join("src/main.rs"), code).unwrap();
        let output = crate::app::cargo_command()
            .args(["run", "--offline", "--manifest-path"])
            .arg(host.path().join("Cargo.toml"))
            .args(["--", "--describe"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let catalog: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(catalog.to_string().contains("company.greetings-http"));
    }

    #[test]
    fn generated_host_uses_the_real_native_web_path() {
        let root = Path::new("/tmp/company.greetings-http");
        let package = package();
        let name = host_package_name(root, &package.name);
        let manifest = host_manifest(root, &package, &name, &FrameworkSource::Registry);
        let parsed: toml::Value = toml::from_str(&manifest).unwrap();

        assert!(name.starts_with("lenso-web-dev-company-greetings-http-"));
        for (name, version) in [
            ("lenso-web-host", "=0.2.6"),
            ("lenso-app-plan", "=0.4.6"),
            ("lenso-kernel", "=0.3.12"),
            ("lenso-native-adapter", "=0.3.20"),
        ] {
            assert_eq!(parsed["dependencies"][name].as_str(), Some(version));
        }
        assert!(parsed.get("patch").is_none());
        assert!(!manifest.contains("git ="));
        assert!(manifest.contains("tower = \"0.5\""));
        assert!(!manifest.contains("lenso-web-ingress-plugin"));
        assert!(manifest.contains("plugin = { package = \"company-greetings-http\""));
    }

    #[test]
    fn generated_registry_plugin_needs_no_lock_or_network_to_select_host() {
        let project = tempfile::tempdir().unwrap();
        let plugin = project.path().join("plugin");
        fs::create_dir(&plugin).unwrap();
        fs::write(
            project.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"plugin\"]\n",
        )
        .unwrap();
        let manifest = web_plugin_scaffold("company.greetings-http")
            .remove(Path::new("Cargo.toml"))
            .unwrap()
            .replace(
                "[build-dependencies]",
                "lenso-business-plugin = { path = \"../business\" }\n\n[build-dependencies]",
            );
        fs::write(plugin.join("Cargo.toml"), manifest).unwrap();

        assert_eq!(
            framework_source(&plugin).unwrap(),
            FrameworkSource::Registry
        );
        assert!(!project.path().join("Cargo.lock").exists());
        assert!(!plugin.join("Cargo.lock").exists());
    }

    #[test]
    fn git_web_plugin_uses_its_exact_source_for_dev_host() {
        let rev = "8e6eb5eb9f468959eea713eab5f20592dfe65a71";
        let git = "https://github.com/LioRael/lenso";
        let manifest = format!(
            r#"[dependencies]
lenso = {{ version = "=0.5.25", git = "{git}", rev = "{rev}" }}
lenso-capability-http-endpoint = {{ version = "0.3.8", git = "{git}", rev = "{rev}" }}

[patch.crates-io]
lenso = {{ git = "{git}", rev = "{rev}" }}
lenso-kernel = {{ git = "{git}", rev = "{rev}" }}
"#
        );
        let framework = source(&manifest).unwrap();
        assert_eq!(
            framework,
            FrameworkSource::Git {
                url: git.to_owned(),
                rev: rev.to_owned(),
            }
        );
        let generated = host_manifest(
            Path::new("/tmp/company.greetings-http"),
            &package(),
            "web-dev",
            &framework,
        );
        let parsed: toml::Value = toml::from_str(&generated).unwrap();
        for name in ["lenso-app-plan", "lenso-kernel", "lenso-web-host"] {
            assert_eq!(parsed["dependencies"][name]["git"].as_str(), Some(git));
            assert_eq!(parsed["dependencies"][name]["rev"].as_str(), Some(rev));
        }
        for name in [
            "lenso",
            "lenso-app-plan",
            "lenso-kernel",
            "lenso-native-adapter",
            "lenso-capability-http-endpoint",
            "lenso-capability-http-stream-endpoint",
            "lenso-capability-websocket-endpoint",
            "lenso-web-ingress-plugin",
            "lenso-runner",
            "lenso-contract-runtime",
        ] {
            assert_eq!(
                parsed["patch"]["crates-io"][name]["git"].as_str(),
                Some(git)
            );
            assert_eq!(
                parsed["patch"]["crates-io"][name]["rev"].as_str(),
                Some(rev)
            );
        }
    }

    #[test]
    fn non_cohort_endpoint_api_requires_explicit_git_source_without_registry_fallback() {
        let registry =
            "[dependencies]\nlenso = \"=0.5.29\"\nlenso-capability-http-endpoint = \"=0.3.5\"\n";
        assert!(
            source(registry)
                .unwrap_err()
                .to_string()
                .contains("lenso-capability-http-endpoint@0.3.8")
        );
        let rev = "2039cee33d8570e8cf202714a9b87c10fb55548b";
        let manifest = format!(
            r#"[dependencies]
lenso = {{ git = "https://github.com/LioRael/lenso", rev = "{rev}" }}
lenso-capability-http-endpoint = {{ version = "=0.3.5", git = "https://github.com/LioRael/lenso", rev = "{rev}" }}
"#
        );
        assert_eq!(
            source(&manifest).unwrap(),
            FrameworkSource::Git {
                url: "https://github.com/LioRael/lenso".into(),
                rev: rev.into()
            }
        );
    }

    #[test]
    fn incompatible_registry_and_mixed_sources_fail_closed() {
        let rev = "8e6eb5eb9f468959eea713eab5f20592dfe65a71";
        let git =
            format!("lenso = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{rev}\" }}");
        let old_registry = "[dependencies]\nlenso = \"=0.5.25\"\n";
        assert!(
            source(old_registry)
                .unwrap_err()
                .to_string()
                .contains("lenso@0.5.29")
        );

        let mixed = format!("[dependencies]\n{git}\nlenso-capability-http-endpoint = \"=0.3.8\"\n");
        assert!(
            source(&mixed)
                .unwrap_err()
                .to_string()
                .contains("mixes crates.io and Git")
        );

        let local = "[dependencies]\nlenso = { path = \"../lenso\" }\n";
        assert!(
            source(local)
                .unwrap_err()
                .to_string()
                .contains("unsupported local")
        );

        let workspace = "[dependencies]\nlenso = { workspace = true }\n";
        assert!(
            source(workspace)
                .unwrap_err()
                .to_string()
                .contains("unsupported local")
        );

        let other_rev = "c9cd15629b7d65d6f6cdc12113234acd85c89a89";
        let different = format!(
            "[dependencies]\n{git}\nlenso-capability-http-endpoint = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{other_rev}\" }}\n"
        );
        assert!(
            source(&different)
                .unwrap_err()
                .to_string()
                .contains("multiple Lenso Git revisions")
        );

        let bad_patch = format!(
            "[dependencies]\n{git}\n[patch.crates-io]\nlenso-kernel = {{ path = \"../core\" }}\n"
        );
        assert!(
            source(&bad_patch)
                .unwrap_err()
                .to_string()
                .contains("patch for lenso-kernel")
        );
    }

    #[test]
    fn web_host_identity_dependencies_cannot_mix_git_and_registry_sources() {
        let rev = "8e6eb5eb9f468959eea713eab5f20592dfe65a71";
        let git = "https://github.com/LioRael/lenso";
        for (name, version) in [
            ("lenso-capability-http-stream-endpoint", "0.1.4"),
            ("lenso-capability-websocket-endpoint", "0.1.4"),
            ("lenso-web-ingress-plugin", "0.4.11"),
            ("lenso-runner", "0.2.20"),
            ("lenso-contract-runtime", "0.2.0"),
        ] {
            let dependency = format!(
                "{name} = {{ version = \"={version}\", git = \"{git}\", rev = \"{rev}\" }}"
            );
            let registry_root = format!("[dependencies]\nlenso = \"=0.5.29\"\n{dependency}\n");
            let git_root = format!(
                "[dependencies]\nlenso = {{ version = \"=0.5.25\", git = \"{git}\", rev = \"{rev}\" }}\n{name} = \"={version}\"\n"
            );
            for manifest in [registry_root, git_root] {
                let error = source(&manifest).unwrap_err();
                assert!(
                    error.to_string().contains("mixes crates.io and Git"),
                    "{name}: {error:#}"
                );
            }
        }
    }
}
