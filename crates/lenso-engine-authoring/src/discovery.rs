//! Read-only local Plugin source discovery. Candidates confer no execution authority.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub mod conventions;
mod project;

const MAX_ENTRIES: usize = 50_000;
const MAX_DEPTH: usize = 64;
const EXCLUDED: &[&str] = &[
    ".git",
    ".lenso",
    "target",
    "node_modules",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
    "plugins",
];

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    #[serde(default)]
    plugin_sources: Vec<String>,
    #[serde(default, rename = "development_host")]
    _development_host: Option<String>,
    /// Optional support configuration is interpreted by App contract authoring.
    #[serde(default, rename = "contracts")]
    _contracts: Option<toml::Value>,
}

/// Where a candidate was found, not whether an Instance is enabled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    AppOwned,
    Shared,
}

/// Build metadata only; declarations and runtime availability still require validation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Implementation {
    pub id: String,
    pub runtime: String,
    pub project: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    /// Source-declared native Plugin's public module linkage anchor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_link: Option<String>,
    /// Logical owner of a selected additive build contribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composite: Option<PathBuf>,
    pub plugin_id: String,
    pub release_version: String,
    pub project: PathBuf,
    pub metadata: PathBuf,
    pub format: String,
    pub role: SourceRole,
    pub implementations: Vec<Implementation>,
    /// Files declared by this selected Plugin source for publication alongside
    /// the App distribution. They are data artifacts, never runtime Plugin
    /// resources or implicit executable entrypoints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub published_resources: Vec<PublishedResource>,
    pub evidence: String,
}

/// One bounded, schema-labelled source file that an App build publishes as
/// immutable distribution data. Consumers select and interpret schemas; the
/// App preset only copies and hashes the declared bytes.
#[derive(Clone, Debug, serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedResource {
    /// Relative regular-file path inside the Plugin project.
    pub path: String,
    /// Consumer-owned schema identifying the resource payload.
    pub schema: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveryReport {
    pub schema_version: u32,
    #[serde(skip_deserializing, default = "discovery_kind")]
    pub kind: &'static str,
    pub root: PathBuf,
    pub candidates: Vec<Candidate>,
}

/// Discover local projects without invoking package managers, build scripts or Plugins.
pub fn discover(root: &Path) -> anyhow::Result<DiscoveryReport> {
    discover_in(root, &lenso_engine::discovery::DiscoverySession::new(root)?)
}

/// Exact regular source inputs of the root package's declared Plugins.
/// Reuses Rust's bounded public, unconditional module traversal or the Bun
/// package's declared source entry. Filenames alone confer no source status.
/// This is build projection input, never an exception to runtime Root validation.
pub fn source_files_in(
    root: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<BTreeSet<PathBuf>> {
    project::source_files_in(root, inputs)
}

/// Read-only Plugin discovery sharing a caller's explicitly selected inputs.
pub fn discover_in(
    root: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<DiscoveryReport> {
    let root = fs::canonicalize(root).context("resolve App discovery root")?;
    if !root.is_dir() {
        bail!("App discovery root must be a directory");
    }
    let config_path = root.join("lenso.toml");
    let config: Configuration = if config_path.try_exists()? {
        toml::from_str(&read_metadata_in(&config_path, inputs)?)
            .context("parse optional lenso.toml tooling configuration")?
    } else {
        Configuration::default()
    };
    let mut scanner = Scanner {
        inputs: inputs.scope(&root)?,
        visited: BTreeMap::new(),
        candidates: BTreeMap::new(),
        entries: 0,
    };
    // The App's root package can itself be the App-owned business Plugin.
    // Inspect only its package metadata here: recursively scanning the root
    // would also treat unrelated workspace members as adopted Plugins.
    for candidate in project::read_all_in(&root, SourceRole::AppOwned, inputs)
        .context("inspect App root package")?
    {
        scanner.insert(candidate)?;
    }
    let app = root.join("app");
    if app.try_exists()? {
        scanner.scan(&app, SourceRole::AppOwned, 0)?;
    }
    for source in config.plugin_sources {
        for path in expand(&root, &source)? {
            scanner.scan(&path, SourceRole::Shared, 0)?;
        }
    }
    Ok(DiscoveryReport {
        schema_version: 1,
        kind: "lenso.app-discovery",
        root,
        candidates: scanner.candidates.into_values().collect(),
    })
}

fn discovery_kind() -> &'static str {
    "lenso.app-discovery"
}

struct Scanner {
    inputs: lenso_engine::discovery::DiscoverySession,
    visited: BTreeMap<PathBuf, SourceRole>,
    candidates: BTreeMap<String, Candidate>,
    entries: usize,
}

impl Scanner {
    fn scan(&mut self, path: &Path, role: SourceRole, depth: usize) -> anyhow::Result<()> {
        if depth > MAX_DEPTH {
            bail!(
                "local Plugin discovery exceeds {MAX_DEPTH} directory levels at {}",
                path.display()
            );
        }
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            bail!("local Plugin discovery exceeds {MAX_ENTRIES} entries; narrow plugin_sources");
        }
        let path = fs::canonicalize(path)
            .with_context(|| format!("resolve local Plugin source {}", path.display()))?;
        if let Some(previous) = self.visited.get(&path) {
            if *previous != role {
                bail!(
                    "local Plugin source {} overlaps app/ and shared sources; remove the shared overlap",
                    path.display()
                );
            }
            return Ok(());
        }
        self.visited.insert(path.clone(), role);
        let metadata = fs::metadata(&path)?;
        if metadata.is_file() {
            if path
                .extension()
                .is_some_and(|extension| extension == "lenso-plugin")
            {
                let candidate = project::bundle(&path, role)?;
                self.insert(candidate)?;
            } else {
                bail!(
                    "unsupported local Plugin source {}; expected a project directory or .lenso-plugin archive",
                    path.display()
                );
            }
            return Ok(());
        }
        if !metadata.is_dir() {
            bail!(
                "local Plugin source must be a directory or archive: {}",
                path.display()
            );
        }
        if path.join("plugin.json").try_exists()? {
            self.insert(conventions::composite(&path, role, &self.inputs)?)?;
            return Ok(());
        }
        if path.join(lenso_plugin_bundle::MANIFEST_FILE).try_exists()? {
            self.insert(project::bundle(&path, role)?)?;
            return Ok(());
        }
        let candidates = project::read_all_in(&path, role, &self.inputs)
            .with_context(|| format!("inspect Plugin source {}", path.display()))?;
        if !candidates.is_empty() {
            // A Plugin owns its nested implementation projects and build products.
            for candidate in candidates {
                self.insert(candidate)?;
            }
            return Ok(());
        }
        if let Some(members) = project::workspace_members(&path, &self.inputs)? {
            for member in members {
                self.scan(&member, role, depth + 1)?;
            }
            return Ok(());
        }
        let mut children = Vec::new();
        for entry in self.inputs.scope(&path)?.directory("")? {
            self.entries += 1;
            if self.entries > MAX_ENTRIES {
                bail!(
                    "local Plugin discovery exceeds {MAX_ENTRIES} entries; narrow plugin_sources"
                );
            }
            children.push(path.join(entry.path));
        }
        children.sort();
        for child in children {
            let name = child
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if EXCLUDED.contains(&name) || name.starts_with('.') {
                continue;
            }
            let metadata = fs::symlink_metadata(&child)?;
            // Follow explicitly configured roots only. Nested links never escape the source boundary.
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir()
                || child
                    .extension()
                    .is_some_and(|extension| extension == "lenso-plugin")
            {
                self.scan(&child, role, depth + 1)?;
            }
        }
        Ok(())
    }

    fn insert(&mut self, candidate: Candidate) -> anyhow::Result<()> {
        if let Some(previous) = self.candidates.get(&candidate.plugin_id) {
            bail!(
                "duplicate Plugin identity `{}` at {} and {}; select distinct source roots or remove the duplicate",
                candidate.plugin_id,
                previous.project.display(),
                candidate.project.display()
            );
        }
        self.candidates
            .insert(candidate.plugin_id.clone(), candidate);
        Ok(())
    }
}

fn expand(root: &Path, source: &str) -> anyhow::Result<Vec<PathBuf>> {
    if source.trim().is_empty() || source.contains("://") {
        bail!("plugin_sources accepts nonempty local paths, not URLs");
    }
    if source.contains("**") {
        bail!(
            "recursive ** patterns are not supported; name a local directory to discover recursively"
        );
    }
    let joined = root.join(source);
    // Escape the project prefix so a directory name containing glob syntax is literal.
    let pattern = if Path::new(source).is_absolute() {
        source.to_owned()
    } else {
        format!(
            "{}/{}",
            glob::Pattern::escape(root.to_str().context("App path is not UTF-8")?),
            source
        )
    };
    if !source.contains(['*', '?', '[']) {
        if !joined.try_exists()? {
            bail!("local Plugin source does not exist: {}", joined.display());
        }
        return Ok(vec![joined]);
    }
    let paths = glob::glob_with(
        &pattern,
        glob::MatchOptions {
            require_literal_separator: true,
            require_literal_leading_dot: true,
            ..Default::default()
        },
    )?
    .collect::<Result<BTreeSet<_>, _>>()?;
    if paths.is_empty() {
        bail!("local Plugin source pattern `{source}` matches no paths");
    }
    Ok(paths.into_iter().collect())
}

fn read_metadata(path: &Path) -> anyhow::Result<String> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("Plugin metadata must be a regular file: {}", path.display());
    }
    let mut text = String::new();
    fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_string(&mut text)
        .with_context(|| format!("read metadata {}", path.display()))?;
    if text.len() as u64 > LIMIT {
        bail!("Plugin metadata exceeds 4 MiB: {}", path.display());
    }
    Ok(text)
}

fn read_metadata_in(
    path: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<String> {
    let directory = path.parent().context("metadata parent")?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("metadata filename")?;
    let bytes = inputs.scope(directory)?.read(name, 4 * 1024 * 1024)?;
    Ok(std::str::from_utf8(&bytes)?.to_owned())
}

#[cfg(test)]
mod tests;
