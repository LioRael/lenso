use crate::{ContractInput, Projection, ProjectionKind, relative};
use anyhow::{Context, ensure};
use lenso_engine::{Snapshot, discovery::DiscoverySession};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// Small default discovery policy; applications may instead provide inputs.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DiscoveryOptions {
    pub roots: Vec<String>,
    pub descriptor_filename: String,
    /// Relative source prefixes. Excluded trees are never parsed.
    pub exclude: Vec<String>,
    pub targets: Vec<ProjectionTarget>,
    /// Explicit per-contract module names override the target's template.
    pub module_overrides: BTreeMap<String, String>,
    /// Accepted snapshots are read-only during discovery/check. None replaces
    /// this policy with explicitly supplied ContractInput.baseline values.
    pub baseline_root: Option<String>,
}
impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            roots: vec!["contracts".into()],
            descriptor_filename: "capability.json".into(),
            exclude: vec![
                "target".into(),
                "node_modules".into(),
                "dist".into(),
                ".lenso".into(),
            ],
            targets: vec![ProjectionTarget {
                kind: ProjectionKind::TypeScript,
                output: "{contract}/generated.ts".into(),
                module: None,
            }],
            module_overrides: BTreeMap::new(),
            baseline_root: Some(".lenso/contracts/accepted".into()),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionTarget {
    pub kind: ProjectionKind,
    /// Supports {contract} (source directory) and {name} (snake-case basename).
    pub output: String,
    #[serde(default)]
    pub module: Option<String>,
}

pub fn discover(
    root: &Path,
    options: &DiscoveryOptions,
) -> anyhow::Result<(Snapshot, Vec<ContractInput>)> {
    discover_in(&mut DiscoverySession::new(root)?, options)
}

/// Select contracts using a caller-owned acquisition epoch.
pub fn discover_in(
    session: &mut DiscoverySession,
    options: &DiscoveryOptions,
) -> anyhow::Result<(Snapshot, Vec<ContractInput>)> {
    ensure!(
        Path::new(&options.descriptor_filename).components().count() == 1,
        "descriptor_filename must be a filename"
    );
    relative(&options.descriptor_filename)?;
    for excluded in &options.exclude {
        relative(excluded)?;
    }
    let mut selected = Vec::new();
    let mut visited = 0;
    for source in &options.roots {
        relative(source)?;
        contained(session.root(), source)?;
        walk(session, source, options, &mut selected, &mut visited, 0)?;
    }
    selected.sort();
    selected.dedup();
    let mut snapshot = Snapshot::default();
    let mut inputs = Vec::new();
    for descriptor in selected {
        let directory = Path::new(&descriptor)
            .parent()
            .context("descriptor parent")?
            .to_str()
            .context("UTF8 path")?;
        let name = Path::new(directory)
            .file_name()
            .context("contract name")?
            .to_str()
            .context("UTF8 name")?
            .replace('-', "_");
        let template = |value: &str| {
            value
                .replace("{contract}", directory)
                .replace("{name}", &name)
        };
        let projections = options
            .targets
            .iter()
            .map(|target| Projection {
                kind: target.kind,
                output: template(&target.output),
                module: target.module.as_ref().map(|module| {
                    options
                        .module_overrides
                        .get(directory)
                        .cloned()
                        .unwrap_or_else(|| template(module))
                }),
            })
            .collect();
        snapshot_contract_in(session, &descriptor, &descriptor, &mut snapshot)?;
        inputs.push(ContractInput {
            identity: descriptor.clone(),
            descriptor,
            projections,
            baseline: None,
        });
    }
    if let Some(baselines) = &options.baseline_root {
        crate::baselines::attach(session.root(), baselines, &mut snapshot, &mut inputs)?;
    }
    Ok((snapshot, inputs))
}

fn walk(
    session: &mut DiscoverySession,
    relative_path: &str,
    options: &DiscoveryOptions,
    selected: &mut Vec<String>,
    visited: &mut usize,
    depth: usize,
) -> anyhow::Result<()> {
    ensure!(depth <= 32, "contract discovery exceeds 32 levels");
    if options
        .exclude
        .iter()
        .any(|prefix| relative_path == prefix || relative_path.starts_with(&format!("{prefix}/")))
    {
        return Ok(());
    }
    for entry in session.directory(relative_path)? {
        *visited += 1;
        ensure!(*visited <= 4096, "contract discovery exceeds 4096 entries");
        let path = entry.path;
        let name = Path::new(&path)
            .file_name()
            .context("contract filename")?
            .to_str()
            .context("UTF8 filename")?;
        if options
            .exclude
            .iter()
            .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
        {
            continue;
        }
        let kind = entry.kind;
        ensure!(
            !kind.is_symlink() && (kind.is_file() || kind.is_dir()),
            "contract input contains symlink/special file: {path}"
        );
        if kind.is_dir() {
            walk(session, &path, options, selected, visited, depth + 1)?;
        } else if name == options.descriptor_filename {
            selected.push(path);
        }
    }
    Ok(())
}

/// Snapshot a descriptor and its package-local JSON inputs into a caller-selected
/// namespace. Registry/git callers keep the source read-only and write outputs
/// elsewhere. Executables and source extraction are deliberately not involved.
pub fn snapshot_contract(
    source_root: &Path,
    descriptor: &str,
    snapshot_descriptor: &str,
    snapshot: &mut Snapshot,
) -> anyhow::Result<()> {
    snapshot_contract_in(
        &mut DiscoverySession::new(source_root)?,
        descriptor,
        snapshot_descriptor,
        snapshot,
    )
}

/// Extend an epoch with a descriptor's mandatory schema closure.
pub fn snapshot_contract_in(
    session: &mut DiscoverySession,
    descriptor: &str,
    snapshot_descriptor: &str,
    snapshot: &mut Snapshot,
) -> anyhow::Result<()> {
    relative(snapshot_descriptor)?;
    let source_base = Path::new(descriptor)
        .parent()
        .context("descriptor parent")?;
    let target_base = Path::new(snapshot_descriptor)
        .parent()
        .context("snapshot parent")?;
    let bytes = session.read(descriptor, 4 * 1024 * 1024)?.to_vec();
    let value = session.json(descriptor, 4 * 1024 * 1024)?;
    let mut pending = Vec::new();
    for operation in value["operations"]
        .as_array()
        .context("contract operations")?
    {
        for (key, value) in operation.as_object().context("contract operation")? {
            if key.ends_with("_schema") {
                pending.push(value.as_str().context("schema path")?.to_owned());
            }
        }
    }
    insert(snapshot, snapshot_descriptor.to_owned(), bytes)?;
    let mut seen = std::collections::BTreeSet::new();
    while let Some(path) = pending.pop() {
        relative(&path)?;
        if !seen.insert(path.clone()) {
            continue;
        }
        ensure!(seen.len() <= 1024, "contract exceeds 1024 schema inputs");
        let source = source_base.join(&path);
        let source = source.to_str().context("UTF8 schema path")?;
        let bytes = session.read(source, 4 * 1024 * 1024)?.to_vec();
        let schema = session.json(source, 4 * 1024 * 1024)?;
        let mut refs = Vec::new();
        references(&schema, &mut refs);
        for reference in refs {
            let filename = reference.split('#').next().unwrap_or_default();
            if filename.is_empty() {
                continue;
            }
            ensure!(
                !filename.starts_with('/') && !filename.contains([':', '\\']),
                "external/absolute schema reference: {reference}"
            );
            let parent = Path::new(&path).parent().context("schema parent")?;
            let joined = parent.join(filename);
            let mut normalized = std::path::PathBuf::new();
            for part in joined.components() {
                match part {
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => ensure!(
                        normalized.pop(),
                        "schema reference escapes descriptor closure: {reference}"
                    ),
                    std::path::Component::Normal(part) => normalized.push(part),
                    _ => anyhow::bail!("invalid schema reference: {reference}"),
                }
            }
            pending.push(normalized.to_str().context("UTF8 reference")?.to_owned());
        }
        insert(
            snapshot,
            target_base
                .join(path)
                .to_str()
                .context("UTF8 input")?
                .to_owned(),
            bytes,
        )?;
    }
    Ok(())
}
fn insert(snapshot: &mut Snapshot, path: String, bytes: Vec<u8>) -> anyhow::Result<()> {
    if let Some(previous) = snapshot.files().get(&path) {
        ensure!(*previous == bytes, "conflicting contract input {path}");
    } else {
        snapshot.insert(path, bytes)?;
    }
    Ok(())
}
fn references<'a>(value: &'a serde_json::Value, refs: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::Object(fields) => {
            if let Some(reference) = fields.get("$ref").and_then(serde_json::Value::as_str) {
                refs.push(reference);
            }
            for value in fields.values() {
                references(value, refs);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                references(value, refs);
            }
        }
        _ => {}
    }
}

pub(crate) fn contained(root: &Path, path: &str) -> anyhow::Result<()> {
    relative(path)?;
    ensure!(
        fs::symlink_metadata(root)?.is_dir(),
        "source/output root must be a real directory"
    );
    let mut current = root.to_path_buf();
    for part in Path::new(path).components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "symlink path: {}",
                current.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
