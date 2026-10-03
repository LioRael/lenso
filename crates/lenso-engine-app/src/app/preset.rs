//! The complete local App workflow is an optional Engine processor.
use lenso_engine::{ContextView, Plugin, Resource, Snapshot, Step};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug)]
pub struct AppProject {
    pub root: PathBuf,
    pub output: PathBuf,
    pub runtime_executable: PathBuf,
    pub trust_linked_build: Vec<String>,
    pub portable_implementations: Vec<String>,
    pub host_many_slots: Vec<String>,
}
impl Plugin for AppProject {
    fn identity(&self) -> &str {
        "lenso.app.v1"
    }
    fn plan(&self, _: &Snapshot) -> anyhow::Result<Vec<Step>> {
        self.plan_in(&lenso_engine::discovery::DiscoverySession::new(&self.root)?)
    }
    fn process(&self, context: &ContextView<'_>) -> anyhow::Result<BTreeMap<String, Resource>> {
        self.process_in(
            context,
            &lenso_engine::discovery::DiscoverySession::new(&self.root)?,
        )
    }
}

impl AppProject {
    /// Prepare one shared acquisition epoch for the official App pipeline.
    pub fn prepare(self) -> anyhow::Result<PreparedAppProject> {
        super::assemble::parse_host_many_slots(&self.host_many_slots)?;
        let inputs =
            std::sync::Mutex::new(lenso_engine::discovery::DiscoverySession::new(&self.root)?);
        Ok(PreparedAppProject {
            project: self,
            inputs,
        })
    }

    fn plan_in(
        &self,
        inputs: &lenso_engine::discovery::DiscoverySession,
    ) -> anyhow::Result<Vec<Step>> {
        let report = lenso_app_authoring::discovery::discover_in(&self.root, inputs)?;
        let root = &report.root;
        super::convention_authoring::linked_catalog::verify_sources(root, &report.candidates)?;
        // Domain-specific source inspection belongs to this optional preset.
        let conventions = lenso_app_authoring::discovery::conventions::plan_in(&report, inputs)?;
        let mut fingerprints = BTreeMap::new();
        fingerprints.insert(
            root.clone(),
            super::local_host::input_digest_in(root, inputs)?,
        );
        for compilation in &conventions.compilations {
            for root in [&compilation.owner_project, &compilation.compiler_project] {
                fingerprints.insert(
                    root.clone(),
                    super::local_host::input_digest_in(root, inputs)?,
                );
            }
        }
        let dependency_locks = super::local_host::dependency_lock_digests(
            std::iter::once(root.as_path())
                .chain(
                    report
                        .candidates
                        .iter()
                        .map(|candidate| candidate.project.as_path()),
                )
                .chain(conventions.compilations.iter().flat_map(|compilation| {
                    [
                        compilation.owner_project.as_path(),
                        compilation.compiler_project.as_path(),
                    ]
                })),
        )?;
        Ok(vec![Step {
            id: "app/build".into(),
            inputs: vec![],
            after: vec![],
            options: serde_json::json!({"root":root,"discovery":report,"output":self.output,"conventions":conventions,"runtime_executable":self.runtime_executable,"fingerprints":fingerprints,"dependency_locks":dependency_locks,"portable_implementations":self.portable_implementations,"host_many_slots":self.host_many_slots}),
        }])
    }
    fn process_in(
        &self,
        context: &ContextView<'_>,
        inputs: &lenso_engine::discovery::DiscoverySession,
    ) -> anyhow::Result<BTreeMap<String, Resource>> {
        if context.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            anyhow::bail!("App build cancelled");
        }
        let fingerprints: BTreeMap<PathBuf, String> =
            serde_json::from_value(context.step.options["fingerprints"].clone())?;
        let verification = lenso_engine::discovery::DiscoverySession::new(&self.root)?;
        for (root, expected) in fingerprints {
            if super::local_host::input_digest_in(&root, &verification)? != expected {
                anyhow::bail!("App inputs changed after planning; replan before execution");
            }
        }
        let dependency_locks: BTreeMap<PathBuf, String> =
            serde_json::from_value(context.step.options["dependency_locks"].clone())?;
        super::local_host::verify_dependency_lock_digests(&dependency_locks)?;
        let _cancellation = CancellationGuard::enter(context.cancelled.clone());
        let _runtime = RuntimeGuard::enter(self.runtime_executable.clone());
        let info = super::build_command(&self.runtime_executable)
            .arg("--engine-host-info")
            .output()?;
        if !info.status.success() {
            anyhow::bail!("precompiled Engine Host rejected its protocol probe");
        }
        let info: serde_json::Value = serde_json::from_slice(&info.stdout)?;
        if info["schema"] != "lenso.engine-host.v1"
            || info["target"] != lenso_app_authoring::native_host_target()
        {
            anyhow::bail!("incompatible precompiled Engine Host");
        }
        super::assemble::assemble_in(
            super::assemble::AssembleArgs {
                root: Some(self.root.clone()),
                id: "local.app".into(),
                out: self.output.clone(),
                json: false,
                executable: true,
                trust_linked_build: self.trust_linked_build.clone(),
                portable_implementations: self.portable_implementations.clone(),
                host_many_slots: self.host_many_slots.clone(),
            },
            Some(serde_json::from_value(
                context.step.options["discovery"].clone(),
            )?),
            Some(inputs),
        )?;
        Ok(BTreeMap::from([(
            "distribution".into(),
            Resource {
                schema: "lenso.app-distribution.v1".into(),
                value: serde_json::json!({"directory":self.output}),
            },
        )]))
    }
}

/// Optional prepared preset; generic Engine registration remains explicit.
#[derive(Debug)]
pub struct PreparedAppProject {
    project: AppProject,
    inputs: std::sync::Mutex<lenso_engine::discovery::DiscoverySession>,
}

impl Plugin for PreparedAppProject {
    fn identity(&self) -> &str {
        self.project.identity()
    }
    fn plan(&self, _: &Snapshot) -> anyhow::Result<Vec<Step>> {
        let mut inputs = self
            .inputs
            .lock()
            .map_err(|_| anyhow::anyhow!("App acquisition poisoned"))?;
        inputs.begin_epoch();
        self.project.plan_in(&inputs)
    }
    fn process(&self, context: &ContextView<'_>) -> anyhow::Result<BTreeMap<String, Resource>> {
        let inputs = self
            .inputs
            .lock()
            .map_err(|_| anyhow::anyhow!("App acquisition poisoned"))?;
        self.project.process_in(context, &inputs)
    }
}

thread_local! {
    static CANCELLATION: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = const { std::cell::RefCell::new(None) };
}
struct CancellationGuard(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);
impl CancellationGuard {
    fn enter(token: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self(CANCELLATION.with(|slot| slot.replace(Some(token))))
    }
}
impl Drop for CancellationGuard {
    fn drop(&mut self) {
        CANCELLATION.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}
pub(super) fn cancellation() -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    CANCELLATION
        .with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))
}
pub(super) fn checkpoint() -> anyhow::Result<()> {
    if cancellation().load(std::sync::atomic::Ordering::SeqCst) {
        anyhow::bail!("App build cancelled before publication");
    }
    Ok(())
}

thread_local! { static RUNTIME: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) }; }
struct RuntimeGuard(Option<PathBuf>);
impl RuntimeGuard {
    fn enter(path: PathBuf) -> Self {
        Self(RUNTIME.with(|slot| slot.replace(Some(path))))
    }
}
impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        RUNTIME.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}
pub(super) fn runtime_executable() -> anyhow::Result<PathBuf> {
    RUNTIME
        .with(|slot| slot.borrow().clone())
        .map_or_else(|| Ok(std::env::current_exe()?), Ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_rejects_body_change_after_acquisition_before_fingerprint() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='race-fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        let source = "#[lenso::plugin(id=\"race.fixture\",root_slot=\"web\")]pub struct Http {} fn body()->u8{1}";
        std::fs::write(root.path().join("src/lib.rs"), source).unwrap();
        let inputs = lenso_engine::discovery::DiscoverySession::new(root.path()).unwrap();
        lenso_app_authoring::discovery::discover_in(root.path(), &inputs).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), source.replace("{1}", "{2}")).unwrap();
        let project = AppProject {
            root: root.path().into(),
            output: root.path().join("dist"),
            runtime_executable: root.path().join("must-not-execute"),
            trust_linked_build: vec![],
            portable_implementations: vec![],
            host_many_slots: vec![],
        };
        let steps = project.plan_in(&inputs).unwrap();
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let context = ContextView {
            step: &steps[0],
            files: BTreeMap::new(),
            dependencies: BTreeMap::new(),
            cancelled: &cancelled,
        };
        let error = project.process_in(&context, &inputs).unwrap_err();
        assert!(
            error.to_string().contains("inputs changed after planning"),
            "{error:#}"
        );
    }
}
