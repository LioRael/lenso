//! Optional Capability authoring. Discovery is replaceable; the official
//! generator, compatibility checks and output admission remain authoritative.
mod baselines;
mod discovery;
mod output;
pub use baselines::accept;
pub use discovery::{
    DiscoveryOptions, ProjectionTarget, discover, discover_in, snapshot_contract,
    snapshot_contract_in,
};
pub use output::{Mode, apply, run};

use anyhow::{Context, bail, ensure};
use lenso_contract_codegen::{generate_module_projection, generate_projection, lint_compatibility};
use lenso_engine::{ContextView, Plugin, Resource, Snapshot, Step};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// Exact official generator target. Language and runtime are independent choices.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectionKind {
    Rust,
    RustRuntime,
    RustPlugin,
    #[serde(rename = "typescript")]
    TypeScript,
    Wit,
}
impl ProjectionKind {
    fn language(self) -> lenso_contract_codegen::ProjectionLanguage {
        use lenso_contract_codegen::ProjectionLanguage as L;
        match self {
            Self::Rust => L::Rust,
            Self::RustRuntime => L::RustRuntime,
            Self::RustPlugin => L::RustPlugin,
            Self::TypeScript => L::TypeScript,
            Self::Wit => L::Wit,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    pub kind: ProjectionKind,
    /// Consumer-local relative output, independent of the source location.
    pub output: String,
    #[serde(default)]
    pub module: Option<String>,
}

/// A selected immutable input. Custom discovery supplies these and a Snapshot;
/// no package-provided program runs to discover or generate a dependency contract.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractInput {
    /// Diagnostic provenance, e.g. an exact Cargo package ID/checksum or git SHA.
    pub identity: String,
    /// Descriptor path in the Snapshot, never an ambient source-machine path.
    pub descriptor: String,
    pub projections: Vec<Projection>,
    /// Previous accepted descriptor, with its schemas, in the same Snapshot.
    #[serde(default)]
    pub baseline: Option<String>,
}

#[derive(Debug)]
pub struct ContractAuthoring {
    inputs: Vec<ContractInput>,
    identity: String,
}
impl ContractAuthoring {
    /// Replaces default discovery entirely while retaining validation/lowering.
    pub fn from_inputs(inputs: Vec<ContractInput>) -> anyhow::Result<Self> {
        let identity = format!(
            "lenso.contracts.v1/support-{}-generator-{}-revision-{}/{}",
            env!("CARGO_PKG_VERSION"),
            lenso_contract_codegen::GENERATOR_VERSION,
            lenso_contract_codegen::GENERATOR_REVISION,
            digest(&serde_json::to_vec(&inputs)?)
        );
        Ok(Self { inputs, identity })
    }
}
impl Plugin for ContractAuthoring {
    fn identity(&self) -> &str {
        &self.identity
    }
    fn cacheable(&self) -> bool {
        true
    }
    fn plan(&self, snapshot: &Snapshot) -> anyhow::Result<Vec<Step>> {
        ensure!(self.inputs.len() <= 256, "at most 256 selected contracts");
        let mut outputs = std::collections::BTreeSet::new();
        let mut identities = std::collections::BTreeSet::new();
        self.inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                ensure!(
                    !input.identity.is_empty() && identities.insert(&input.identity),
                    "empty or duplicate contract identity: {}",
                    input.identity
                );
                relative(&input.descriptor)?;
                ensure!(
                    snapshot.files().contains_key(&input.descriptor),
                    "{}: missing descriptor {}",
                    input.identity,
                    input.descriptor
                );
                ensure!(
                    !input.projections.is_empty(),
                    "{}: select at least one projection",
                    input.identity
                );
                for projection in &input.projections {
                    relative(&projection.output)?;
                    ensure!(
                        !snapshot.files().contains_key(&projection.output),
                        "projection would overwrite an input: {}",
                        projection.output
                    );
                    ensure!(
                        outputs.insert(&projection.output),
                        "multiple owners of output {}",
                        projection.output
                    );
                }
                if let Some(baseline) = &input.baseline {
                    relative(baseline)?;
                    ensure!(
                        snapshot.files().contains_key(baseline),
                        "missing compatibility baseline {baseline}"
                    );
                }
                // All supplied bytes are declared so cross-file Schema refs and
                // compatibility baselines participate in deterministic invalidation.
                Ok(Step {
                    id: format!("contracts/{index}"),
                    inputs: snapshot.files().keys().cloned().collect(),
                    after: vec![],
                    options: serde_json::to_value(input)?,
                })
            })
            .collect()
    }
    fn process(&self, context: &ContextView<'_>) -> anyhow::Result<BTreeMap<String, Resource>> {
        let input: ContractInput = serde_json::from_value(context.step.options.clone())?;
        let stage = tempfile::tempdir()?;
        for (path, bytes) in &context.files {
            relative(path)?;
            let target = stage.path().join(path);
            fs::create_dir_all(target.parent().context("input parent")?)?;
            fs::write(target, bytes)?;
        }
        let descriptor = stage.path().join(&input.descriptor);
        if let Some(previous) = &input.baseline {
            let mut accepted = Snapshot::default();
            let mut current = Snapshot::default();
            snapshot_contract(
                stage.path(),
                previous,
                "contract/capability.json",
                &mut accepted,
            )?;
            snapshot_contract(
                stage.path(),
                &input.descriptor,
                "contract/capability.json",
                &mut current,
            )?;
            if accepted.files() != current.files() {
                lint_compatibility(&stage.path().join(previous), &descriptor)
                    .with_context(|| format!("{}: incompatible contract change", input.identity))?;
            }
        }
        input
            .projections
            .iter()
            .map(|projection| {
                let generated = if let Some(module) = &projection.module {
                    generate_module_projection(&descriptor, projection.kind.language(), module)
                } else {
                    generate_projection(&descriptor, projection.kind.language())
                }
                .with_context(|| format!("{}: generate {}", input.identity, projection.output))?;
                Ok((
                    projection.output.clone(),
                    Resource::file(projection.output.clone(), generated.source.into_bytes())?,
                ))
            })
            .collect()
    }
}

pub(crate) fn relative(path: &str) -> anyhow::Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':'])
        || Path::new(path).is_absolute()
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        bail!("expected contained relative path: {path}");
    }
    Ok(())
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(output, "{byte:02x}").expect("String write");
    }
    output
}
