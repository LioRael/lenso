use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use lenso_app_plan::authoring::HostCatalog;
use serde::Serialize;

use crate::plugins::{load_resolved_app, project_root};

mod assemble;
mod bootstrap_configuration_source;
mod build;
mod configuration_source;
mod contracts;
pub(crate) mod convention_authoring;
mod convention_build;
mod explain;
mod facility_inspection;
pub mod facts;
pub use configuration_source::Status as ConfigurationStatus;
pub use configuration_source::sync_external_configuration;
pub use convention_authoring::linked_catalog::RecommendationRestrictions;
pub use facts::{ProjectFacts, inspect_project_facts};
mod local_dev;
pub use local_dev::DevArgs;
mod local_host;
mod local_host_retirement;

pub(crate) fn prepare_web_source(root: &Path, destination: &Path) -> anyhow::Result<PathBuf> {
    local_host::web_authoring::stage_project(root, destination)
}
mod local_lock;
mod local_start;
mod local_workflow;
pub use local_workflow::CreateArgs;
mod portable_runtime {
    include!("app/local_runtime_template.rs");
    include!("app/local_json_template.rs");
}
mod precompiled;
mod prepare;
mod preset;
#[cfg(test)]
pub(crate) use local_workflow::prepare_web_starter;
pub use preset::{AppProject, PreparedAppProject};
mod signed_catalog;
mod target_closure;
mod tool_cli;
pub use signed_catalog::{PortableCatalogPage, PortableCatalogQuery};
pub(crate) use signed_catalog::{
    read_snapshot as read_signed_portable_snapshot, read_trust as read_signed_portable_trust,
};
#[allow(dead_code)]
mod terminal;

/// Keep build-phase subprocesses from inheriting business/runtime credentials.
/// This is an environment boundary, not a filesystem or execution sandbox.
pub(crate) fn cargo_command() -> Command {
    build_command("cargo")
}

/// Create a build-phase command with only the toolchain environment allowlist.
/// This removes ambient business credentials but is not a filesystem sandbox.
pub fn build_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TERM",
        "DEVELOPER_DIR",
        "SDKROOT",
        "MACOSX_DEPLOYMENT_TARGET",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
        "CARGO_NET_OFFLINE",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

// Keep the command argument constructible for an embedding CLI.  The command
// owns the persisted Host inspection; callers should not recreate a profile
// parser or a second resolver around it.
pub use explain::ExplainArgs;

/// Inspect the same persisted Host admission and binding evidence as
/// `lenso app explain --json`, without invoking a second resolver.
pub fn inspect_app_explanation(root: impl AsRef<Path>) -> anyhow::Result<serde_json::Value> {
    inspect_app_explanation_with_facilities(root, None)
}

/// Inspect persisted App evidence and optional Host grants without constructing
/// owner facilities or querying resources.
pub fn inspect_app_explanation_with_facilities(
    root: impl AsRef<Path>,
    host_facilities: Option<&Path>,
) -> anyhow::Result<serde_json::Value> {
    explain::report_with_facilities(root.as_ref(), host_facilities)
}

/// Validate one built App with the same resolver as `lenso app check`.
pub fn inspect_app_check(root: impl AsRef<Path>) -> anyhow::Result<AppCheckReport> {
    contracts::check(root.as_ref())?;
    let resolved = load_resolved_app(root.as_ref())?;
    target_closure::check_generated_host(root.as_ref(), &resolved)?;
    Ok(AppCheckReport {
        schema_version: 1,
        kind: "lenso.app-check",
        status: "passed",
        plugin_instances: resolved.instances().len(),
        capability_bindings: resolved.plan().capability_bindings().len(),
    })
}

#[derive(Debug, Serialize)]
pub struct AppCheckReport {
    pub schema_version: u32,
    pub kind: &'static str,
    pub status: &'static str,
    pub plugin_instances: usize,
    pub capability_bindings: usize,
}

/// Read exact signed linked-Cargo candidate metadata without adopting or
/// claiming that an archive or generated Host has passed compatibility checks.
pub fn inspect_linked_cargo_catalog(
    snapshot: impl AsRef<Path>,
    trust: impl AsRef<Path>,
    query: &str,
    target: &str,
) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(
        convention_authoring::linked_catalog::inspect(
            snapshot.as_ref(),
            trust.as_ref(),
            query,
            target,
        )?,
    )?)
}

/// Project-aware read-only projection of signed linked-Cargo candidate facts.
/// Unknown permission, infrastructure, and fee metadata never become grants.
pub fn inspect_linked_cargo_recommendations(
    root: impl AsRef<Path>,
    snapshot: impl AsRef<Path>,
    trust: impl AsRef<Path>,
    query: &str,
    target: &str,
    restrictions: RecommendationRestrictions,
) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(
        convention_authoring::linked_catalog::recommend(
            root.as_ref(),
            snapshot.as_ref(),
            trust.as_ref(),
            query,
            target,
            restrictions,
        )?,
    )?)
}

/// Browse one explicitly supplied signed Portable snapshot. This verifies
/// metadata provenance only; artifact bytes and Host admission remain unchecked.
pub fn inspect_signed_portable_catalog(
    request: PortableCatalogQuery<'_>,
) -> anyhow::Result<PortableCatalogPage> {
    signed_catalog::inspect(request)
}

/// Preview one exact signed npm-only release and local archive for a source
/// App. This shares the CLI adoption verifier, but does not install dependencies,
/// select the Plugin, approve build-time code, or claim runtime readiness.
pub fn inspect_signed_npm_adoption(
    root: impl AsRef<Path>,
    snapshot: impl AsRef<Path>,
    trust: impl AsRef<Path>,
    tgz: impl AsRef<Path>,
    plugin_id: &str,
    version: &str,
    distribution_id: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    convention_authoring::npm_catalog::preview(
        root.as_ref(),
        snapshot.as_ref(),
        trust.as_ref(),
        tgz.as_ref(),
        plugin_id,
        version,
        distribution_id,
    )
}

/// Preview an npm distribution only after joining exact signed linked Cargo
/// and release-details snapshots. It never installs or selects the Plugin.
#[expect(
    clippy::too_many_arguments,
    reason = "preserve the existing public preview API used by CLI and external callers"
)]
pub fn inspect_signed_linked_npm_adoption(
    root: impl AsRef<Path>,
    linked_snapshot: impl AsRef<Path>,
    release_details: impl AsRef<Path>,
    trust: impl AsRef<Path>,
    tgz: impl AsRef<Path>,
    plugin_id: &str,
    version: &str,
    distribution_id: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    convention_authoring::npm_catalog::preview_linked(
        convention_authoring::npm_catalog::LinkedNpmPreview {
            root: root.as_ref(),
            linked_snapshot: linked_snapshot.as_ref(),
            release_details: release_details.as_ref(),
            trust: trust.as_ref(),
            tgz: tgz.as_ref(),
            plugin_id,
            release_version: version,
            distribution_id,
        },
    )
}

pub use convention_authoring::linked_catalog::DocumentRequest as LinkedDocumentRequest;

/// Return one UTF-8 chunk only after verifying exact signed documentation bytes.
pub fn inspect_linked_cargo_document(
    request: LinkedDocumentRequest<'_>,
) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(
        convention_authoring::linked_catalog::document(request)?,
    )?)
}

/// Create the same source App through an embedding or root CLI.
pub fn create_source(args: CreateArgs) -> anyhow::Result<()> {
    local_workflow::create(args)
}

/// Run the source App development loop through an embedding or root CLI.
pub async fn dev_source(args: DevArgs) -> anyhow::Result<()> {
    local_dev::dev(args).await
}

/// Start a locally built App through the same validated distribution path as
/// `lenso app start --from`, without exposing the generated Host's private CLI.
pub fn start_built_local_app(from: PathBuf, args: Vec<String>) -> anyhow::Result<()> {
    local_workflow::start_built_local_app(from, args)
}

#[derive(Clone, Debug, Subcommand)]
pub enum AppCommand {
    /// Adopt a local source or bundled convention support.
    Add(convention_authoring::AddArgs),
    /// Withdraw one exact signed linked Cargo source from the next Host build.
    Unadopt(convention_authoring::UnadoptArgs),
    /// Reconcile a Host-authorized versioned configuration source into a built App.
    #[command(name = "config-sync")]
    ConfigSync(configuration_source::SyncArgs),
    /// Private Host receipt after a Generation passes its Ready Gate.
    #[command(name = "config-activated", hide = true)]
    ConfigActivated(configuration_source::ActivatedArgs),
    /// Inspect accepted and last-activated external configuration revisions.
    #[command(name = "config-status")]
    ConfigStatus(configuration_source::StatusArgs),
    /// Create an App-owned CLI command Plugin (requires @lenso/cli support).
    Plugin {
        #[command(subcommand)]
        command: convention_authoring::PluginCommand,
    },
    /// Author local Capability contracts using existing generated SDK projections.
    Contract {
        #[command(subcommand)]
        command: contracts::scaffold::ContractCommand,
    },
    /// Build a runnable local App, or explicit static TypeScript Host authoring artifacts.
    Build(local_workflow::BuildArgs),
    /// Create a convention-based local App without a handwritten Host configuration.
    Create(local_workflow::CreateArgs),
    /// Start an already built local App without package managers or network resolution.
    Start(local_workflow::StartArgs),
    /// Invoke only Host-bound Agent Tool providers in a built local App.
    Tools {
        #[command(subcommand)]
        command: ToolCommand,
    },
    /// Build and restart the local App when sources or Plugin Root intent change.
    Dev(local_dev::DevArgs),
    #[command(name = "__run-local", hide = true)]
    Runtime {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Prepare one immutable, offline Host distribution for an exact target.
    Prepare(prepare::PrepareArgs),
    /// Create an App workspace from one exact Host executable and Host Catalog.
    Init(AppInitArgs),
    /// Validate the App derived from this Host and its `plugins/` directory.
    Check(ProjectArgs),
    /// Explain the derived Plugin Instances, provenance, and bindings.
    Show(ShowArgs),
    /// Explain target admission, selected implementations, and consumer capability demand.
    Explain(explain::ExplainArgs),
    /// Discover local Plugin source projects and Bundles without building or activating them.
    Discover(ProjectArgs),
    /// Search an exact signed source-only linked Cargo snapshot without adopting candidates.
    #[command(name = "linked-catalog")]
    LinkedCatalog(convention_authoring::LinkedCatalogArgs),
    /// Read one exact signed Markdown revision after verifying its bytes.
    #[command(name = "linked-doc")]
    LinkedDocument(convention_authoring::LinkedDocumentArgs),
    /// Explain local convention support and selected surface packages without executing code.
    Inspect(ProjectArgs),
    /// Report resolved project facts for agents and other development tools.
    Facts(facts::FactsArgs),
    /// Build local Plugin sources into a validated Host authoring directory.
    Assemble(assemble::AssembleArgs),
}

#[derive(Clone, Debug, Subcommand)]
pub enum ToolCommand {
    /// List tools from one exact Provider Instance bound to the Host CLI.
    Catalog(ToolCatalogArgs),
    /// Execute one tool through that same exact Host-approved binding.
    Execute(ToolExecuteArgs),
}

#[derive(Clone, Debug, Args)]
pub struct ToolCatalogArgs {
    /// Built local App directory.
    #[arg(long, default_value = "dist")]
    from: PathBuf,
    /// Exact App-local Provider Instance (plugin-id/instance-key).
    #[arg(long)]
    provider: String,
}

#[derive(Clone, Debug, Args)]
pub struct ToolExecuteArgs {
    /// Built local App directory.
    #[arg(long, default_value = "dist")]
    from: PathBuf,
    /// Exact App-local Provider Instance (plugin-id/instance-key).
    #[arg(long)]
    provider: String,
    /// Tool name from the provider's catalog.
    #[arg(long)]
    name: String,
    /// One portable JSON value passed as tool arguments.
    #[arg(long)]
    arguments_json: String,
}

#[derive(Args, Clone, Debug)]
pub struct AppInitArgs {
    /// Host executable to copy into the App workspace.
    #[arg(long)]
    host: PathBuf,
    /// Host Catalog JSON emitted by the same Host Build.
    #[arg(long)]
    host_catalog: PathBuf,
    /// New App workspace directory. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Emit a stable JSON report.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Clone, Debug)]
pub struct ProjectArgs {
    /// App project root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Emit a stable JSON report.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Clone, Debug)]
pub struct ShowArgs {
    /// App project root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Emit a stable JSON report.
    #[arg(long)]
    json: bool,
    /// Distribution Host authority used by the private runtime resolver.
    #[arg(long, hide = true, requires = "runtime_json")]
    host_build: Option<PathBuf>,
    /// Emit the exact private runtime input rather than the management projection.
    #[arg(long, hide = true, requires = "host_build")]
    runtime_json: bool,
}

pub async fn app(command: AppCommand) -> anyhow::Result<()> {
    match command {
        AppCommand::Add(args) => convention_authoring::add(args),
        AppCommand::Unadopt(args) => convention_authoring::unadopt(args),
        AppCommand::ConfigSync(args) => configuration_source::sync_command(args),
        AppCommand::ConfigActivated(args) => configuration_source::activated_command(args),
        AppCommand::ConfigStatus(args) => configuration_source::status_command(args),
        AppCommand::Plugin { command } => convention_authoring::new(command),
        AppCommand::Contract { command } => contracts::scaffold::run(command),
        AppCommand::Build(args) => local_workflow::build(args),
        AppCommand::Create(args) => local_workflow::create(args),
        AppCommand::Start(args) => local_workflow::start_command(args).await,
        AppCommand::Tools { command } => match command {
            ToolCommand::Catalog(args) => local_workflow::start_built_local_tool_app(
                args.from,
                vec!["catalog".into(), args.provider],
            ),
            ToolCommand::Execute(args) => local_workflow::start_built_local_tool_app(
                args.from,
                vec![
                    "execute".into(),
                    args.provider,
                    args.name,
                    args.arguments_json,
                ],
            ),
        },
        AppCommand::Dev(args) => local_dev::dev(args).await,
        AppCommand::Runtime { args } => std::thread::spawn(move || portable_runtime::run(args))
            .join()
            .map_err(|_| anyhow::anyhow!("local runtime worker panicked"))?,
        AppCommand::Prepare(args) => prepare::prepare(args),
        AppCommand::Init(args) => init(args),
        AppCommand::Check(args) => check(args),
        AppCommand::Show(args) => show(args),
        AppCommand::Explain(args) => explain::run(args),
        AppCommand::Discover(args) => discover(args),
        AppCommand::LinkedCatalog(args) => convention_authoring::linked_catalog(args),
        AppCommand::LinkedDocument(args) => convention_authoring::linked_document(args),
        AppCommand::Inspect(args) => inspect(args),
        AppCommand::Facts(args) => facts::facts(args),
        AppCommand::Assemble(args) => assemble::assemble(args),
    }
}

fn discover(args: ProjectArgs) -> anyhow::Result<()> {
    let report = lenso_app_authoring::discovery::discover(&project_root(args.root)?)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for candidate in &report.candidates {
            println!(
                "{}@{}\t{:?}\t{}\t{}",
                candidate.plugin_id,
                candidate.release_version,
                candidate.role,
                candidate.format,
                candidate.project.display()
            );
        }
        println!(
            "Discovered {} candidates; no Plugins were built or activated.",
            report.candidates.len()
        );
    }
    Ok(())
}

fn init(args: AppInitArgs) -> anyhow::Result<()> {
    let root = args.root.unwrap_or(std::env::current_dir()?);
    let host = fs::canonicalize(&args.host)
        .with_context(|| format!("locate Host executable {}", args.host.display()))?;
    let host_metadata = fs::symlink_metadata(&host)?;
    if !host_metadata.file_type().is_file() {
        bail!("Host executable must be a regular file: {}", host.display());
    }
    let catalog_bytes = fs::read(&args.host_catalog)
        .with_context(|| format!("read Host Catalog {}", args.host_catalog.display()))?;
    let _: HostCatalog =
        serde_json::from_slice(&catalog_bytes).context("Host Catalog is invalid")?;
    let control = root.join(".lenso");
    let plugins = root.join("plugins");
    if control.exists() || plugins.exists() {
        bail!(
            "App workspace already contains `.lenso/` or `plugins/`: {}",
            root.display()
        );
    }
    fs::create_dir_all(&root)
        .with_context(|| format!("create App workspace {}", root.display()))?;
    let staging = tempfile::Builder::new()
        .prefix(".lenso-init-")
        .tempdir_in(&root)
        .context("stage App workspace")?;
    let staged_control = staging.path().join(".lenso");
    fs::create_dir(&staged_control)?;
    fs::copy(&host, staged_control.join("host"))?;
    fs::write(staged_control.join("host-catalog.json"), &catalog_bytes)?;
    fs::create_dir(staging.path().join("plugins"))?;
    fs::rename(&staged_control, &control)?;
    if let Err(error) = fs::rename(staging.path().join("plugins"), &plugins) {
        let _ = fs::remove_dir_all(&control);
        return Err(error).context("publish Plugin Root");
    }
    let resolved = load_resolved_app(&root).inspect_err(|_| {
        let _ = fs::remove_dir_all(&plugins);
        let _ = fs::remove_dir_all(&control);
    })?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "kind": "lenso.app-init",
                "status": "created",
                "root": root,
                "plugin_instances": resolved.instances().len(),
                "capability_bindings": resolved.plan().capability_bindings().len(),
            }))?
        );
    } else {
        println!("Created App workspace at {}.", root.display());
    }
    Ok(())
}

fn check(args: ProjectArgs) -> anyhow::Result<()> {
    let root = project_root(args.root)?;
    let report = inspect_app_check(&root)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "App is valid: {} Plugin Instance(s), {} Capability binding(s).",
            report.plugin_instances, report.capability_bindings
        );
    }
    Ok(())
}

fn show(args: ShowArgs) -> anyhow::Result<()> {
    let root = project_root(args.root)?;
    if args.runtime_json {
        let host_build = args
            .host_build
            .context("runtime resolution needs Host authority")?;
        let resolution = lenso_app_authoring::resolve_runtime_app(&root, &host_build)?;
        println!("{}", serde_json::to_string(&resolution)?);
        return Ok(());
    }
    let resolved = load_resolved_app(&root)?;
    if args.json {
        let instances = resolved
            .instances()
            .iter()
            .map(|instance| {
                serde_json::json!({
                    "id": instance.id().to_string(),
                    "source": format!("{:?}", instance.source()),
                    "plan_key": instance.plan_key(),
                })
            })
            .collect::<Vec<_>>();
        let bindings = resolved
            .plan()
            .capability_bindings()
            .iter()
            .map(|binding| {
                serde_json::json!({
                    "consumer_instance": binding.consumer_instance(),
                    "capability_id": binding.capability_id(),
                    "descriptor_version": binding.descriptor_version(),
                    "provider_instance": binding.provider_instance(),
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "kind": "lenso.app-show",
                "instances": instances,
                "bindings": bindings,
            }))?
        );
        return Ok(());
    }
    println!("Plugin Instances:");
    for instance in resolved.instances() {
        println!(
            "  {}  source={:?}  plan-key={}",
            instance.id(),
            instance.source(),
            instance.plan_key()
        );
    }
    println!("Capability bindings:");
    for binding in resolved.plan().capability_bindings() {
        println!(
            "  {} --{}@{}--> {}",
            binding.consumer_instance(),
            binding.capability_id(),
            binding.descriptor_version(),
            binding.provider_instance()
        );
    }
    Ok(())
}

pub use lenso_app_authoring::discovery::discover as discover_sources;
pub use local_workflow::{build_local, create_empty, start_distribution};

pub use convention_authoring::{Language, adopt, create_plugin};

fn inspect(args: ProjectArgs) -> anyhow::Result<()> {
    let report = lenso_app_authoring::discovery::discover(&project_root(args.root)?)?;
    let plan = lenso_app_authoring::discovery::conventions::plan(&report)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        for surface in &plan.surfaces {
            println!(
                "{}\t{}\t{}\t{}",
                surface.owner,
                surface.entry.display(),
                surface.reason,
                surface.support.as_deref().unwrap_or("-")
            );
        }
        println!("No package managers, compilers or Plugins were executed.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use lenso_app_plan::authoring::{
        HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    #[test]
    fn init_publishes_one_valid_app_workspace() {
        let temporary = tempfile::tempdir().unwrap();
        let host = temporary.path().join("host-source");
        fs::write(&host, b"host").unwrap();
        let catalog = HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(PluginDescriptor::new(
                "example.agent",
                "1.0.0",
                "agent",
            ))],
            [HostDefaultPlugin::new("example.agent", "default")],
        );
        let catalog_path = temporary.path().join("catalog.json");
        fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
        let root = temporary.path().join("app");

        init(AppInitArgs {
            host,
            host_catalog: catalog_path,
            root: Some(root.clone()),
            json: false,
        })
        .unwrap();

        assert!(root.join(".lenso/host").is_file());
        assert!(root.join("plugins").is_dir());
        assert_eq!(load_resolved_app(&root).unwrap().instances().len(), 1);
    }

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: AppCommand,
    }

    #[test]
    fn facts_is_available_through_the_shared_app_command_parser() {
        let parsed =
            TestCli::try_parse_from(["lenso", "facts", "--root", "app", "--json"]).unwrap();

        assert!(matches!(
            parsed.command,
            AppCommand::Facts(facts::FactsArgs {
                root: Some(root),
                host_build: None,
                json: true,
            }) if root.as_path() == std::path::Path::new("app")
        ));
        assert!(TestCli::try_parse_from(["lenso", "check", "--host-build", "build.json"]).is_err());
    }
}
