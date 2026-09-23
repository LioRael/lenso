//! Bundled support and ordinary local sources use the same discovery/adoption path.
use anyhow::{Context, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
include!(concat!(env!("OUT_DIR"), "/terminal_assets.rs"));
pub(super) mod linked_catalog;

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
    /// Local public trust configuration for the signed catalog.
    #[arg(long)]
    trust: Option<PathBuf>,
    /// Exact registry .crate archive; it must match the signed digest.
    #[arg(long = "crate")]
    crate_archive: Option<PathBuf>,
}
#[derive(Clone, Debug, Args)]
pub struct UnadoptArgs {
    /// Exact linked Cargo Plugin ID and version originally adopted by app add.
    source: String,
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
fn writable_path(root: &Path, relative: &Path) -> anyhow::Result<()> {
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
    if !Command::new("bun")
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
    if args.linked_snapshot.is_some() || args.trust.is_some() || args.crate_archive.is_some() {
        return linked_catalog::add(&root, &args);
    }
    if args.source == "@lenso/openapi" {
        return add_openapi(&root, args.no_install);
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

/// Bundle only the source link needed to make the existing optional Plugin
/// available to this generated Host. The App's Plugin Root selects it.
fn add_openapi(root: &Path, no_install: bool) -> anyhow::Result<()> {
    let relative = Path::new("support/lenso-openapi");
    let destination = root.join(relative);
    writable_path(root, relative)?;
    if fs::symlink_metadata(&destination).is_ok() {
        bail!(
            "OpenAPI support already exists at {}",
            destination.display()
        );
    }
    let mut document = preflight_source_adoption(root, "lenso.openapi")?;
    let source = relative.to_str().context("OpenAPI support source UTF-8")?;
    let sources = document
        .as_table_mut()
        .context("lenso.toml table")?
        .entry("plugin_sources")
        .or_insert_with(|| toml::Value::Array(vec![]))
        .as_array_mut()
        .context("plugin_sources array")?;
    if !sources.contains(&toml::Value::String(source.to_owned())) {
        sources.push(toml::Value::String(source.to_owned()));
    }
    let intent = root.join("plugins/lenso.openapi");
    if intent.exists() {
        bail!(
            "OpenAPI Plugin Root entry already exists at {}",
            intent.display()
        );
    }

    let parent = destination.parent().context("OpenAPI support parent")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".lenso-openapi-support-")
        .tempdir_in(parent)?;
    let revision = crate::plugin::LENSO_FRAMEWORK_REVISION;
    write(
        stage.path(),
        "Cargo.toml",
        format!(
            "[package]\nname = \"app-openapi-link\"\nversion = \"0.2.4\"\nedition = \"2024\"\npublish = false\n\n[package.metadata.lenso]\nplugin-id = \"lenso.openapi\"\nroot-slot = \"http-endpoints\"\n\n[dependencies]\nlenso = {{ version = \"=0.5.25\", git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-openapi-plugin = {{ version = \"=0.2.4\", git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\n\n[patch.crates-io]\nlenso = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-app-plan = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-kernel = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\nlenso-native-adapter = {{ git = \"https://github.com/LioRael/lenso\", rev = \"{revision}\" }}\n\n[workspace]\n"
        ),
    )?;
    write(
        stage.path(),
        "src/lib.rs",
        "//! Links the optional OpenAPI Plugin into this App's native Host.\n\npub fn link_plugin() { lenso_openapi_plugin::link_plugin(); }\n",
    )?;
    if !no_install {
        let status = super::cargo_command()
            .args(["generate-lockfile", "--manifest-path"])
            .arg(stage.path().join("Cargo.toml"))
            .status()?;
        if !status.success() {
            bail!("OpenAPI support lockfile generation failed");
        }
    }
    super::build::publish_new_output(stage.path(), &destination)?;
    fs::create_dir_all(&intent)?;
    fs::write(
        intent.join("default.toml"),
        "# Selecting this Instance publishes the public HTTP Endpoint document.\n",
    )?;
    let config = root.join("lenso.toml");
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    use std::io::Write;
    staged.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
    staged.persist(&config)?;
    println!("Selected optional OpenAPI Plugin at {}.", intent.display());
    Ok(())
}

fn preflight_source_adoption(root: &Path, plugin_id: &str) -> anyhow::Result<toml::Value> {
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
    linked_catalog::unadopt(&root, &args.source)
}

pub fn new(command: PluginCommand) -> anyhow::Result<()> {
    let PluginCommand::New(args) = command;
    lenso_app_authoring::identity::validate_plugin_id_v1(&args.id)?;
    let root = crate::plugins::project_root(args.root)?;
    writable_path(&root, Path::new("app"))?;
    let support = root.join("app/lenso-terminal-cli");
    if !support.is_dir() {
        bail!("install CLI support first: lenso app add @lenso/cli");
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
        trust: None,
        crate_archive: None,
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
mod tests {
    use std::fs;

    use super::{AddArgs, add};

    #[test]
    fn openapi_support_is_shared_source_selected_only_by_plugin_root() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("plugins")).unwrap();
        add(AddArgs {
            source: "@lenso/openapi".into(),
            root: Some(root.path().to_path_buf()),
            no_install: true,
            linked_snapshot: None,
            trust: None,
            crate_archive: None,
        })
        .unwrap();

        let report = lenso_app_authoring::discovery::discover(root.path()).unwrap();
        let [candidate] = report.candidates.as_slice() else {
            panic!("expected exactly one optional OpenAPI source")
        };
        assert_eq!(candidate.plugin_id, "lenso.openapi");
        assert_eq!(
            candidate.role,
            lenso_app_authoring::discovery::SourceRole::Shared
        );
        assert_eq!(candidate.implementations[0].runtime, "native-linked");
        assert!(
            root.path()
                .join("plugins/lenso.openapi/default.toml")
                .is_file()
        );
        assert!(
            fs::read_to_string(root.path().join("lenso.toml"))
                .unwrap()
                .contains("support/lenso-openapi")
        );
        let manifest = fs::read_to_string(candidate.project.join("Cargo.toml")).unwrap();
        assert!(manifest.contains(crate::plugin::LENSO_FRAMEWORK_REVISION));
        assert!(
            fs::read_to_string(candidate.project.join("src/lib.rs"))
                .unwrap()
                .contains("lenso_openapi_plugin::link_plugin()")
        );
    }
}
