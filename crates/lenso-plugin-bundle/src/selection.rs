use std::collections::{BTreeMap, BTreeSet};

use lenso_app_plan::{
    CapabilityOperationKind, ExecutionClassId,
    ExecutionTargetCapability as PlanExecutionTargetCapability,
    authoring::{PluginContract, PluginDescriptor, PluginImplementation},
};
pub use lenso_process_protocol::{
    EXECUTION_TARGET_CAPABILITY_PROFILE as EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT,
    ExecutionTargetCapability, ExecutionTargetCapabilityProfile,
};
use serde::{Deserialize, Serialize};

use crate::{BundleError, PluginArtifactV2, PluginManifest};

/// An explicit, fail-closed capability profile for one admitted target runtime.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExecutionTargetCapabilities(BTreeSet<ExecutionTargetCapability>);

impl ExecutionTargetCapabilities {
    /// Creates a profile from exact target-owned feature evidence.
    pub fn new(features: impl IntoIterator<Item = ExecutionTargetCapability>) -> Self {
        Self(features.into_iter().collect())
    }

    /// Creates a profile that intentionally admits no optional target feature.
    pub fn none() -> Self {
        Self::default()
    }

    /// Returns whether this profile explicitly supports one feature.
    pub fn supports(&self, feature: ExecutionTargetCapability) -> bool {
        self.0.contains(&feature)
    }

    /// Returns unsupported features in stable order.
    pub fn missing(
        &self,
        required: impl IntoIterator<Item = ExecutionTargetCapability>,
    ) -> Vec<ExecutionTargetCapability> {
        required
            .into_iter()
            .filter(|feature| !self.supports(*feature))
            .collect()
    }

    /// Returns the exact feature declarations in stable order.
    pub fn features(&self) -> impl ExactSizeIterator<Item = ExecutionTargetCapability> + '_ {
        self.0.iter().copied()
    }

    /// Exports the profile in its canonical protocol wire shape.
    pub fn profile_for(
        &self,
        target_profile: impl Into<String>,
    ) -> ExecutionTargetCapabilityProfile {
        let mut capabilities = self.features().collect::<Vec<_>>();
        capabilities.sort_unstable_by_key(|capability| capability.as_str());
        ExecutionTargetCapabilityProfile {
            profile: EXECUTION_TARGET_CAPABILITY_PROFILE_CONTRACT.to_owned(),
            target_profile: target_profile.into(),
            capabilities,
        }
    }
}

/// One operation the selected implementation needs its target to support.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TargetCapabilityRequirement {
    pub capability_id: String,
    pub operation: String,
    pub feature: ExecutionTargetCapability,
}

/// A machine-readable reason why one candidate cannot be selected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImplementationRejectionReason {
    HostTargetMismatch {
        host_target: String,
        candidate_targets: Vec<String>,
    },
    RuntimeNotAdmitted,
    MissingTargetCapabilities {
        requirements: Vec<TargetCapabilityRequirement>,
    },
    InvalidTargetCapabilityRequirement {
        feature: String,
    },
    InvalidTargetCapabilityProfile {
        profile: ExecutionTargetCapabilityProfile,
    },
    AmbiguousRuntimeAdmission {
        matching_implementation_ids: Vec<String>,
    },
}

/// A rejected candidate from an implementation-resolution explanation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RejectedPluginImplementation {
    pub implementation_id: String,
    pub execution_class: ExecutionClassId,
    pub runtime_profile: String,
    pub reason: ImplementationRejectionReason,
}

/// The deterministic answer to one Host's implementation selection attempt.
///
/// Hosts and CLIs can serialize this value to explain a rejection without
/// reverse-engineering the resolver. `resolve_implementation` remains the
/// compatibility entry point for callers that only need a selected artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationSelectionExplanation {
    pub selected: Option<ResolvedPluginImplementation>,
    pub rejected: Vec<RejectedPluginImplementation>,
}

impl ImplementationSelectionExplanation {
    /// Returns whether exactly one implementation was admitted.
    pub const fn is_selected(&self) -> bool {
        self.selected.is_some()
    }

    fn failure_detail(&self, schema: &str) -> String {
        let detail = self
            .rejected
            .iter()
            .map(render_rejection)
            .collect::<Vec<_>>()
            .join("; ");
        if detail.is_empty() {
            format!("{schema} Bundle has no implementation admitted by Host policy")
        } else {
            format!("{schema} Bundle has no implementation admitted by Host policy: {detail}")
        }
    }
}

/// One exact runtime protocol admitted by the Host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeAdmission {
    pub execution_class: ExecutionClassId,
    pub runtime_profile: String,
    pub capabilities: ExecutionTargetCapabilities,
}

impl RuntimeAdmission {
    /// Creates an explicit Host admission. An empty profile remains fail closed.
    pub fn new(
        execution_class: ExecutionClassId,
        runtime_profile: impl Into<String>,
        capabilities: ExecutionTargetCapabilities,
    ) -> Self {
        Self {
            execution_class,
            runtime_profile: runtime_profile.into(),
            capabilities,
        }
    }

    /// Returns the exact portable profile this Host is admitting.
    pub fn capability_profile(&self) -> ExecutionTargetCapabilityProfile {
        self.capabilities.profile_for(&self.runtime_profile)
    }
}

/// Host policy used to resolve one implementation before Plan construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationPolicy {
    pub host_target: String,
    pub runtimes: Vec<RuntimeAdmission>,
}

/// One final implementation selected from a Plugin Release.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPluginImplementation {
    pub implementation_id: String,
    pub descriptor: PluginDescriptor,
    pub artifact: PluginArtifactV2,
}

/// Selects one implementation deterministically. Selection never implies runtime fallback.
pub fn resolve_implementation(
    manifest: &PluginManifest,
    policy: &ImplementationPolicy,
) -> Result<ResolvedPluginImplementation, BundleError> {
    let schema = match manifest {
        PluginManifest::V2(_) => "V2",
        PluginManifest::V3(_) => "V3",
        PluginManifest::V4(_) => "V4",
    };
    let explanation = explain_implementation(manifest, policy)?;
    let failure_detail = explanation.failure_detail(schema);
    explanation
        .selected
        .ok_or(BundleError::InvalidBundle(failure_detail))
}

/// Explains a Host's exact implementation choice or all rejected candidates.
///
/// A missing feature remains a rejection even if another implementation can be
/// selected. This lets a CLI show the reason for a fallback while preserving the
/// Host's declared runtime priority order.
pub fn explain_implementation(
    manifest: &PluginManifest,
    policy: &ImplementationPolicy,
) -> Result<ImplementationSelectionExplanation, BundleError> {
    match manifest {
        PluginManifest::V2(value) => {
            let descriptor =
                serde_json::from_value::<PluginDescriptor>(value.entry.descriptor.clone())
                    .map_err(|error| BundleError::InvalidManifest(error.to_string()))?;
            let candidate = Candidate {
                implementation_id: "default".to_owned(),
                host_targets: vec![value.artifact.target.clone()],
                artifact: value.artifact.clone(),
                descriptor,
                artifact_matches_wasm: value.artifact.media_type == "application/wasm",
            };
            Ok(explain_candidates(&[candidate], policy))
        }
        PluginManifest::V3(value) => Ok(explain_profiled_implementation(
            &value.contract,
            value.implementations.iter().map(|candidate| {
                (
                    &candidate.id,
                    &candidate.host_targets,
                    &candidate.artifact,
                    &candidate.runtime,
                )
            }),
            policy,
        )),
        PluginManifest::V4(value) => Ok(explain_profiled_implementation(
            &value.contract,
            value.implementations.iter().map(|candidate| {
                (
                    &candidate.id,
                    &candidate.host_targets,
                    &candidate.artifact,
                    &candidate.runtime,
                )
            }),
            policy,
        )),
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    implementation_id: String,
    host_targets: Vec<String>,
    artifact: PluginArtifactV2,
    descriptor: PluginDescriptor,
    artifact_matches_wasm: bool,
}

fn explain_profiled_implementation<'a>(
    contract: &PluginContract,
    candidates: impl Iterator<
        Item = (
            &'a String,
            &'a Vec<String>,
            &'a PluginArtifactV2,
            &'a PluginImplementation,
        ),
    >,
    policy: &ImplementationPolicy,
) -> ImplementationSelectionExplanation {
    let candidates = candidates
        .map(|(id, targets, artifact, runtime)| Candidate {
            implementation_id: id.clone(),
            host_targets: targets.clone(),
            artifact: artifact.clone(),
            descriptor: contract.resolve(runtime),
            artifact_matches_wasm: false,
        })
        .collect::<Vec<_>>();
    explain_candidates(&candidates, policy)
}

fn explain_candidates(
    candidates: &[Candidate],
    policy: &ImplementationPolicy,
) -> ImplementationSelectionExplanation {
    let mut rejected = Vec::new();

    for admission in &policy.runtimes {
        let compatible = candidates_for_admission(candidates, policy, admission, &mut rejected);

        match compatible.as_slice() {
            [] => {}
            [candidate] => {
                return ImplementationSelectionExplanation {
                    selected: Some(ResolvedPluginImplementation {
                        implementation_id: candidate.implementation_id.clone(),
                        descriptor: candidate.descriptor.clone(),
                        artifact: candidate.artifact.clone(),
                    }),
                    rejected,
                };
            }
            matches => {
                rejected.push(RejectedPluginImplementation {
                    implementation_id: "<host-policy>".to_owned(),
                    execution_class: admission.execution_class.clone(),
                    runtime_profile: admission.runtime_profile.clone(),
                    reason: ImplementationRejectionReason::AmbiguousRuntimeAdmission {
                        matching_implementation_ids: matches
                            .iter()
                            .map(|candidate| candidate.implementation_id.clone())
                            .collect(),
                    },
                });
                return ImplementationSelectionExplanation {
                    selected: None,
                    rejected,
                };
            }
        }
    }
    record_unadmitted_runtimes(candidates, policy, &mut rejected);
    ImplementationSelectionExplanation {
        selected: None,
        rejected,
    }
}

fn candidates_for_admission<'a>(
    candidates: &'a [Candidate],
    policy: &ImplementationPolicy,
    admission: &RuntimeAdmission,
    rejected: &mut Vec<RejectedPluginImplementation>,
) -> Vec<&'a Candidate> {
    let mut compatible = Vec::new();
    let capability_profile = admission.capability_profile();
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate_matches_admission(candidate, admission))
    {
        if capability_profile.validate().is_err() {
            rejected.push(rejected_candidate(
                candidate,
                ImplementationRejectionReason::InvalidTargetCapabilityProfile {
                    profile: capability_profile.clone(),
                },
            ));
            continue;
        }
        if record_target_match(candidate, policy, rejected)
            && record_capability_match(candidate, admission, rejected)
        {
            compatible.push(candidate);
        }
    }
    compatible
}

fn candidate_matches_admission(candidate: &Candidate, admission: &RuntimeAdmission) -> bool {
    candidate.descriptor.execution_class() == &admission.execution_class
        && candidate.descriptor.runtime_profile() == admission.runtime_profile
}

fn record_target_match(
    candidate: &Candidate,
    policy: &ImplementationPolicy,
    rejected: &mut Vec<RejectedPluginImplementation>,
) -> bool {
    if candidate.artifact_matches_wasm
        || candidate
            .host_targets
            .iter()
            .any(|target| target == "*" || target == &policy.host_target)
    {
        return true;
    }
    rejected.push(rejected_candidate(
        candidate,
        ImplementationRejectionReason::HostTargetMismatch {
            host_target: policy.host_target.clone(),
            candidate_targets: candidate.host_targets.clone(),
        },
    ));
    false
}

fn record_capability_match(
    candidate: &Candidate,
    admission: &RuntimeAdmission,
    rejected: &mut Vec<RejectedPluginImplementation>,
) -> bool {
    let requirements = match target_requirements(&candidate.descriptor) {
        Ok(requirements) => requirements,
        Err(feature) => {
            rejected.push(rejected_candidate(
                candidate,
                ImplementationRejectionReason::InvalidTargetCapabilityRequirement { feature },
            ));
            return false;
        }
    };
    let missing = admission
        .capabilities
        .missing(requirements.iter().map(|requirement| requirement.feature));
    if missing.is_empty() {
        return true;
    }
    let requirements = requirements
        .into_iter()
        .filter(|requirement| missing.contains(&requirement.feature))
        .collect();
    rejected.push(rejected_candidate(
        candidate,
        ImplementationRejectionReason::MissingTargetCapabilities { requirements },
    ));
    false
}

fn record_unadmitted_runtimes(
    candidates: &[Candidate],
    policy: &ImplementationPolicy,
    rejected: &mut Vec<RejectedPluginImplementation>,
) {
    for candidate in candidates {
        if !policy
            .runtimes
            .iter()
            .any(|admission| candidate_matches_admission(candidate, admission))
        {
            rejected.push(rejected_candidate(
                candidate,
                ImplementationRejectionReason::RuntimeNotAdmitted,
            ));
        }
    }
}

fn target_requirements(
    descriptor: &PluginDescriptor,
) -> Result<Vec<TargetCapabilityRequirement>, String> {
    // Explicit implementation requirements cover target mechanics that cannot
    // be inferred from a Capability operation (for example Workers, a native
    // process, or Host imports). Capability operation requirements remain
    // mandatory, so a consumer-only Stream cannot bypass admission either.
    // One feature is reported once, with an explicit implementation reason
    // taking precedence over a redundant endpoint-derived reason.
    let mut requirements = BTreeMap::new();
    for requirement in descriptor.required_target_capabilities() {
        let feature = protocol_capability(*requirement)?;
        requirements.insert(
            feature,
            TargetCapabilityRequirement {
                capability_id: descriptor.plugin_id().to_owned(),
                operation: "<implementation>".to_owned(),
                feature,
            },
        );
    }
    for endpoint in descriptor.provided_capabilities() {
        for operation in endpoint.operations() {
            let Some(kind) = endpoint.operation_kind(operation) else {
                continue;
            };
            let feature = capability_for_operation_kind(kind);
            requirements
                .entry(feature)
                .or_insert(TargetCapabilityRequirement {
                    capability_id: endpoint.capability_id().to_owned(),
                    operation: operation.clone(),
                    feature,
                });
        }
    }
    let mut requirements = requirements.into_values().collect::<Vec<_>>();
    requirements.sort_unstable_by_key(|requirement| requirement.feature.as_str());
    Ok(requirements)
}

fn protocol_capability(
    capability: PlanExecutionTargetCapability,
) -> Result<ExecutionTargetCapability, String> {
    ExecutionTargetCapability::from_name(capability.as_str())
        .ok_or_else(|| capability.as_str().to_owned())
}

const fn capability_for_operation_kind(kind: CapabilityOperationKind) -> ExecutionTargetCapability {
    match kind {
        CapabilityOperationKind::Request => ExecutionTargetCapability::Request,
        CapabilityOperationKind::Stream => ExecutionTargetCapability::Stream,
        CapabilityOperationKind::Event => ExecutionTargetCapability::Event,
    }
}

fn rejected_candidate(
    candidate: &Candidate,
    reason: ImplementationRejectionReason,
) -> RejectedPluginImplementation {
    RejectedPluginImplementation {
        implementation_id: candidate.implementation_id.clone(),
        execution_class: candidate.descriptor.execution_class().clone(),
        runtime_profile: candidate.descriptor.runtime_profile().to_owned(),
        reason,
    }
}

fn render_rejection(rejection: &RejectedPluginImplementation) -> String {
    let reason = match &rejection.reason {
        ImplementationRejectionReason::HostTargetMismatch { host_target, .. } => {
            format!("host target `{host_target}` does not match")
        }
        ImplementationRejectionReason::RuntimeNotAdmitted => "runtime is not admitted".to_owned(),
        ImplementationRejectionReason::MissingTargetCapabilities { requirements } => format!(
            "missing target capabilities {}",
            requirements
                .iter()
                .map(|requirement| {
                    if requirement.operation == "<implementation>" {
                        format!(
                            "`{}` required by implementation `{}`",
                            requirement.feature.as_str(),
                            requirement.capability_id
                        )
                    } else {
                        format!(
                            "`{}` for {}.{}",
                            requirement.feature.as_str(),
                            requirement.capability_id,
                            requirement.operation
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ImplementationRejectionReason::InvalidTargetCapabilityRequirement { feature } => {
            format!("unknown implementation target capability `{feature}`")
        }
        ImplementationRejectionReason::InvalidTargetCapabilityProfile { profile } => format!(
            "invalid target capability profile `{}` for `{}`",
            profile.profile, profile.target_profile
        ),
        ImplementationRejectionReason::AmbiguousRuntimeAdmission {
            matching_implementation_ids,
        } => format!(
            "ambiguous candidates {}",
            matching_implementation_ids.join(", ")
        ),
    };
    format!("{}: {reason}", rejection.implementation_id)
}
