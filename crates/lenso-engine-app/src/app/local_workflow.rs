use anyhow::{Context, bail};
use clap::{Args, ValueEnum};
use std::{fs, path::PathBuf, process::Command};

mod notes;
mod workers;

#[derive(Clone, Debug, Args)]
pub struct BuildArgs {
    /// Existing static TypeScript Host source. Omit for the local App convention.
    #[arg(long, requires_all = ["target", "out"], conflicts_with = "root")]
    source: Option<PathBuf>,
    /// `workers` for a local workerd App, or an exact explicit TypeScript Host target.
    #[arg(long)]
    target: Option<String>,
    /// Local App source root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// New output directory. Defaults to dist (Native) or dist-workers (Workers).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Exact @lenso/workers-runtime package directory for a Workers build.
    #[arg(long, requires = "target")]
    workers_runtime: Option<PathBuf>,
    /// Jco 1.35.0 executable for a Workers Component build.
    #[arg(long, requires = "target")]
    jco: Option<PathBuf>,
    /// wasm-bindgen 0.2.127 executable for the static linked-Rust Workers profile.
    #[arg(long, requires = "target", conflicts_with_all = ["jco", "workers_integration"])]
    wasm_bindgen: Option<PathBuf>,
    /// Exact per-Instance grants to source-owned Workers facility factories.
    #[arg(long, requires = "wasm_bindgen")]
    workers_facilities: Option<PathBuf>,
    /// Explicit bounded event, HTTP and cleanup limits for the static Workers Host.
    #[arg(long, requires = "wasm_bindgen")]
    workers_host_limits: Option<PathBuf>,
    /// Plugin-owned Workers Host integration profile; requires explicit code trust.
    #[arg(long, requires_all = ["target", "trust_workers_integration"], conflicts_with = "source")]
    workers_integration: Option<PathBuf>,
    /// Authorize the exact profile and its digest-bound trusted Host JavaScript.
    #[arg(long, value_name = "sha256:DIGEST", requires = "workers_integration")]
    trust_workers_integration: Option<String>,
    /// Trust exact adopted linked Cargo or npm build-time code for this unsandboxed build.
    #[arg(
        long,
        visible_alias = "trust-adopted-build",
        value_name = "PLUGIN_ID@VERSION=sha256:DIGEST"
    )]
    trust_linked_build: Vec<String>,
    /// Host policy for one exact portable Plugin: PLUGIN_ID=process|wasm.
    #[arg(
        long = "portable-implementation",
        value_name = "PLUGIN_ID=process|wasm"
    )]
    portable_implementations: Vec<String>,
}
pub fn build(args: BuildArgs) -> anyhow::Result<()> {
    if args.target.as_deref() == Some("workers") && args.source.is_none() {
        if !args.portable_implementations.is_empty()
            || (!args.trust_linked_build.is_empty() && args.wasm_bindgen.is_none())
        {
            bail!(
                "Workers linked-build trust requires --wasm-bindgen; portable implementation overrides are only for native builds"
            );
        }
        let root = crate::plugins::project_root(args.root)?;
        return workers::build(workers::BuildArgs {
            out: args.out.unwrap_or_else(|| root.join("dist-workers")),
            root,
            workers_runtime: args
                .workers_runtime
                .context("Workers build needs --workers-runtime pointing to an exact @lenso/workers-runtime package")?,
            jco: args.jco,
            wasm_bindgen: args.wasm_bindgen,
            facilities: args.workers_facilities,
            host_limits: args.workers_host_limits,
            trust_linked_build: args.trust_linked_build,
            integration: args.workers_integration,
            trust_integration: args.trust_workers_integration,
        });
    }
    if args.workers_runtime.is_some()
        || args.jco.is_some()
        || args.wasm_bindgen.is_some()
        || args.workers_facilities.is_some()
        || args.workers_host_limits.is_some()
        || args.workers_integration.is_some()
        || args.trust_workers_integration.is_some()
    {
        bail!(
            "Workers runtime, Jco and integration inputs are only for `app build --target workers`"
        );
    }
    if let Some(source) = args.source {
        if !args.trust_linked_build.is_empty() || !args.portable_implementations.is_empty() {
            bail!(
                "--trust-linked-build and --portable-implementation are only for a native source App build"
            );
        }
        return super::build::build(&super::build::HostBuildArgs {
            source,
            target: args.target.context("TS Host target")?,
            out: args.out.context("TS Host output")?,
        });
    }
    if let Some(target) = args.target {
        bail!(
            "unsupported local App build target `{target}`; supported targets are native (omit --target) and workers"
        );
    }
    let root = crate::plugins::project_root(args.root)?;
    let out = args.out.unwrap_or_else(|| root.join("dist"));
    let mut engine = lenso_engine::Engine::default();
    engine.register(lenso_engine_runtime::RuntimeProcessor::new(
        super::preset::AppProject {
            root,
            output: out,
            runtime_executable: std::env::current_exe()?,
            trust_linked_build: args.trust_linked_build,
            portable_implementations: args.portable_implementations,
        },
    ))?;
    let plan = engine.plan(lenso_engine::Snapshot::default())?;
    engine.execute(
        &plan,
        &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )?;
    Ok(())
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Starter {
    Process,
    Bun,
    Wasm,
    Multi,
    Empty,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum AppLanguage {
    Rust,
}
#[derive(Clone, Debug, Args)]
pub struct CreateArgs {
    /// New App directory. Existing directories are never overwritten.
    directory: PathBuf,
    /// Application language. Rust creates a normal root Cargo package.
    #[arg(long, value_enum, conflicts_with_all = ["runtime", "web", "cli"])]
    lang: Option<AppLanguage>,
    /// Select a matching HTTP Endpoint SDK source crate for the typed Process starter.
    #[arg(long, value_name = "DIR", conflicts_with_all = ["runtime", "web", "cli"])]
    http_sdk_source: Option<PathBuf>,
    /// Legacy nested starter implementation under app/.
    #[arg(long, value_enum)]
    runtime: Option<Starter>,
    /// Create a native Rust Web Plugin with Plugin-owned HTML assets.
    #[arg(long, conflicts_with = "runtime")]
    web: bool,
    /// Create a TypeScript CLI App with bundled convention support.
    #[arg(long, conflicts_with_all = ["runtime", "web"])]
    cli: bool,
    /// Skip the starter's package installation and initial compile check.
    #[arg(long)]
    no_install: bool,
}
pub fn create(args: CreateArgs) -> anyhow::Result<()> {
    if args.lang.is_some() && (args.runtime.is_some() || args.web || args.cli) {
        bail!("--lang selects the root package and cannot be combined with a nested starter");
    }
    let root_package = !args.cli && !args.web && args.runtime.is_none();
    if args.http_sdk_source.is_some() && !root_package {
        bail!("--http-sdk-source selects the root Rust Process starter");
    }
    let destination = std::path::absolute(args.directory)?;
    if fs::symlink_metadata(&destination).is_ok() {
        bail!("App directory already exists: {}", destination.display());
    }
    let parent = destination
        .parent()
        .context("App directory needs a parent")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".lenso-create-")
        .tempdir_in(parent)?;
    fs::create_dir(staging.path().join("app"))?;
    fs::create_dir(staging.path().join("plugins"))?;
    if !args.cli {
        if args.web {
            crate::plugin::create_web_scaffold(
                "local.starter".to_owned(),
                staging.path().to_path_buf(),
                PathBuf::from("app/local.starter"),
                true,
            )?;
            prepare_web_starter(&staging.path().join("app/local.starter"), args.no_install)?;
        } else if let Some(starter) = args.runtime {
            if !matches!(starter, Starter::Empty) {
                let runtime = match starter {
                    Starter::Process => "process",
                    Starter::Bun => "bun",
                    Starter::Wasm => "wasm",
                    Starter::Multi => "multi",
                    Starter::Empty => unreachable!(),
                };
                let mut command = Command::new(std::env::current_exe()?);
                command
                    .args(["plugin", "new", "local.starter", "--repo-root"])
                    .arg(staging.path())
                    .args(["--dir", "app/local.starter"])
                    .args(["--runtime", runtime]);
                if args.no_install {
                    command.arg("--no-install");
                }
                if !command.status()?.success() {
                    bail!("App starter creation failed");
                }
            }
        } else {
            for (path, contents) in notes::scaffold(args.http_sdk_source.as_deref())? {
                let file = staging.path().join(path);
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(file, contents)?;
            }
            if !args.no_install {
                let manifest = staging.path().join("Cargo.toml");
                for arguments in [vec!["generate-lockfile"], vec!["check"]] {
                    if !super::cargo_command()
                        .args(arguments)
                        .arg("--manifest-path")
                        .arg(&manifest)
                        .status()?
                        .success()
                    {
                        bail!("Process notes starter compile check failed");
                    }
                }
            }
        }
    }
    fs::write(
        staging.path().join(".gitignore"),
        ".lenso/\ndist/\ntarget/\nnode_modules/\n",
    )?;
    let business_source = if root_package {
        "The root Cargo package is the default business Plugin. Add more Plugin source projects under `app/` only when they need an independent identity."
    } else {
        "Add Plugin source projects under `app/`."
    };
    let web_routes = if args.web {
        "The starter Web Plugin keeps one handler per `src/routes/*.rs` file. Add or remove a file and rebuild; duplicate route IDs or method/path pairs fail during compilation. The built Host never scans route source.\n\n"
    } else if root_package {
        notes::readme(args.http_sdk_source.is_some())
    } else {
        ""
    };
    let openapi = if args.web {
        "For a public API document, run `lenso app add @lenso/openapi --root .` before building. This selects the optional `lenso.openapi` Plugin and links it from the same pinned Rust cohort as the starter Web Plugin. Fetch `/openapi.json` from the running App and pass that actual document to `lenso-web-client generate openapi.json src/generated/lenso-api.ts`.\n\n"
    } else {
        ""
    };
    fs::write(
        staging.path().join("README.md"),
        format!(
            "# Local Lenso App\n\nRun `lenso dev` to build and watch this App. Run `lenso app build` to produce an offline executable, then `lenso app start --from dist`.\n\n{business_source} Keep instance configuration and explicit dependency choices in `plugins/`. No App configuration file is required. Optional `plugin_sources` in `lenso.toml` adds shared local candidates; an explicit Plugin Root instance is required to select them.\n\n{web_routes}{openapi}The Host selects only execution adapters required by this App's declared candidates. Existing custom Host authoring remains available through `lenso app build --source ... --target ...`.\n"
        ),
    )?;
    super::build::publish_new_output(staging.path(), &destination)?;
    if args.cli {
        for invocation in [
            vec!["app", "add", "@lenso/cli"],
            vec!["app", "plugin", "new", "local.hello"],
        ] {
            let mut command = Command::new(std::env::current_exe()?);
            command.args(invocation).arg("--root").arg(&destination);
            if args.no_install {
                command.arg("--no-install");
            }
            if !command.status()?.success() {
                bail!("CLI App scaffold is created; support setup failed");
            }
        }
    }
    println!(
        "Created App at {}. Run `lenso dev --root {}`.",
        destination.display(),
        destination.display()
    );
    Ok(())
}

#[derive(Clone, Debug, Args)]
pub struct StartArgs {
    /// Built local App directory.
    #[arg(long, default_value = "dist")]
    from: PathBuf,
    /// External App root whose plugins/ intent replaces the build snapshot.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Continuously supervised Host policy; incompatible with --check or terminal arguments.
    #[arg(long, conflicts_with = "root")]
    configuration_policy: Option<PathBuf>,
    /// Host-owned authority for one selected business snapshot object.
    #[arg(long, conflicts_with_all = ["configuration_policy", "args"])]
    business_snapshot_policy: Option<PathBuf>,
    /// Explicit per-Instance binding file for source-owned Host facilities.
    #[arg(long, conflicts_with = "configuration_policy")]
    host_facilities: Option<PathBuf>,
    /// Start, validate readiness and shut down immediately.
    #[arg(long, conflicts_with = "configuration_policy")]
    check: bool,
    /// Local control-plane readiness receipt; not an App-authored input.
    #[arg(long, hide = true, conflicts_with = "check")]
    ready_file: Option<PathBuf>,
    /// Arguments passed to installed terminal support.
    #[arg(last = true, conflicts_with_all = ["check", "configuration_policy"])]
    args: Vec<String>,
}
pub(super) fn start_built_local_app(from: PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    start(StartArgs {
        from,
        root: None,
        configuration_policy: None,
        business_snapshot_policy: None,
        host_facilities: None,
        check: false,
        ready_file: None,
        args,
    })
}

pub(super) fn start_built_local_tool_app(from: PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    start_with_host_command(
        StartArgs {
            from,
            root: None,
            configuration_policy: None,
            business_snapshot_policy: None,
            host_facilities: None,
            check: false,
            ready_file: None,
            args,
        },
        true,
    )
}

pub(super) async fn start_command(args: StartArgs) -> anyhow::Result<()> {
    if let Some(policy) = args.configuration_policy.as_ref() {
        // A one-shot Host would bypass policy revocation and the freshness
        // deadline; replaying a terminal command could repeat side effects.
        if args.check || !args.args.is_empty() {
            bail!(
                "--configuration-policy requires a continuously supervised App; --check and terminal arguments are unsupported"
            );
        }
        return super::local_start::run(args.from, policy.clone(), args.ready_file).await;
    }
    start(args)
}

pub fn start(args: StartArgs) -> anyhow::Result<()> {
    start_with_host_command(args, false)
}

fn start_with_host_command(args: StartArgs, agent_tool_cli: bool) -> anyhow::Result<()> {
    super::configuration_source::require_or_sync(&args.from, args.configuration_policy.as_deref())?;
    let executable = fs::canonicalize(args.from.join(".lenso/host"))
        .context("locate built local Host; run lenso app build first")?;
    let mut command = Command::new(executable);
    command.args(super::local_host::host_arguments(&args.from)?);
    if let Some(root) = args.root {
        command.arg("--root").arg(root);
    }
    if let Some(policy) = args.business_snapshot_policy {
        command.arg("--business-snapshot-policy").arg(policy);
    }
    if let Some(facilities) = args.host_facilities {
        command.arg("--host-facilities").arg(facilities);
    }
    if args.check {
        command.arg("--check");
    }
    if let Some(path) = args.ready_file {
        command.arg("--ready-file").arg(path);
    }
    if agent_tool_cli {
        command.arg("--agent-tool-cli");
    }
    if !args.args.is_empty() {
        command.arg("--").args(&args.args);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec()).context("execute local App")
    }
    #[cfg(not(unix))]
    {
        if !command.status()?.success() {
            bail!("local App exited unsuccessfully");
        }
        Ok(())
    }
}

fn prepare_web_starter(root: &std::path::Path, no_install: bool) -> anyhow::Result<()> {
    let source = root.join("src/lib.rs");
    let code = fs::read_to_string(&source)?
        .replace("pub const fn link() {}", "pub fn link() { link_plugin(); }");
    fs::write(source, code)?;
    fs::create_dir_all(root.join("src/routes"))?;
    fs::write(
        root.join("src/routes/home.rs"),
        r#"#[get("local.home", "/")]
async fn home(&self) -> Result<HandleResponse, Problem> {
    let mut response = lenso_capability_http_endpoint::response::text(
        StatusCode::OK, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/public/index.html")));
    response.headers[0].value = "text/html; charset=utf-8".into();
    Ok(response)
}
"#,
    )?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("tests/home.rs"),
        r#"use std::time::Duration;

use bytes::Bytes;
use http::Request;
use lenso_kernel::ShutdownOutcome;
use lenso_test::TestApp;
use lenso_web_host::NativeWebHost;
use local_starter::GreetingsHttp;

#[test]
fn serves_the_homepage_through_ingress() {
    let prepared = NativeWebHost::new()
        .plugin::<GreetingsHttp>()
        .prepare_simulated()
        .unwrap();
    let (plan, registry, web) = prepared.into_parts();
    let app = TestApp::builder(plan).with_registry(registry).start().unwrap();
    let response = app.run(web.request(
        Request::builder().method("GET").uri("/").body(Bytes::new()).unwrap(),
    )).unwrap();
    assert_eq!(response.status(), 200);
    assert!(response.body().starts_with(b"<!doctype html>"));
    assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
}
"#,
    )?;
    fs::create_dir_all(root.join("public"))?;
    fs::write(
        root.join("public/index.html"),
        r#"<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Local Lenso App</title>
<style>body{font:18px system-ui;max-width:40rem;margin:12vh auto;padding:2rem;background:#f6f7f9;color:#182132}input,button{font:inherit;padding:.65rem;border:1px solid #bac3d0;border-radius:.4rem}button{background:#182132;color:white}output{display:block;margin-top:1.5rem}</style>
<h1>Your Lenso App is running.</h1><p>This page and its HTTP routes belong to the starter Plugin.</p>
<form><label>Your name <input name="name" required value="Lenso"></label> <button>Create greeting</button></form><output aria-live="polite"></output>
<script>document.querySelector('form').onsubmit=async(event)=>{event.preventDefault();const response=await fetch('/greetings',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({name:new FormData(event.target).get('name')})});const body=await response.json();document.querySelector('output').textContent=body.message||body.detail||'Request failed';};</script></html>
"#,
    )?;
    if !no_install
        && !super::cargo_command()
            .args(["check", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .status()?
            .success()
    {
        bail!("Web starter compile check failed");
    }
    Ok(())
}

/// Build an App from a library host without parsing command-line arguments.
pub fn build_local(
    root: PathBuf,
    output: PathBuf,
    runtime_executable: PathBuf,
) -> anyhow::Result<()> {
    let mut engine = lenso_engine::Engine::default();
    engine.register(lenso_engine_runtime::RuntimeProcessor::new(
        super::preset::AppProject {
            root,
            output,
            runtime_executable,
            trust_linked_build: Vec::new(),
            portable_implementations: Vec::new(),
        },
    ))?;
    let plan = engine.plan(lenso_engine::Snapshot::default())?;
    engine.execute(
        &plan,
        &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )?;
    Ok(())
}
/// Create an empty App; language and surface support are adopted independently.
pub fn create_empty(directory: PathBuf) -> anyhow::Result<()> {
    create(CreateArgs {
        directory,
        lang: None,
        http_sdk_source: None,
        runtime: Some(Starter::Empty),
        web: false,
        cli: false,
        no_install: true,
    })
}

/// Run a source-free distribution from an embedding host.
pub fn start_distribution(from: PathBuf, arguments: Vec<String>) -> anyhow::Result<()> {
    start(StartArgs {
        from,
        root: None,
        configuration_policy: None,
        business_snapshot_policy: None,
        host_facilities: None,
        check: false,
        ready_file: None,
        args: arguments,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clap::Parser;

    use super::{
        AppLanguage, BuildArgs, CreateArgs, StartArgs, create, prepare_web_starter, start_command,
    };

    #[derive(Parser)]
    struct ParsedBuild {
        #[command(flatten)]
        args: BuildArgs,
    }

    #[test]
    fn workers_integration_requires_separate_explicit_trust() {
        assert!(
            ParsedBuild::try_parse_from([
                "build",
                "--target",
                "workers",
                "--workers-integration",
                "integration.json"
            ])
            .is_err()
        );
        assert!(
            ParsedBuild::try_parse_from(["build", "--trust-workers-integration", "sha256:pin"])
                .is_err()
        );
        let parsed = ParsedBuild::try_parse_from([
            "build",
            "--target",
            "workers",
            "--workers-integration",
            "integration.json",
            "--trust-workers-integration",
            "sha256:pin",
        ])
        .unwrap();
        assert_eq!(
            parsed.args.workers_integration,
            Some("integration.json".into())
        );
        assert_eq!(
            parsed.args.trust_workers_integration.as_deref(),
            Some("sha256:pin")
        );
    }

    #[derive(Parser)]
    struct ParsedStart {
        #[command(flatten)]
        args: StartArgs,
    }

    #[derive(Parser)]
    struct ParsedCreate {
        #[command(flatten)]
        args: CreateArgs,
    }

    #[test]
    fn sdk_source_selects_only_the_root_process_starter() {
        for extra in [vec!["--web"], vec!["--cli"], vec!["--runtime", "process"]] {
            let mut args = vec!["create", "app", "--http-sdk-source", "sdk"];
            args.extend(extra);
            assert!(ParsedCreate::try_parse_from(args).is_err());
        }
        let registry = ParsedCreate::try_parse_from(["create", "app", "--no-install"]).unwrap();
        assert!(registry.args.http_sdk_source.is_none());
        let typed = ParsedCreate::try_parse_from([
            "create",
            "app",
            "--http-sdk-source",
            "sdk",
            "--no-install",
        ])
        .unwrap();
        assert_eq!(typed.args.http_sdk_source, Some("sdk".into()));
        assert!(typed.args.no_install);
    }

    #[test]
    fn snapshot_policy_supports_check_but_not_configuration_policy() {
        let parsed = ParsedStart::try_parse_from([
            "start",
            "--from",
            "dist",
            "--business-snapshot-policy",
            "/absolute/host-policy.json",
            "--check",
        ])
        .unwrap();
        assert!(parsed.args.check);
        assert_eq!(
            parsed.args.business_snapshot_policy.as_deref(),
            Some(std::path::Path::new("/absolute/host-policy.json"))
        );
        assert!(
            ParsedStart::try_parse_from([
                "start",
                "--business-snapshot-policy",
                "/absolute/host-policy.json",
                "--configuration-policy",
                "/absolute/configuration-policy.json",
            ])
            .is_err()
        );
    }

    #[tokio::test]
    async fn policy_start_rejects_one_shot_modes_before_sync() {
        let base = StartArgs {
            from: "missing-distribution".into(),
            root: None,
            configuration_policy: Some("missing-policy".into()),
            business_snapshot_policy: None,
            host_facilities: None,
            check: false,
            ready_file: None,
            args: Vec::new(),
        };
        let mut check = base.clone();
        check.check = true;
        assert!(
            start_command(check)
                .await
                .unwrap_err()
                .to_string()
                .contains("continuously supervised")
        );
        let mut terminal = base;
        terminal.args = vec!["status".into()];
        assert!(
            start_command(terminal)
                .await
                .unwrap_err()
                .to_string()
                .contains("terminal arguments are unsupported")
        );
    }

    #[test]
    fn web_app_create_uses_the_native_web_scaffold() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("starter");

        create(CreateArgs {
            directory: destination.clone(),
            lang: None,
            http_sdk_source: None,
            runtime: None,
            web: true,
            cli: false,
            no_install: true,
        })
        .unwrap();

        let plugin = destination.join("app/local.starter");
        let manifest = fs::read_to_string(plugin.join("Cargo.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&manifest).unwrap();
        assert_eq!(
            parsed["dev-dependencies"]["lenso-web-host"].as_str(),
            Some("=0.2.6")
        );
        assert_eq!(
            parsed["dev-dependencies"]["lenso-test"].as_str(),
            Some("=0.1.3")
        );
        assert!(parsed.get("patch").is_none());
        assert!(!manifest.contains("git ="));
        assert!(plugin.join("tests/simulated_web.rs").is_file());
        assert!(plugin.join("public/index.html").is_file());
        assert!(plugin.join("src/routes/home.rs").is_file());
        assert!(
            fs::read_to_string(destination.join("README.md"))
                .unwrap()
                .contains("src/routes/*.rs")
        );
        assert!(
            fs::read_to_string(plugin.join("src/routes/home.rs"))
                .unwrap()
                .contains("#[get(\"local.home\", \"/\")]")
        );
    }

    #[test]
    #[ignore = "clean-room test downloads the pinned Web cohort"]
    fn clean_room_web_app_starter_runs_generated_tests() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("starter");
        create(CreateArgs {
            directory: destination.clone(),
            lang: None,
            http_sdk_source: None,
            runtime: None,
            web: true,
            cli: false,
            no_install: true,
        })
        .unwrap();
        let status = std::process::Command::new("cargo")
            .args(["test"])
            .current_dir(destination.join("app/local.starter"))
            .status()
            .unwrap();
        assert!(status.success(), "generated Web App tests failed");
    }

    #[test]
    fn source_sdk_rust_app_uses_typed_handlers_in_the_root_cargo_package() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("notes");
        create(CreateArgs {
            directory: destination.clone(),
            lang: Some(AppLanguage::Rust),
            http_sdk_source: Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../lenso-capability-http-endpoint"),
            ),
            runtime: None,
            web: false,
            cli: false,
            no_install: true,
        })
        .unwrap();

        assert!(destination.join("Cargo.toml").is_file());
        assert!(destination.join("src/lib.rs").is_file());
        assert!(destination.join("src/main.rs").is_file());
        assert!(destination.join("tests/notes.rs").is_file());
        let manifest = fs::read_to_string(destination.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("runtime = \"process\""));
        assert!(manifest.contains("root-slot = \"web\""));
        let parsed: toml::Value = toml::from_str(&manifest).unwrap();
        assert_eq!(
            parsed["dependencies"]["lenso-capability-http-endpoint"]["features"][0].as_str(),
            Some("process")
        );
        assert!(parsed["dependencies"].get("lenso-process-sdk").is_none());
        assert!(parsed.get("patch").is_none());
        let source = fs::read_to_string(destination.join("src/lib.rs")).unwrap();
        assert_eq!(source.matches("\"notes.create\"").count(), 1);
        assert_eq!(source.matches("\"notes.read\"").count(), 1);
        assert!(source.contains("#[endpoint(standalone)]"));
        for protocol in [
            "ProcessPlugin",
            "ProcessOutcome",
            "DESCRIBE_OPERATION",
            "HANDLE_OPERATION",
            "HandleRequest",
            "HandleResponse",
        ] {
            assert!(
                !source.contains(protocol),
                "business source contains {protocol}"
            );
        }
        let readme = fs::read_to_string(destination.join("README.md")).unwrap();
        assert!(readme.contains("in-memory development data"));
        assert!(readme.contains("disable both"));
        assert!(readme.contains("local release candidates"));
        assert!(!destination.join("app/local.starter").exists());
        let report = lenso_app_authoring::discovery::discover(&destination).unwrap();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].plugin_id, "local.starter");
        assert_eq!(report.candidates[0].implementations[0].runtime, "process");
        assert_eq!(
            report.candidates[0].project,
            fs::canonicalize(destination).unwrap()
        );
    }

    #[test]
    fn web_starter_keeps_the_scaffold_web_cohort_pinned() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            r#"[package]
name = "local-starter"
version = "0.1.0"

[dependencies]
lenso = { version = "=0.5.26", git = "https://github.com/LioRael/lenso", rev = "runtime-pin" }
lenso-capability-http-endpoint = { version = "0.3.4", git = "https://github.com/LioRael/lenso", rev = "c9cd15629b7d65d6f6cdc12113234acd85c89a89" }
"#,
        )
        .unwrap();
        fs::write(
            root.path().join("src/lib.rs"),
            "pub const fn link() {}\nimpl GreetingsHttp {\n}\n",
        )
        .unwrap();

        prepare_web_starter(root.path(), true).unwrap();

        let manifest = fs::read_to_string(root.path().join("Cargo.toml")).unwrap();
        assert!(manifest.contains("lenso = { version = \"=0.5.26\""));
        assert!(manifest.contains("https://github.com/LioRael/lenso"));
        assert!(manifest.contains("runtime-pin"));
        assert!(manifest.contains("lenso-capability-http-endpoint"));
        assert!(manifest.contains("https://github.com/LioRael/lenso"));
        assert!(manifest.contains("c9cd15629b7d65d6f6cdc12113234acd85c89a89"));
        assert!(!manifest.contains("0.3.2"));
    }
}
