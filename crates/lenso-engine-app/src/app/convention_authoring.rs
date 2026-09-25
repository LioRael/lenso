//! Bundled support and ordinary local sources use the same discovery/adoption path.
use anyhow::{Context, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
include!(concat!(env!("OUT_DIR"), "/terminal_assets.rs"));
pub(crate) mod linked_catalog;
mod openapi;

#[derive(Clone, Debug, Args)]
pub struct AddArgs {
    /// Local Plugin source directory, or bundled @lenso/cli or @lenso/openapi support.
    source: String,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    no_install: bool,
    /// Exact signed source-only linked Cargo snapshot.
    #[arg(long)]
    linked_snapshot: Option<PathBuf>,
    /// Exact signed Portable snapshot for a source App Bundle adoption.
    #[arg(long, conflicts_with = "linked_snapshot")]
    portable_snapshot: Option<PathBuf>,
    /// Local public trust configuration for the signed catalog.
    #[arg(long)]
    trust: Option<PathBuf>,
    /// Exact registry .crate archive; it must match the signed digest.
    #[arg(long = "crate", conflicts_with = "bundle")]
    crate_archive: Option<PathBuf>,
    /// Verified V6 Bundle carrying the signed .crate as a Host build input.
    #[arg(long, conflicts_with = "crate_archive")]
    bundle: Option<PathBuf>,
    /// Exact downloaded Portable archive matching the signed snapshot.
    #[arg(long, conflicts_with = "origin")]
    archive: Option<PathBuf>,
    /// Independently allowed HTTPS origin for the signed Portable archive.
    #[arg(long, conflicts_with = "archive")]
    origin: Option<String>,
    /// Replace the selected linked Cargo Plugin version using signed new-release inputs.
    #[arg(long)]
    replace: bool,
    /// Separately signed v2 source-content snapshot for this exact Plugin release.
    #[arg(long)]
    content_snapshot: Option<PathBuf>,
    /// Exact content ID within the signed v2 release.
    #[arg(long)]
    content_id: Option<String>,
    /// Local downloaded `.tar.gz` content archive matching the signed digest.
    #[arg(long)]
    content_archive: Option<PathBuf>,
    /// New App-relative directory that will own an editable copy of the content.
    #[arg(long)]
    content_destination: Option<PathBuf>,
    /// Verify the signed input and show its file plan without writing to the App.
    #[arg(long)]
    content_preview: bool,
}
#[derive(Clone, Debug, Args)]
pub struct UnadoptArgs {
    /// Exact signed Plugin ID and version originally adopted by app add.
    source: String,
    /// Unselect a signed Portable source App Bundle, retaining its exact archive.
    #[arg(long)]
    portable: bool,
    #[arg(long)]
    root: Option<PathBuf>,
}
#[derive(Clone, Debug, Args)]
pub struct LinkedCatalogArgs {
    /// Optional text matched against signed Plugin ID, title, and summary.
    query: Option<String>,
    /// Exact signed source-only linked Cargo snapshot.
    #[arg(long)]
    linked_snapshot: PathBuf,
    /// Local public trust configuration for the signed catalog.
    #[arg(long)]
    trust: PathBuf,
    /// Host target to check; defaults to this machine's Native target.
    #[arg(long)]
    target: Option<String>,
    /// Restrict results to candidates with no known hard rejection for this App.
    #[arg(long, requires = "root")]
    recommendations: bool,
    /// Source App root used only for read-only recommendation facts.
    #[arg(long, requires = "recommendations")]
    root: Option<PathBuf>,
    /// Exclude candidates unless signed metadata proves they request no permissions.
    #[arg(long, requires = "recommendations")]
    require_no_permissions: bool,
    /// Exclude candidates unless signed metadata proves no external service is required.
    #[arg(long, requires = "recommendations")]
    require_no_external_services: bool,
    /// Exclude candidates unless signed metadata proves no external fee is required.
    #[arg(long, requires = "recommendations")]
    require_no_fees: bool,
    /// Emit a stable JSON report.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
pub struct LinkedDocumentArgs {
    /// Exact signed Plugin ID and version.
    source: String,
    /// Exact document ID in the signed release.
    document_id: String,
    /// Exact immutable document revision.
    #[arg(long)]
    revision: String,
    /// Exact signed source-only linked Cargo snapshot.
    #[arg(long)]
    linked_snapshot: PathBuf,
    /// Local public trust configuration for the signed catalog.
    #[arg(long)]
    trust: PathBuf,
    /// Local downloaded Markdown file; its bytes must match the signed metadata.
    #[arg(long, conflicts_with = "fetch")]
    file: Option<PathBuf>,
    /// Explicitly fetch the signed HTTPS document URL.
    #[arg(long, conflicts_with = "file")]
    fetch: bool,
    /// UTF-8 byte offset for a bounded document chunk.
    #[arg(long, default_value_t = 0)]
    offset: usize,
    /// Maximum UTF-8 bytes in one chunk, from 4 to 8192.
    #[arg(long, default_value_t = 4096)]
    max_bytes: usize,
    /// Emit a stable JSON report.
    #[arg(long)]
    json: bool,
}

pub fn linked_document(args: LinkedDocumentArgs) -> anyhow::Result<()> {
    let (plugin_id, version) = args
        .source
        .split_once('@')
        .context("document source must be exact PLUGIN_ID@VERSION")?;
    let chunk = linked_catalog::document(linked_catalog::DocumentRequest {
        snapshot_path: &args.linked_snapshot,
        trust_path: &args.trust,
        plugin_id,
        version,
        document_id: &args.document_id,
        revision: &args.revision,
        local_file: args.file.as_deref(),
        fetch: args.fetch,
        offset: args.offset,
        max_bytes: args.max_bytes,
    })?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&chunk)?);
    } else {
        print!("{}", chunk.content);
        if let Some(next) = chunk.next_offset {
            eprintln!("\nNext verified document offset: {next}");
        }
    }
    Ok(())
}

pub fn linked_catalog(args: LinkedCatalogArgs) -> anyhow::Result<()> {
    let target = args
        .target
        .unwrap_or_else(|| lenso_app_authoring::native_host_target().to_owned());
    if args.recommendations {
        let report = linked_catalog::recommend(
            args.root
                .as_deref()
                .context("--root is required for recommendations")?,
            &args.linked_snapshot,
            &args.trust,
            args.query.as_deref().unwrap_or_default(),
            &target,
            linked_catalog::RecommendationRestrictions {
                require_no_permissions: args.require_no_permissions,
                require_no_external_services: args.require_no_external_services,
                require_no_fees: args.require_no_fees,
            },
        )?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            if report.releases.is_empty() {
                println!("No signed linked Cargo candidates passed known hard filters.");
            }
            for release in &report.releases {
                println!(
                    "{}@{}\tneeds verification ({})\t{}",
                    release.plugin_id,
                    release.version,
                    release.unverified.join(", "),
                    release
                        .summary
                        .chars()
                        .flat_map(char::escape_default)
                        .collect::<String>()
                );
            }
            for release in &report.excluded {
                println!(
                    "{}@{}\texcluded: {}",
                    release.plugin_id,
                    release.version,
                    release.rejection_reasons.join(", ")
                );
            }
        }
        return Ok(());
    }
    let report = linked_catalog::inspect(
        &args.linked_snapshot,
        &args.trust,
        args.query.as_deref().unwrap_or_default(),
        &target,
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.releases.is_empty() {
        println!("No signed linked Cargo candidates matched.");
    } else {
        for release in &report.releases {
            println!(
                "{}@{}\t{}\t{}",
                release.plugin_id, release.version, release.adoption, release.summary
            );
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Subcommand)]
pub enum PluginCommand {
    /// Create a CLI command Plugin; use `lenso plugin new --web` for Web Plugins.
    New(NewArgs),
}
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Language {
    Ts,
    Rust,
}
#[derive(Clone, Debug, Args)]
pub struct NewArgs {
    id: String,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "ts")]
    language: Language,
    #[arg(long)]
    no_install: bool,
}
fn write(root: &Path, name: &str, bytes: impl AsRef<[u8]>) -> anyhow::Result<()> {
    let path = root.join(name);
    fs::create_dir_all(path.parent().context("file parent")?)?;
    fs::write(path, bytes)?;
    Ok(())
}
pub(crate) fn writable_path(root: &Path, relative: &Path) -> anyhow::Result<()> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("refusing to modify a symlink: {}", path.display())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn install(root: &Path) -> anyhow::Result<()> {
    if !super::build_command("bun")
        .args(["install", "--ignore-scripts"])
        .current_dir(root)
        .status()?
        .success()
    {
        bail!("Bun dependency installation failed at {}", root.display());
    }
    Ok(())
}
fn package(id: &str, source: &str) -> serde_json::Value {
    json!({"name":id,"version":"1.0.0","private":true,"type":"module",
        "scripts":{"check":"tsc --noEmit"},
        "dependencies":{"@lenso/bun-plugin":"0.4.1","@lenso/contract-runtime":"0.3.0"},
        "devDependencies":{"typescript":"7.0.2","@types/bun":"1.4.0"},
        "lenso":{"pluginId":id,"runtime":"bun","rootSlot":"tools","source":source}})
}
fn tsconfig(root: &Path, source: &str) -> anyhow::Result<()> {
    write(
        root,
        "tsconfig.json",
        serde_json::to_vec_pretty(
            &json!({"compilerOptions":{"strict":true,"noEmit":true,"module":"Preserve","moduleResolution":"bundler","allowImportingTsExtensions":true,"types":["bun"]},"include":[source]}),
        )?,
    )
}

pub fn add(args: AddArgs) -> anyhow::Result<()> {
    let root = fs::canonicalize(crate::plugins::project_root(args.root.clone())?)?;
    if args.content_snapshot.is_some()
        || args.content_id.is_some()
        || args.content_archive.is_some()
        || args.content_destination.is_some()
        || args.content_preview
    {
        return linked_catalog::content::add(&root, &args);
    }
    if args.portable_snapshot.is_some() || args.archive.is_some() || args.origin.is_some() {
        if args.linked_snapshot.is_some() || args.crate_archive.is_some() || args.bundle.is_some() {
            bail!(
                "signed Portable adoption cannot use linked Cargo snapshot, crate, or Bundle inputs"
            );
        }
        return crate::plugins::signed_install::adopt_source(
            &root,
            &args.source,
            args.portable_snapshot
                .as_deref()
                .context("--portable-snapshot required")?,
            args.trust.as_deref().context("--trust required")?,
            args.archive.as_deref(),
            args.origin.as_deref(),
            args.replace,
        );
    }
    if args.linked_snapshot.is_some()
        || args.trust.is_some()
        || args.crate_archive.is_some()
        || args.bundle.is_some()
    {
        return linked_catalog::add(&root, &args);
    }
    if args.replace {
        bail!("--replace requires an exact signed linked Cargo source");
    }
    if args.source == "@lenso/openapi" {
        return openapi::add(&root, args.no_install);
    }
    writable_path(&root, Path::new("app"))?;
    if args.source == "@lenso/cli" {
        let destination = root.join("app/lenso-terminal-cli");
        if destination.exists() {
            bail!("CLI support already exists at {}", destination.display());
        }
        fs::create_dir_all(root.join("app"))?;
        let stage = tempfile::Builder::new()
            .prefix(".lenso-support-")
            .tempdir_in(root.join("app"))?;
        for (name, bytes) in TERMINAL_ASSETS {
            write(
                stage.path(),
                name.strip_suffix(".template").unwrap_or(name),
                bytes,
            )?;
        }
        let mut metadata = package("lenso.terminal.cli", "consumer.ts");
        metadata["name"] = "@lenso/cli".into();
        metadata["exports"] = json!({".":"./sdk.ts"});
        metadata["lenso"]["conventions"] = json!([
            {"id":"lenso.cli.typescript","entries":["cli.ts"],"compiler":{"program":"bun","args":["compiler.mjs"]}},
            {"id":"lenso.cli.rust","entries":["cli.rs"],"compiler":{"program":"bun","args":["compiler.mjs"]}},
            {"id":"lenso.cli.router","entries":["router.ts"]}
        ]);
        metadata["lenso"]["surfaces"] =
            json!([{"entry":"router/router.ts","project":"router","required":true}]);
        write(
            stage.path(),
            "package.json",
            serde_json::to_vec_pretty(&metadata)?,
        )?;
        tsconfig(stage.path(), "consumer.ts")?;
        for name in ["router.ts", "provider.ts", "command.ts"] {
            write(
                stage.path(),
                &format!("router/{name}"),
                fs::read(stage.path().join(name))?,
            )?;
        }
        write(
            stage.path(),
            "router/package.json",
            serde_json::to_vec_pretty(&package("lenso.terminal.command", "router.ts"))?,
        )?;
        tsconfig(&stage.path().join("router"), "router.ts")?;
        write(
            stage.path(),
            "rust-sdk/src/generated.rs",
            include_str!("terminal/provider.rs"),
        )?;
        for (name, bytes) in TERMINAL_ASSETS {
            if let Some(relative) = name.strip_prefix("contracts/provider/") {
                write(stage.path(), &format!("rust-sdk/{relative}"), bytes)?;
            }
        }
        super::build::publish_new_output(stage.path(), &destination)?;
        if !args.no_install {
            install(&destination)?;
            install(&destination.join("router"))?;
        }
        println!("Adopted bundled CLI support at {}", destination.display());
        return Ok(());
    }
    let source = fs::canonicalize(&args.source)
        .context("app add accepts a local source path, @lenso/cli, or @lenso/openapi")?;
    let probe = tempfile::tempdir()?;
    write(
        probe.path(),
        "lenso.toml",
        toml::to_string(&json!({"plugin_sources":[source]}))?,
    )?;
    let report = lenso_app_authoring::discovery::discover(probe.path())?;
    let [candidate] = report.candidates.as_slice() else {
        bail!("select one Plugin source package, not a workspace of candidates");
    };
    let mut document = preflight_source_adoption(&root, &candidate.plugin_id)?;
    let config = root.join("lenso.toml");
    let sources = document
        .as_table_mut()
        .context("lenso.toml table")?
        .entry("plugin_sources")
        .or_insert_with(|| toml::Value::Array(vec![]))
        .as_array_mut()
        .context("plugin_sources array")?;
    let portable_source = source.strip_prefix(&root).unwrap_or(&source);
    let value = toml::Value::String(
        portable_source
            .to_str()
            .context("source path UTF-8")?
            .to_owned(),
    );
    if !source.starts_with(root.join("app")) && !sources.contains(&value) {
        sources.push(value);
    }
    if !source.starts_with(root.join("app")) {
        let mut staged = tempfile::NamedTempFile::new_in(&root)?;
        use std::io::Write;
        staged.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
        staged.persist(&config)?;
    }
    let intent = root.join("plugins").join(&candidate.plugin_id);
    fs::create_dir_all(&intent)?;
    let default = intent.join("default.toml");
    if !default.exists() {
        fs::write(default, "# Explicit local Plugin adoption\n")?;
    }
    if !args.no_install && candidate.format == "bun" {
        install(&candidate.project)?;
    }
    println!("Adopted {} from {}", candidate.plugin_id, source.display());
    Ok(())
}

pub(crate) fn preflight_source_adoption(
    root: &Path,
    plugin_id: &str,
) -> anyhow::Result<toml::Value> {
    writable_path(root, Path::new("app"))?;
    writable_path(root, Path::new("lenso.toml"))?;
    let intent_relative = Path::new("plugins").join(plugin_id);
    for name in ["default.toml", "default.disabled"] {
        writable_path(root, &intent_relative.join(name))?;
    }
    let config = root.join("lenso.toml");
    let document: toml::Value = if config.exists() {
        toml::from_str(&fs::read_to_string(&config)?)?
    } else {
        toml::Value::Table(Default::default())
    };
    let table = document.as_table().context("lenso.toml table")?;
    if let Some(sources) = table.get("plugin_sources") {
        sources
            .as_array()
            .context("lenso.toml plugin_sources array")?;
    }
    Ok(document)
}

pub fn unadopt(args: UnadoptArgs) -> anyhow::Result<()> {
    let root = fs::canonicalize(crate::plugins::project_root(args.root)?)?;
    if args.portable {
        return crate::plugins::signed_install::unadopt_source(&root, &args.source);
    }
    linked_catalog::unadopt(&root, &args.source)
}

pub fn new(command: PluginCommand) -> anyhow::Result<()> {
    let PluginCommand::New(args) = command;
    lenso_app_authoring::identity::validate_plugin_id_v1(&args.id)?;
    let root = crate::plugins::project_root(args.root)?;
    writable_path(&root, Path::new("app"))?;
    let support = root.join("app/lenso-terminal-cli");
    if !support.is_dir() {
        bail!(
            "App CLI command Plugins require support from `lenso app add @lenso/cli`; \
             for a Web Plugin, use `lenso plugin new <id> --web --repo-root <app>/app`"
        );
    }
    let destination = root.join("app").join(&args.id);
    if destination.exists() {
        bail!("Plugin project already exists");
    }
    let stage = tempfile::Builder::new()
        .prefix(".lenso-plugin-")
        .tempdir_in(root.join("app"))?;
    match args.language {
        Language::Ts => {
            let mut metadata = package(&args.id, "plugin.ts");
            metadata["dependencies"]["@lenso/cli"] = "file:../lenso-terminal-cli".into();
            write(
                stage.path(),
                "package.json",
                serde_json::to_vec_pretty(&metadata)?,
            )?;
            write(
                stage.path(),
                "plugin.ts",
                "import { definePlugin } from '@lenso/bun-plugin';\nexport default definePlugin({ provides: [], create() { return {}; } });\n",
            )?;
            write(
                stage.path(),
                "cli.ts",
                "import { command } from '@lenso/cli';\nexport default command({\n  name: 'hello',\n  description: 'Say hello',\n  args: { name: { type: 'string', default: 'world' } },\n  run({ args, output }) { output.text(`Hello, ${args.name}!`); },\n});\n",
            )?;
            tsconfig(stage.path(), "*.ts")?;
        }
        Language::Rust => {
            write(
                stage.path(),
                "Cargo.toml",
                format!(
                    "[package]\nname = {:?}\nversion = \"1.0.0\"\nedition = \"2024\"\n[workspace]\n[package.metadata.lenso]\nplugin-id = {:?}\nroot-slot = \"tools\"\n[dependencies]\nlenso = \"=0.5.23\"\n",
                    args.id.replace('.', "-"),
                    args.id
                ),
            )?;
            write(
                stage.path(),
                "src/lib.rs",
                "#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n",
            )?;
            write(
                stage.path(),
                "cli.rs",
                "use lenso_cli_support::command;\n\n/// Say hello from Rust\n#[command(name = \"hello-rust\")]\nasync fn hello(#[arg(long, default = \"world\")] name: String) -> anyhow::Result<String> {\n    Ok(format!(\"Hello, {name}!\"))\n}\n",
            )?;
        }
    }
    super::build::publish_new_output(stage.path(), &destination)?;
    if !args.no_install && matches!(args.language, Language::Ts) {
        install(&destination)?;
    }
    if !args.no_install
        && matches!(args.language, Language::Rust)
        && !super::cargo_command()
            .arg("check")
            .current_dir(&destination)
            .status()?
            .success()
    {
        bail!("initial Rust Plugin check failed");
    }
    println!("Created {} at {}", args.id, destination.display());
    Ok(())
}

pub fn adopt(root: PathBuf, source: String, install_dependencies: bool) -> anyhow::Result<()> {
    add(AddArgs {
        root: Some(root),
        source,
        no_install: !install_dependencies,
        linked_snapshot: None,
        portable_snapshot: None,
        trust: None,
        crate_archive: None,
        bundle: None,
        archive: None,
        origin: None,
        replace: false,
        content_snapshot: None,
        content_id: None,
        content_archive: None,
        content_destination: None,
        content_preview: false,
    })
}
pub fn create_plugin(
    root: PathBuf,
    id: String,
    language: Language,
    install_dependencies: bool,
) -> anyhow::Result<()> {
    new(PluginCommand::New(NewArgs {
        root: Some(root),
        id,
        language,
        no_install: !install_dependencies,
    }))
}

#[cfg(test)]
mod plugin_new_tests {
    use super::{Language, NewArgs, PluginCommand, new};

    #[test]
    fn app_cli_plugin_creation_points_web_authors_to_web_scaffold() {
        let root = tempfile::tempdir().unwrap();
        let error = new(PluginCommand::New(NewArgs {
            id: "local.clock".into(),
            root: Some(root.path().into()),
            language: Language::Rust,
            no_install: true,
        }))
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("lenso app add @lenso/cli"));
        assert!(message.contains("lenso plugin new <id> --web --repo-root <app>/app"));
    }
}
