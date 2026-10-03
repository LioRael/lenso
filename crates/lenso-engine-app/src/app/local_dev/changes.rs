//! Watch events select work, never authority. Configuration reuse also verifies
//! source bytes and resolves against the exact retained Host before activation.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use serde::Serialize;
use tokio::{sync::mpsc, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Work {
    Frontend,
    ConsoleFrontend,
    Configuration,
    Rust,
    TypeScript,
    Generation,
}

pub(super) struct Batch {
    pub paths: BTreeSet<PathBuf>,
    pub observed: Instant,
    pub classification: Option<Work>,
    pub invalidation_reason: Option<String>,
    pub host_build_invoked: bool,
    pub packaged_plugins: Vec<String>,
    pub affected_instances: Vec<String>,
}

impl Batch {
    pub fn new(event: notify::Event) -> Self {
        Self {
            paths: event
                .paths
                .into_iter()
                .filter(|path| super::relevant(path))
                .collect(),
            observed: Instant::now(),
            classification: None,
            invalidation_reason: None,
            host_build_invoked: false,
            packaged_plugins: Vec::new(),
            affected_instances: Vec::new(),
        }
    }

    pub async fn collect(
        &mut self,
        events: &mut mpsc::Receiver<notify::Result<notify::Event>>,
    ) -> anyhow::Result<()> {
        tokio::time::sleep(Duration::from_millis(150)).await;
        while let Ok(event) = events.try_recv() {
            let event = event.context("App watcher failed during debounce")?;
            if super::rebuild_event(&event) {
                self.paths
                    .extend(event.paths.into_iter().filter(|path| super::relevant(path)));
            }
        }
        Ok(())
    }

    pub fn work(&self, root: &Path, frontend_enabled: bool) -> Work {
        if self.paths.is_empty() {
            return Work::Generation;
        }
        if let Some(classification) = self.classification {
            return classification;
        }
        let paths = self
            .paths
            .iter()
            .filter(|path| !(frontend_enabled && super::frontend::is_frontend(root, path)));
        let backend: Vec<_> = paths.collect();
        if backend.is_empty() {
            return Work::Frontend;
        }
        if backend
            .iter()
            .all(|path| instance_configuration(root, path))
        {
            return Work::Configuration;
        }
        if backend
            .iter()
            .all(|path| path.extension().is_some_and(|ext| ext == "rs"))
        {
            return Work::Rust;
        }
        if backend.iter().all(|path| {
            path.extension().is_some_and(|ext| {
                matches!(ext.to_str(), Some("ts" | "tsx" | "js" | "jsx" | "mjs"))
            })
        }) {
            return Work::TypeScript;
        }
        Work::Generation
    }

    pub fn report(
        &self,
        root: &Path,
        work: Work,
        status: &str,
        revision: u64,
        compiled: bool,
        reason: &str,
    ) -> anyhow::Result<()> {
        let report = serde_json::json!({
            "schema":"lenso.dev-feedback.v1", "change":work, "status":status,
            "generation":revision, "build_invoked":compiled, "elapsed_ms":self.observed.elapsed().as_millis(), "dev_process_id":std::process::id(),
            "reason":reason, "paths":self.paths,
            "host_build_invoked":self.host_build_invoked, "packaged_plugins":self.packaged_plugins,
            "affected_instances":self.affected_instances,
            "activation_scope":if work == Work::Frontend { "frontend_reload" } else { "host_generation" },
            "invalidation_reason":self.invalidation_reason,
        });
        let mut temporary = tempfile::NamedTempFile::new_in(root.join(".lenso"))?;
        use std::io::Write;
        temporary.write_all(&serde_json::to_vec_pretty(&report)?)?;
        temporary.persist(root.join(".lenso/dev-feedback.json"))?;
        eprintln!(
            "Dev {:?}: {status} in {} ms ({reason})",
            work,
            self.observed.elapsed().as_millis()
        );
        Ok(())
    }
}

pub(super) fn instance_configuration(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root.join("plugins")) else {
        return false;
    };
    relative.components().count() == 2 && path.extension().is_some_and(|ext| ext == "toml")
}

#[derive(Debug)]
pub(super) struct Inputs {
    roots: BTreeSet<PathBuf>,
    files: std::collections::BTreeMap<PathBuf, String>,
    configurations: BTreeSet<PathBuf>,
}

impl Inputs {
    pub fn capture(root: &Path, frontend_enabled: bool) -> anyhow::Result<Option<Self>> {
        let session = lenso_engine::discovery::DiscoverySession::new(root)?;
        let report = lenso_app_authoring::discovery::discover_in(root, &session)?;
        // A selected convention may consume configuration as compiler input.
        // Its owning processor must decide reuse; the generic dev loop cannot.
        if !lenso_app_authoring::discovery::conventions::plan_in(&report, &session)?
            .compilations
            .is_empty()
        {
            return Ok(None);
        }
        let mut roots = BTreeSet::from([root.to_path_buf()]);
        roots.extend(
            report
                .candidates
                .iter()
                .map(|candidate| candidate.project.clone()),
        );
        let dependencies = root.join(".lenso/host-cache/watch-roots.json");
        if dependencies.is_file() {
            roots.extend(serde_json::from_slice::<Vec<PathBuf>>(&fs::read(
                dependencies,
            )?)?);
        }
        let mut files = std::collections::BTreeMap::new();
        let mut configurations = BTreeSet::new();
        for source in &roots {
            if !source.is_dir() {
                anyhow::bail!("incremental input must be a directory");
            }
            let mut scoped = session.scope(source)?;
            let mut pending = vec![String::new()];
            let mut entries = 0;
            while let Some(directory) = pending.pop() {
                for entry in scoped.directory(&directory)? {
                    entries += 1;
                    anyhow::ensure!(
                        entries <= 50_000,
                        "incremental input exceeds 50,000 entries"
                    );
                    let path = source.join(&entry.path);
                    if directory.is_empty()
                        && super::super::local_host::generated_distribution(&path)?
                    {
                        continue;
                    }
                    if !super::relevant(&path)
                        || (frontend_enabled && super::frontend::is_frontend(root, &path))
                    {
                        continue;
                    }
                    if entry.kind.is_dir() {
                        pending.push(entry.path);
                    } else if entry.kind.is_file() {
                        if instance_configuration(root, &path) {
                            configurations.insert(path);
                        } else {
                            use sha2::{Digest, Sha256};
                            let digest =
                                Sha256::digest(scoped.read(&entry.path, 64 * 1024 * 1024)?);
                            files.insert(
                                path,
                                digest.iter().map(|byte| format!("{byte:02x}")).collect(),
                            );
                        }
                    } else {
                        anyhow::bail!("incremental source contains a symlink or special file");
                    }
                }
            }
        }
        Ok(Some(Self {
            roots,
            files,
            configurations,
        }))
    }

    pub fn matches(&self, root: &Path, frontend_enabled: bool) -> anyhow::Result<bool> {
        Ok(Self::capture(root, frontend_enabled)?.is_some_and(|next| {
            self.roots == next.roots
                && self.files == next.files
                && self.configurations == next.configurations
        }))
    }

    pub fn agrees_with_before_build(&self, before: &Self) -> bool {
        // First-build Cargo discovery may add watched dependency directories.
        // Every previously observed root must still have the exact membership
        // and bytes acquired before compilation; later edits cannot become the
        // reuse baseline for an older executable.
        before.roots.iter().all(|root| {
            before
                .files
                .iter()
                .filter(|(path, _)| path.starts_with(root))
                .eq(self.files.iter().filter(|(path, _)| path.starts_with(root)))
        }) && self.configurations == before.configurations
    }

    /// Membership, dependency files and configuration must stay unchanged.
    /// A watch event alone cannot authorize reuse of a compiled Host.
    pub fn implementation_edits(
        &self,
        next: &Self,
        current: &Path,
        root: &Path,
    ) -> anyhow::Result<Option<Vec<PathBuf>>> {
        if self.roots != next.roots
            || self.configurations != next.configurations
            || !self.files.keys().eq(next.files.keys())
        {
            return Ok(None);
        }
        for path in &self.configurations {
            if fs::read(path)? != fs::read(current.join(path.strip_prefix(root)?))? {
                return Ok(None);
            }
        }
        let changed: Vec<_> = self
            .files
            .iter()
            .filter(|(path, digest)| next.files.get(*path) != Some(*digest))
            .map(|(path, _)| path.clone())
            .collect();
        if changed.is_empty()
            || changed.iter().any(|path| {
                !path.extension().is_some_and(|ext| {
                    matches!(ext.to_str(), Some("ts" | "tsx" | "js" | "jsx" | "mjs"))
                })
            })
        {
            return Ok(None);
        }
        Ok(Some(changed))
    }
}

/// Reuse immutable execution packaging, but always create a fresh Generation.
/// No Kernel or native Plugin factory is patched while the old Host is live.
pub(super) fn configuration_candidate(
    root: &Path,
    current: &Path,
    output: &Path,
    _batch: &Batch,
    inputs: &Inputs,
    frontend_enabled: bool,
) -> anyhow::Result<bool> {
    if !inputs.matches(root, frontend_enabled)? {
        return Ok(false);
    }
    // Copy declared packaging only, never runtime-created databases, activation
    // state or retirement receipts. Existing preflight still verifies the lock.
    copy_execution_packaging(current, output)?;
    for path in &inputs.configurations {
        let relative = path.strip_prefix(root)?;
        let bytes = fs::read(path)?;
        for base in [output.to_path_buf(), output.join("intent")] {
            let destination = base.join(relative);
            anyhow::ensure!(
                destination.is_file(),
                "configuration is absent from retained Host intent"
            );
            fs::write(destination, &bytes)?;
        }
    }
    let previous = lenso_app_authoring::load_resolved_app(current)?;
    let candidate = lenso_app_authoring::load_resolved_app(output)?;
    let mut before = serde_json::to_value(previous.plan())?;
    let mut after = serde_json::to_value(candidate.plan())?;
    for plan in [&mut before, &mut after] {
        for instance in plan["plugin_instances"]
            .as_array_mut()
            .context("resolved Plan Instances")?
        {
            instance
                .as_object_mut()
                .context("resolved Instance")?
                .remove("configuration");
        }
    }
    // Topology, contracts, placement, grants and lifecycle stay structural.
    if before != after {
        fs::remove_dir_all(output)?;
        return Ok(false);
    }
    anyhow::ensure!(
        inputs.matches(root, frontend_enabled)?,
        "App implementation inputs changed during configuration resolution"
    );
    for path in &inputs.configurations {
        anyhow::ensure!(
            fs::read(path)? == fs::read(output.join(path.strip_prefix(root)?))?,
            "configuration changed during resolution"
        );
    }
    Ok(true)
}

pub(super) fn copy_execution_packaging(current: &Path, output: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(!output.exists(), "configuration candidate already exists");
    fs::create_dir(output)?;
    let lock_path = ".lenso/distribution.lock.json";
    let lock: serde_json::Value = serde_json::from_slice(&fs::read(current.join(lock_path))?)?;
    anyhow::ensure!(
        matches!(
            lock["schema"].as_str(),
            Some("lenso.local-host-distribution.v1" | "lenso.host-distribution.v1")
        ),
        "unsupported execution packaging lock"
    );
    let mut paths = BTreeSet::from([lock_path.to_owned()]);
    for file in lock["files"]
        .as_array()
        .context("execution packaging files")?
    {
        paths.insert(
            file["path"]
                .as_str()
                .context("execution packaging path")?
                .to_owned(),
        );
    }
    if current.join("resources.json").is_file() {
        paths.insert("resources.json".into());
        let resources: serde_json::Value =
            serde_json::from_slice(&fs::read(current.join("resources.json"))?)?;
        for resource in resources["resources"]
            .as_array()
            .context("execution resources")?
        {
            paths.insert(
                resource["path"]
                    .as_str()
                    .context("execution resource path")?
                    .to_owned(),
            );
        }
    }
    anyhow::ensure!(
        paths.len() <= 4096,
        "execution packaging exceeds 4096 files"
    );
    for path in paths {
        let relative = Path::new(&path);
        anyhow::ensure!(
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
            "execution packaging path is not relative"
        );
        let mut source = current.to_path_buf();
        for component in relative.components() {
            source.push(component);
            anyhow::ensure!(
                !fs::symlink_metadata(&source)?.file_type().is_symlink(),
                "execution packaging cannot traverse symlinks"
            );
        }
        anyhow::ensure!(
            fs::symlink_metadata(&source)?.is_file(),
            "execution artifact is not a regular file"
        );
        let destination = output.join(relative);
        fs::create_dir_all(destination.parent().context("execution artifact parent")?)?;
        fs::copy(source, destination)?;
    }
    for (source, destination) in [
        (current.join("plugins"), output.join("plugins")),
        (
            current.join("intent/plugins"),
            output.join("intent/plugins"),
        ),
    ] {
        if source.exists() {
            fs::create_dir_all(destination.parent().context("intent parent")?)?;
            super::super::assemble::copy_root(&source, &destination, 0, &mut 0)?;
        }
    }
    fs::create_dir_all(output.join("intent/.lenso"))?;
    for base in [output.to_path_buf(), output.join("intent")] {
        fs::create_dir_all(base.join(".lenso"))?;
        fs::write(base.join(".lenso/plugin-root-authoring.lock"), [])?;
    }
    if output.join(".lenso/host-build.json").exists() {
        fs::copy(
            output.join(".lenso/host-build.json"),
            output.join("intent/.lenso/host-build.json"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
