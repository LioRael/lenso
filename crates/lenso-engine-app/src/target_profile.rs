//! Concrete Host/Adapter target capability evidence.
//!
//! The Engine does not infer a target profile from a CLI flag.  Each Host path
//! declares the interaction surface it actually wires to an Adapter, then uses
//! the runtime-owned canonical wire type for implementation selection.

use anyhow::{Context, bail};
use lenso_app_plan::ExecutionClassId;
use lenso_plugin_bundle::{
    ExecutionTargetCapabilities, ExecutionTargetCapability, ExecutionTargetCapabilityProfile,
    ImplementationPolicy, PluginManifest, RejectedPluginImplementation,
    ResolvedPluginImplementation, RuntimeAdmission, explain_implementation,
};
use serde::{Deserialize, Serialize};

/// Persisted result of the runtime resolver's one implementation decision.
///
/// This is a projection of `lenso_plugin_bundle::explain_implementation`, not
/// a second implementation resolver. It deliberately retains rejections even
/// when an earlier Host-preferred candidate was selected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImplementationSelectionEvidence {
    pub(crate) selected: SelectedImplementationEvidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) rejected: Vec<RejectedPluginImplementation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedImplementationEvidence {
    pub(crate) implementation_id: String,
    pub(crate) execution_class: ExecutionClassId,
    pub(crate) runtime_profile: String,
}

/// One fully explained selection plus the exact capability profile consumed by
/// the resolver.
#[derive(Clone, Debug)]
pub(crate) struct SelectedImplementation {
    pub(crate) implementation: ResolvedPluginImplementation,
    pub(crate) target_capability_profile: ExecutionTargetCapabilityProfile,
    pub(crate) evidence: ImplementationSelectionEvidence,
}

/// Admits a concrete child-process target that wires request dispatch and owns
/// the child process lifecycle.
pub(crate) fn request_native_process_admission(
    execution_class: ExecutionClassId,
    runtime_profile: impl Into<String>,
) -> RuntimeAdmission {
    RuntimeAdmission::new(
        execution_class,
        runtime_profile,
        ExecutionTargetCapabilities::new([
            ExecutionTargetCapability::Request,
            ExecutionTargetCapability::NativeProcess,
        ]),
    )
}

/// Admits a concrete Wasm Component target that wires request dispatch and
/// owns Component execution.
pub(crate) fn request_wasm_component_admission(
    execution_class: ExecutionClassId,
    runtime_profile: impl Into<String>,
) -> RuntimeAdmission {
    RuntimeAdmission::new(
        execution_class,
        runtime_profile,
        ExecutionTargetCapabilities::new([
            ExecutionTargetCapability::Request,
            ExecutionTargetCapability::WasmComponent,
        ]),
    )
}

/// The TypeScript Host uses the Bun Adapter's owned profile identity while
/// declaring the two mechanisms it actually wires: Request dispatch and the
/// trusted Bun child-process lifecycle. It does not advertise other Adapter
/// facilities until this Host owns and qualifies them end to end.
pub(crate) fn bun_admission() -> anyhow::Result<RuntimeAdmission> {
    let adapter_profile = lenso_bun_adapter::bun_authoring_target_capability_profile();
    adapter_profile
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .context("validate Bun Adapter target capability profile")?;
    if adapter_profile.target_profile != lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE
        || !adapter_profile.supports(ExecutionTargetCapability::Request)
        || !adapter_profile.supports(ExecutionTargetCapability::NativeProcess)
    {
        bail!("Bun Adapter did not expose its required Request and NativeProcess target profile");
    }
    Ok(request_native_process_admission(
        ExecutionClassId::bun_child_process(),
        adapter_profile.target_profile,
    ))
}

/// Reconstructs one admission from durable, canonical Host evidence.
///
/// Prepared distributions use this rather than reconstructing a profile from
/// a CLI option or a hard-coded JSON document.
pub(crate) fn admission_from_profile(
    execution_class: ExecutionClassId,
    runtime_profile: &str,
    profile: &ExecutionTargetCapabilityProfile,
) -> anyhow::Result<RuntimeAdmission> {
    profile
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .context("validate persisted target capability profile")?;
    if profile.target_profile != runtime_profile {
        bail!(
            "target capability profile `{}` does not match runtime profile `{runtime_profile}`",
            profile.target_profile
        );
    }
    Ok(RuntimeAdmission::new(
        execution_class,
        runtime_profile,
        ExecutionTargetCapabilities::new(profile.capabilities.iter().copied()),
    ))
}

/// Uses the runtime-owned explanation exactly once and records the selected
/// Host admission profile alongside its rejected alternatives.
pub(crate) fn select_implementation(
    manifest: &PluginManifest,
    policy: &ImplementationPolicy,
) -> anyhow::Result<SelectedImplementation> {
    let explanation = explain_implementation(manifest, policy)
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .context("explain Plugin implementation selection")?;
    let implementation = explanation.selected.ok_or_else(|| {
        anyhow::anyhow!(
            "no Plugin implementation is admitted by the Host policy: {}",
            serde_json::to_string(&explanation.rejected)
                .unwrap_or_else(|_| "unserializable rejection evidence".to_owned())
        )
    })?;
    let admission = policy
        .runtimes
        .iter()
        .find(|admission| {
            admission.execution_class == *implementation.descriptor.execution_class()
                && admission.runtime_profile == implementation.descriptor.runtime_profile()
        })
        .context("selected Plugin implementation is missing its Host admission")?;
    let target_capability_profile = admission.capability_profile();
    target_capability_profile
        .validate()
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .context("validate selected target capability profile")?;
    let evidence = ImplementationSelectionEvidence {
        selected: SelectedImplementationEvidence {
            implementation_id: implementation.implementation_id.clone(),
            execution_class: implementation.descriptor.execution_class().clone(),
            runtime_profile: implementation.descriptor.runtime_profile().to_owned(),
        },
        rejected: explanation.rejected,
    };
    Ok(SelectedImplementation {
        implementation,
        target_capability_profile,
        evidence,
    })
}
