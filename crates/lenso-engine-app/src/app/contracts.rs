//! Local contract synchronization is authoring work, never runtime discovery.
use anyhow::{Context, bail, ensure};
use lenso_app_authoring::discovery::Candidate;
use lenso_contract_codegen_next::{ProjectionLanguage, lint_compatibility, load_descriptor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

mod freshness;
pub mod scaffold;
mod snapshot;
mod source;
use freshness::file_digest;
pub(super) use freshness::{Freshness, check};
use snapshot::{safe_directory, safe_output, snapshot, snapshot_files};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Projection {
    projection: String,
    output: PathBuf,
    #[serde(default)]
    module: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    descriptor: PathBuf,
    #[serde(default)]
    source: Option<PathBuf>,
    #[serde(default)]
    projection: Option<String>,
    #[serde(default)]
    output: Option<PathBuf>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    projections: Vec<Projection>,
}
struct Contract {
    root: PathBuf,
    declaration: Declaration,
    cargo: Option<(Value, Value)>,
    // Registry/git sources remain read-only. Projections are consumer-local.
    output_root: Option<PathBuf>,
    published_projections: Vec<Projection>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Configuration {
    roots: Vec<PathBuf>,
    exclude_paths: Vec<PathBuf>,
    exclude_packages: Vec<String>,
    dependency_output: PathBuf,
    /// Consumer-selected targets override external packages' published targets.
    dependency_projections: Vec<Projection>,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            roots: vec!["contracts".into()],
            exclude_paths: vec![],
            exclude_packages: vec![],
            dependency_output: ".lenso/contracts/dependencies".into(),
            dependency_projections: vec![],
        }
    }
}
fn configuration(root: &Path) -> anyhow::Result<Configuration> {
    let path = root.join("lenso.toml");
    if !path.exists() {
        return Ok(Configuration::default());
    }
    let document: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    document
        .get("contracts")
        .cloned()
        .map(|value| value.try_into().map_err(Into::into))
        .unwrap_or_else(|| Ok(Configuration::default()))
}

pub(super) fn synchronize(root: &Path, candidates: &[Candidate]) -> anyhow::Result<Vec<Freshness>> {
    let config = configuration(root)?;
    let mut contracts = BTreeMap::new();
    let mut manifests = BTreeSet::new();
    let exclusions = config
        .exclude_paths
        .iter()
        .map(|path| safe_directory(root, path))
        .collect::<anyhow::Result<Vec<_>>>()?;
    // Only selected Plugin projects and explicitly owned contracts participate.
    for candidate in candidates {
        match candidate.format.as_str() {
            "cargo" => {
                manifests.insert(candidate.project.join("Cargo.toml"));
            }
            "bun" => {
                read_npm(&candidate.project, &mut contracts)?;
            }
            _ => {}
        }
    }
    for directory in &config.roots {
        let directory = safe_directory(root, directory)?;
        if directory.is_dir() {
            scan(
                &directory,
                &mut manifests,
                &mut contracts,
                &mut 0,
                0,
                &exclusions,
            )?;
        }
    }
    let mut visited_packages = BTreeSet::new();
    for manifest in manifests {
        let metadata = cargo_metadata(&manifest)?;
        let packages = metadata["packages"].as_array().context("Cargo packages")?;
        // Dependency closure, not every unrelated member in a sibling workspace.
        let selected = packages
            .iter()
            .find(|p| Path::new(p["manifest_path"].as_str().unwrap_or("")) == manifest)
            .or_else(|| {
                packages
                    .iter()
                    .find(|p| metadata["resolve"]["root"] == p["id"])
            });
        let mut pending = if let Some(package) = selected {
            vec![package["id"].clone()]
        } else {
            metadata["workspace_members"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        };
        let nodes = match metadata["resolve"]["nodes"].as_array() {
            Some(nodes) => nodes.as_slice(),
            None => {
                ensure!(
                    dependency_free(&metadata),
                    "Cargo dependency graph is missing"
                );
                &[]
            }
        };
        while let Some(id) = pending.pop() {
            if !visited_packages.insert(id.as_str().context("Cargo package ID")?.to_owned()) {
                continue;
            }
            if let Some(node) = nodes.iter().find(|n| n["id"] == id) {
                for dep in node["deps"].as_array().context("Cargo dependencies")? {
                    if dep["dep_kinds"].as_array().is_some_and(|kinds| {
                        kinds
                            .iter()
                            .any(|k| k["kind"].is_null() || k["kind"] == "build")
                    }) {
                        pending.push(dep["pkg"].clone());
                    }
                }
            }
            let package = packages
                .iter()
                .find(|p| p["id"] == id)
                .context("Cargo package")?;
            if config
                .exclude_packages
                .contains(&package["name"].as_str().unwrap_or("").to_owned())
            {
                continue;
            }
            let Some(value) = package.pointer("/metadata/lenso/contract") else {
                continue;
            };
            let package_root = Path::new(
                package["manifest_path"]
                    .as_str()
                    .context("Cargo manifest")?,
            )
            .parent()
            .context("Cargo root")?
            .to_path_buf();
            insert(
                &mut contracts,
                package_root.clone(),
                value.clone(),
                Some((package.clone(), metadata.clone())),
            )?;
            if !package["source"].is_null() {
                let contract = contracts
                    .get_mut(&package_root)
                    .context("dependency contract")?;
                configure_external(root, package, &config, contract)?;
            }
        }
    }
    synchronize_selected(root, contracts)
}
fn configure_external(
    root: &Path,
    package: &Value,
    config: &Configuration,
    contract: &mut Contract,
) -> anyhow::Result<()> {
    ensure!(
        contract
            .root
            .join(&contract.declaration.descriptor)
            .is_file(),
        "{}: external source-only contracts must publish Descriptor/Schema inputs; dependency code is not executed for extraction",
        package["id"]
    );
    contract.published_projections = contract.declaration.projections.clone();
    if let (Some(kind), Some(output)) = (
        &contract.declaration.projection,
        &contract.declaration.output,
    ) {
        contract.published_projections.push(Projection {
            projection: kind.clone(),
            output: output.clone(),
            module: contract.declaration.module.clone(),
        });
    }
    contract.declaration.source = None;
    let identity = super::local_host::digest_text(package["id"].as_str().context("package ID")?);
    contract.output_root = Some(safe_directory(root, &config.dependency_output)?.join(identity));
    if !config.dependency_projections.is_empty() {
        contract.declaration.projection = None;
        contract.declaration.output = None;
        contract.declaration.projections = config.dependency_projections.clone();
    }
    Ok(())
}
fn synchronize_selected(
    root: &Path,
    contracts: BTreeMap<PathBuf, Contract>,
) -> anyhow::Result<Vec<Freshness>> {
    if contracts.is_empty() {
        return Ok(vec![]);
    }
    fs::create_dir_all(root.join(".lenso"))?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(".lenso/contracts.lock"))?;
    lock.lock().context("lock local contract generation")?;
    let staging = tempfile::tempdir_in(root.join(".lenso"))?;
    let inputs = contracts
        .values()
        .filter(|contract| contract.output_root.is_none())
        .map(|contract| {
            Ok((
                contract.root.clone(),
                super::local_host::input_digest(&contract.root)?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut external_inputs = BTreeMap::new();
    let mut changes = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut owned = BTreeSet::new();
    let mut baselines = Vec::new();
    let mut ordered = contracts.values().collect::<Vec<_>>();
    ordered.sort_by_key(|c| c.declaration.source.is_none());
    let mut staged_sources = BTreeMap::<PathBuf, PathBuf>::new();
    for (index, contract) in ordered.into_iter().enumerate() {
        let declaration = &contract.declaration;
        let descriptor = safe_output(&contract.root, &declaration.descriptor)?;
        let stage = staging.path().join(index.to_string());
        fs::create_dir_all(&stage)?;
        let staged_descriptor = stage.join("capability.json");
        let source_path = declaration.source.clone();
        let source_owned = source_path.is_some();
        if !source_owned && descriptor.is_file() {
            for (relative, bytes) in snapshot_files(&descriptor)? {
                let path = if relative == Path::new("capability.json") {
                    descriptor.clone()
                } else {
                    descriptor.parent().unwrap().join(relative)
                };
                external_inputs.insert(path, bytes);
            }
        }
        if let Some(source_path) = source_path {
            let source_path = safe_output(&contract.root, &source_path)?;
            source::extract(root, contract, Some(&source_path), &staged_descriptor)?;
        } else {
            snapshot(
                staged_sources.get(&descriptor).unwrap_or(&descriptor),
                &stage,
            )?;
            if contract.output_root.is_none()
                && contract.cargo.as_ref().is_some_and(|(package, _)| {
                    package["dependencies"].as_array().is_some_and(|deps| {
                        deps.iter().any(|dep| {
                            dep["kind"] == "build" && dep["name"] == "lenso-contract-codegen"
                        })
                    })
                })
            {
                source::extract(root, contract, None, &staged_descriptor)?;
            }
        }
        if source_owned {
            staged_sources.insert(descriptor.clone(), staged_descriptor.clone());
        }
        let next = load_descriptor(&staged_descriptor)
            .map_err(|e| anyhow::anyhow!("{}: {e}", descriptor.display()))?;
        let key = if contract.output_root.is_some() {
            super::local_host::digest_text(&format!("dependency/{}", next.capability_id()))
        } else {
            super::local_host::digest_text(&descriptor.to_string_lossy())
        };
        let baseline = root.join(".lenso/contracts").join(key);
        let previous = if baseline.join("capability.json").is_file() {
            Some(baseline.join("capability.json"))
        } else if source_owned && descriptor.is_file() {
            Some(descriptor.clone())
        } else {
            None
        };
        if let Some(previous) = previous {
            let old = load_descriptor(&previous).map_err(|e| anyhow::anyhow!("{e}"))?;
            if old.descriptor_digest() != next.descriptor_digest() {
                lint_compatibility(&previous, &staged_descriptor).map_err(|e| anyhow::anyhow!("{}: contract change needs an explicit compatible version or a new Capability identity/package: {e}", descriptor.display()))?;
            }
        }
        let mut projections = declaration.projections.clone();
        match (&declaration.projection, &declaration.output) {
            (Some(projection), Some(output)) => projections.push(Projection {
                projection: projection.clone(),
                output: output.clone(),
                module: declaration.module.clone(),
            }),
            (None, None) => {}
            _ => bail!(
                "{}: contract projection and output must be declared together",
                contract.root.display()
            ),
        }
        if projections.is_empty() {
            bail!(
                "{}: contract needs at least one generated projection",
                contract.root.display()
            );
        }
        for (projection_index, projection) in projections.into_iter().enumerate() {
            let language = match projection.projection.as_str() {
                "rust" => ProjectionLanguage::Rust,
                "rust-runtime" => ProjectionLanguage::RustRuntime,
                "rust-plugin" => ProjectionLanguage::RustPlugin,
                "typescript" => ProjectionLanguage::TypeScript,
                "wit" => ProjectionLanguage::Wit,
                value => bail!("unsupported contract projection {value}"),
            };
            let output_root = contract.output_root.as_ref().unwrap_or(&contract.root);
            let output = safe_output(output_root, &projection.output)?;
            if let Ok(existing) = fs::read_to_string(&output)
                && !existing
                    .lines()
                    .take(3)
                    .any(|line| line.contains("@generated by lenso-contract-codegen"))
            {
                bail!(
                    "refusing to overwrite authored file with a contract projection: {}",
                    output.display()
                );
            }
            if !owned.insert(output.clone()) {
                bail!("generated output has multiple owners: {}", output.display());
            }
            let extracted = stage.join(format!("projection-{projection_index}.txt"));
            // Preserve local projections from a compatible generator cohort when
            // the resolved descriptor (including schemas) is unchanged. An
            // explicit exporter or module request still owns regeneration.
            if !source_owned
                && contract.output_root.is_none()
                && projection.module.is_none()
                && !extracted.is_file()
                && fs::read_to_string(&output)
                    .is_ok_and(|text| text.contains(next.descriptor_digest()))
            {
                continue;
            }
            let bytes = if extracted.is_file() {
                fs::read(extracted)?
            } else if let Some((path, bytes)) =
                published_projection(contract, &projection, next.descriptor_digest())?
            {
                external_inputs.insert(path, bytes.clone());
                bytes
            } else {
                if matches!(
                    language,
                    ProjectionLanguage::Rust
                        | ProjectionLanguage::RustRuntime
                        | ProjectionLanguage::RustPlugin
                ) && targets_legacy_guest(contract)
                {
                    bail!(
                        "{}: regeneration needs this contract's compatible lenso-contract-codegen build-dependency; the bundled generator targets Guest SDK 0.5",
                        contract.root.display()
                    );
                }
                let mut inputs = lenso_engine::Snapshot::default();
                lenso_engine_contracts::snapshot_contract(
                    &stage,
                    "capability.json",
                    "input/capability.json",
                    &mut inputs,
                )?;
                let mut identity = descriptor.display().to_string();
                if let Some((package, metadata)) = &contract.cargo {
                    identity = package["id"]
                        .as_str()
                        .context("locked Cargo package ID")?
                        .to_owned();
                    let workspace = Path::new(
                        metadata["workspace_root"]
                            .as_str()
                            .context("Cargo workspace")?,
                    );
                    let lock = workspace.join("Cargo.lock");
                    if lock.exists() {
                        inputs.insert("lock/Cargo.lock".into(), fs::read(lock)?)?;
                    } else {
                        ensure!(
                            contract.output_root.is_none(),
                            "external contracts require an existing consumer Cargo.lock"
                        );
                    }
                }
                let kind = match language {
                    ProjectionLanguage::Rust => lenso_engine_contracts::ProjectionKind::Rust,
                    ProjectionLanguage::RustRuntime => {
                        lenso_engine_contracts::ProjectionKind::RustRuntime
                    }
                    ProjectionLanguage::RustPlugin => {
                        lenso_engine_contracts::ProjectionKind::RustPlugin
                    }
                    ProjectionLanguage::TypeScript => {
                        lenso_engine_contracts::ProjectionKind::TypeScript
                    }
                    ProjectionLanguage::Wit => lenso_engine_contracts::ProjectionKind::Wit,
                };
                lenso_engine_contracts::run(
                    inputs,
                    vec![lenso_engine_contracts::ContractInput {
                        identity,
                        descriptor: "input/capability.json".into(),
                        baseline: None,
                        projections: vec![lenso_engine_contracts::Projection {
                            kind,
                            output: "projection.txt".into(),
                            module: projection.module.clone(),
                        }],
                    }],
                    &stage,
                    Some(&root.join(".lenso/contracts/cache")),
                    lenso_engine_contracts::Mode::Generate,
                )?;
                let bytes = fs::read(stage.join("projection.txt"))?;
                // Reuse a matching published dependency projection without ever
                // writing to its immutable source package.
                if contract.output_root.is_some()
                    && fs::read(contract.root.join(&projection.output))
                        .ok()
                        .as_ref()
                        == Some(&bytes)
                {
                    fs::read(contract.root.join(&projection.output))?
                } else {
                    bytes
                }
            };
            changes.insert(output, bytes);
        }
        if source_owned {
            for (relative, bytes) in snapshot_files(&staged_descriptor)? {
                let target = if relative == Path::new("capability.json") {
                    safe_output(&contract.root, &declaration.descriptor)?
                } else {
                    safe_output(descriptor.parent().context("Descriptor parent")?, &relative)?
                };
                if !owned.insert(target.clone()) {
                    bail!(
                        "generated snapshot has multiple owners: {}",
                        target.display()
                    );
                }
                changes.insert(target, bytes);
            }
        }
        baselines.push((baseline, stage));
    }
    for (path, digest) in inputs {
        if super::local_host::input_digest(&path)? != digest {
            bail!(
                "contract source changed during generation: {}; retry after edits settle",
                path.display()
            );
        }
    }
    for (path, bytes) in &external_inputs {
        if fs::read(path)?.as_slice() != bytes.as_slice() {
            bail!(
                "contract input changed during generation: {}; retry after edits settle",
                path.display()
            );
        }
    }
    // Validate every contract before installing any output. Preserve unchanged mtimes.
    let backups = changes
        .keys()
        .map(|path| {
            Ok((
                path.clone(),
                if path.exists() {
                    Some(fs::read(path)?)
                } else {
                    None
                },
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if let Err(error) = install(&changes) {
        for (path, previous) in backups {
            match previous {
                Some(bytes) => {
                    let _ = fs::write(path, bytes);
                }
                None => {
                    let _ = fs::remove_file(path);
                }
            }
        }
        return Err(error);
    }
    for (baseline, stage) in baselines {
        fs::create_dir_all(&baseline)?;
        snapshot(&stage.join("capability.json"), &baseline)?;
    }
    eprintln!("Synchronized {} local contract packages", contracts.len());
    let mut evidence = Vec::new();
    for contract in contracts.values() {
        let descriptor = contract.root.join(&contract.declaration.descriptor);
        let mut files = BTreeMap::new();
        for (relative, _) in snapshot_files(&descriptor)? {
            let path = if relative == Path::new("capability.json") {
                descriptor.clone()
            } else {
                descriptor
                    .parent()
                    .context("descriptor parent")?
                    .join(relative)
            };
            files.insert(path.clone(), file_digest(&path)?);
        }
        if let Some((_, metadata)) = &contract.cargo {
            let path = Path::new(
                metadata["workspace_root"]
                    .as_str()
                    .context("Cargo workspace")?,
            )
            .join("Cargo.lock");
            if path.exists() {
                files.insert(path.clone(), file_digest(&path)?);
            }
        }
        for path in external_inputs
            .keys()
            .filter(|path| path.starts_with(&contract.root))
        {
            files.insert(path.clone(), file_digest(path)?);
        }
        for filename in ["Cargo.toml", "package.json"] {
            let path = contract.root.join(filename);
            if path.exists() {
                files.insert(path.clone(), file_digest(&path)?);
            }
        }
        for projection in &contract.declaration.projections {
            let path = contract
                .output_root
                .as_ref()
                .unwrap_or(&contract.root)
                .join(&projection.output);
            files.insert(path.clone(), file_digest(&path)?);
        }
        if let Some(output) = &contract.declaration.output {
            let path = contract
                .output_root
                .as_ref()
                .unwrap_or(&contract.root)
                .join(output);
            files.insert(path.clone(), file_digest(&path)?);
        }
        evidence.push(Freshness {
            root: contract.root.clone(),
            files,
            generator_version: lenso_contract_codegen_next::GENERATOR_VERSION.into(),
            generator_revision: lenso_contract_codegen_next::GENERATOR_REVISION.into(),
            configuration: if root.join("lenso.toml").exists() {
                Some(file_digest(&root.join("lenso.toml"))?)
            } else {
                None
            },
            source_digest: if contract.declaration.source.is_some() {
                Some(super::local_host::input_digest(&contract.root)?)
            } else {
                None
            },
        });
    }
    Ok(evidence)
}

fn published_projection(
    contract: &Contract,
    requested: &Projection,
    digest: &str,
) -> anyhow::Result<Option<(PathBuf, Vec<u8>)>> {
    if contract.output_root.is_none() {
        return Ok(None);
    }
    for published in &contract.published_projections {
        if published.projection != requested.projection || published.module != requested.module {
            continue;
        }
        let path = safe_output(&contract.root, &published.output)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= 32 * 1024 * 1024 => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Ok(_) => bail!(
                "published projection is not a bounded regular file: {}",
                path.display()
            ),
            Err(error) => return Err(error.into()),
        }
        let bytes = fs::read(&path)?;
        if let Ok(text) = std::str::from_utf8(&bytes)
            && text
                .lines()
                .take(3)
                .any(|line| line.contains("@generated by lenso-contract-codegen"))
            && text.contains(digest)
        {
            return Ok(Some((path, bytes)));
        }
    }
    Ok(None)
}

fn targets_legacy_guest(contract: &Contract) -> bool {
    let Some((package, metadata)) = &contract.cargo else {
        return false;
    };
    let Some(nodes) = metadata["resolve"]["nodes"].as_array() else {
        return false;
    };
    let Some(dependencies) = nodes
        .iter()
        .find(|node| node["id"] == package["id"])
        .and_then(|node| node["deps"].as_array())
    else {
        return false;
    };
    metadata["packages"].as_array().is_some_and(|packages| {
        dependencies.iter().any(|dep| {
            packages.iter().any(|p| {
                p["id"] == dep["pkg"]
                    && p["name"] == "lenso-guest-sdk"
                    && p["version"]
                        .as_str()
                        .is_some_and(|version| version.starts_with("0.4."))
            })
        })
    })
}

fn install(changes: &BTreeMap<PathBuf, Vec<u8>>) -> anyhow::Result<()> {
    for (path, bytes) in changes {
        if fs::read(path).ok().as_ref() == Some(bytes) {
            continue;
        }
        fs::create_dir_all(path.parent().context("generated output parent")?)?;
        fs::write(path, bytes)
            .with_context(|| format!("write generated contract {}", path.display()))?;
    }
    Ok(())
}
fn insert(
    contracts: &mut BTreeMap<PathBuf, Contract>,
    root: PathBuf,
    value: Value,
    cargo: Option<(Value, Value)>,
) -> anyhow::Result<()> {
    let declaration: Declaration = serde_json::from_value(value)
        .with_context(|| format!("{}: invalid lenso.contract metadata", root.display()))?;
    contracts.entry(root.clone()).or_insert(Contract {
        root,
        declaration,
        cargo,
        output_root: None,
        published_projections: Vec::new(),
    });
    Ok(())
}
fn read_npm(root: &Path, contracts: &mut BTreeMap<PathBuf, Contract>) -> anyhow::Result<()> {
    let document: Value = serde_json::from_slice(&fs::read(root.join("package.json"))?)?;
    if let Some(value) = document.pointer("/lenso/contract") {
        insert(contracts, fs::canonicalize(root)?, value.clone(), None)?;
    }
    Ok(())
}
fn scan(
    root: &Path,
    manifests: &mut BTreeSet<PathBuf>,
    contracts: &mut BTreeMap<PathBuf, Contract>,
    visited: &mut usize,
    depth: usize,
    exclusions: &[PathBuf],
) -> anyhow::Result<()> {
    if exclusions.iter().any(|excluded| root.starts_with(excluded)) {
        return Ok(());
    }
    *visited += 1;
    if *visited > 5000 || depth > 32 {
        bail!("local contracts exceed traversal limits");
    }
    if root.join("Cargo.toml").is_file() {
        manifests.insert(fs::canonicalize(root.join("Cargo.toml"))?);
        return Ok(());
    }
    if root.join("package.json").is_file() {
        read_npm(root, contracts)?;
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        if entry.file_type()?.is_dir()
            && !name.to_string_lossy().starts_with('.')
            && !["target", "node_modules", "dist", "build"]
                .contains(&name.to_string_lossy().as_ref())
        {
            scan(
                &entry.path(),
                manifests,
                contracts,
                visited,
                depth + 1,
                exclusions,
            )?;
        }
    }
    Ok(())
}
fn dependency_free(metadata: &Value) -> bool {
    metadata["packages"].as_array().is_some_and(|packages| {
        packages.iter().all(|package| {
            package["dependencies"]
                .as_array()
                .is_some_and(Vec::is_empty)
        })
    })
}

fn cargo_metadata(manifest: &Path) -> anyhow::Result<Value> {
    // Establish the actual workspace owner without resolving dependencies,
    // fetching packages, running build scripts, or creating a lockfile.
    let mut probe = super::cargo_command();
    probe.args([
        "metadata",
        "--no-deps",
        "--offline",
        "--format-version=1",
        "--manifest-path",
    ]);
    let output = probe.arg(manifest).output()?;
    ensure!(
        output.status.success(),
        "contract dependency discovery: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let local: Value = serde_json::from_slice(&output.stdout)?;
    let lock_path = Path::new(
        local["workspace_root"]
            .as_str()
            .context("Cargo workspace root")?,
    )
    .join("Cargo.lock");
    let lock_before = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= 32 * 1024 * 1024 => {
            Some(fs::read(&lock_path)?)
        }
        Ok(_) => bail!(
            "Cargo dependency lock is not a bounded regular file: {}",
            lock_path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if lock_before.is_none() {
        ensure!(
            dependency_free(&local),
            "Cargo dependencies are not prepared: {} has no Cargo.lock; explicitly resolve/install dependencies before contract discovery (for --no-install projects, run cargo generate-lockfile --manifest-path {})",
            lock_path
                .parent()
                .context("Cargo workspace directory")?
                .display(),
            manifest.display()
        );
        // Dependency-free local descriptors need no resolution graph or lock.
        return Ok(local);
    }
    let mut command = super::cargo_command();
    command.args([
        "metadata",
        "--locked",
        "--format-version=1",
        "--manifest-path",
    ]);
    command.arg(manifest);
    let output = command.output()?;
    if let Some(before) = &lock_before {
        ensure!(
            fs::read(&lock_path)? == *before,
            "Cargo dependency lock changed during contract discovery: {}",
            lock_path.display()
        );
    }
    if !output.status.success() {
        bail!(
            "contract dependency discovery: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    Ok(metadata)
}
#[cfg(test)]
mod tests;
