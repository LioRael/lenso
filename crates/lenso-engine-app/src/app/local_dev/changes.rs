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
    Configuration,
    Rust,
    TypeScript,
    Generation,
}

pub(super) struct Batch {
    pub paths: BTreeSet<PathBuf>,
    pub observed: Instant,
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

fn copy_execution_packaging(current: &Path, output: &Path) -> anyhow::Result<()> {
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
mod tests {
    use super::*;
    fn event(path: &str) -> notify::Event {
        notify::Event::new(notify::EventKind::Any).add_path(PathBuf::from(path))
    }

    #[tokio::test]
    async fn debounce_keeps_backend_edits_after_a_frontend_event_and_reports_errors() {
        let (sender, mut receiver) = mpsc::channel(8);
        let mut batch = Batch::new(event("/app/frontend/page.tsx"));
        sender.send(Ok(event("/app/src/plugin.rs"))).await.unwrap();
        batch.collect(&mut receiver).await.unwrap();
        assert_eq!(batch.work(Path::new("/app"), true), Work::Rust);
        sender
            .send(Err(notify::Error::generic("watch failed")))
            .await
            .unwrap();
        assert!(batch.collect(&mut receiver).await.is_err());
    }

    #[test]
    fn intent_membership_dependencies_and_resources_are_structural() {
        let root = Path::new("/app");
        assert_eq!(
            Batch::new(event("/app/plugins/a/default.toml")).work(root, false),
            Work::Configuration
        );
        for path in [
            "/app/plugins/a/default.disabled",
            "/app/plugins/a/default/resource.toml",
            "/app/plugins/.dependencies.json",
            "/app/Cargo.toml",
        ] {
            assert_eq!(Batch::new(event(path)).work(root, true), Work::Generation);
        }
        assert_eq!(
            Batch::new(event("/app/frontend/page.tsx")).work(root, true),
            Work::Frontend
        );
        assert_eq!(
            Batch::new(event("/app/frontend/page.tsx")).work(root, false),
            Work::TypeScript
        );
    }

    fn configuration_fixture() -> (tempfile::TempDir, tempfile::TempDir, Inputs) {
        use lenso_app_plan::authoring::{
            HostCatalog, HostPluginRelease, HostSlot, PluginDescriptor,
        };
        let root = tempfile::tempdir().unwrap();
        let current = tempfile::tempdir().unwrap();
        for directory in [root.path(), current.path()] {
            fs::create_dir_all(directory.join("plugins/example.greeting")).unwrap();
            fs::write(
                directory.join("plugins/example.greeting/default.toml"),
                "message = 'before'\n",
            )
            .unwrap();
        }
        fs::create_dir_all(current.path().join("intent/plugins/example.greeting")).unwrap();
        fs::write(
            current
                .path()
                .join("intent/plugins/example.greeting/default.toml"),
            "message = 'before'\n",
        )
        .unwrap();
        fs::create_dir(current.path().join(".lenso")).unwrap();
        let descriptor = PluginDescriptor::new("example.greeting", "1.0.0", "web")
            .with_configuration_schema(serde_json::json!({"type":"object","properties":{"message":{"type":"string"}},"additionalProperties":false}));
        let catalog = HostCatalog::new(
            [HostSlot::many("web")],
            [HostPluginRelease::new(descriptor)],
            [],
        );
        fs::write(
            current.path().join(".lenso/host-catalog.json"),
            serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();
        fs::write(current.path().join(".lenso/distribution.lock.json"), serde_json::to_vec(&serde_json::json!({"schema":"lenso.local-host-distribution.v1","files":[{"path":".lenso/host-catalog.json"}]})).unwrap()).unwrap();
        let inputs = Inputs::capture(root.path(), false).unwrap().unwrap();
        (root, current, inputs)
    }

    #[test]
    fn configuration_reuse_resolves_new_values_without_mutating_previous_intent() {
        let (root, current, inputs) = configuration_fixture();
        let path = root.path().join("plugins/example.greeting/default.toml");
        fs::write(&path, "message = 'after'\n").unwrap();
        fs::write(
            current.path().join("runtime-created-database"),
            "must not cross into the next Generation",
        )
        .unwrap();
        let output = root.path().join(".lenso/candidate");
        fs::create_dir(root.path().join(".lenso")).unwrap();
        let batch = Batch::new(notify::Event::new(notify::EventKind::Any).add_path(path));
        assert!(
            configuration_candidate(root.path(), current.path(), &output, &batch, &inputs, false)
                .unwrap()
        );
        assert!(!output.join("runtime-created-database").exists());
        assert!(
            fs::read_to_string(output.join("intent/plugins/example.greeting/default.toml"))
                .unwrap()
                .contains("after")
        );
        assert!(
            fs::read_to_string(current.path().join("plugins/example.greeting/default.toml"))
                .unwrap()
                .contains("before")
        );
    }

    #[test]
    fn schema_rejection_retains_previous_intent_and_source_changes_force_a_build() {
        let (root, current, inputs) = configuration_fixture();
        let path = root.path().join("plugins/example.greeting/default.toml");
        fs::write(&path, "message = 42\n").unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        let output = root.path().join(".lenso/candidate");
        let batch = Batch::new(notify::Event::new(notify::EventKind::Any).add_path(path));
        assert!(
            configuration_candidate(root.path(), current.path(), &output, &batch, &inputs, false)
                .is_err()
        );
        assert!(
            fs::read_to_string(current.path().join("plugins/example.greeting/default.toml"))
                .unwrap()
                .contains("before")
        );
        fs::write(
            root.path().join("new-source.rs"),
            "// newly discovered implementation\n",
        )
        .unwrap();
        assert!(
            !configuration_candidate(
                root.path(),
                current.path(),
                &root.path().join(".lenso/next"),
                &batch,
                &inputs,
                false
            )
            .unwrap()
        );
        assert!(!root.path().join(".lenso/next").exists());
    }
}
