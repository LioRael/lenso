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

use crate::{
    BundleError, ExecutionAdmissionRequirementV6, PluginArtifactV2, PluginCargoBuildInputV6,
    PluginManifest, PluginVariantInputV6, RequiredPermissionV6,
};

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
    ArtifactTargetMismatch {
        host_target: String,
        artifact_target: String,
    },
    ArtifactFormatMismatch {
        execution_class: ExecutionClassId,
        artifact_media_type: String,
        expected_media_type: String,
    },
    /// A linked native factory is a Host build input, not a loadable Bundle Artifact.
    HostLinkedBuildRequired,
    /// Verified Cargo source cannot be activated until a new Host is built.
    CargoBuildInputRequiresHostRebuild {
        package: String,
        version: String,
        digest: String,
    },
    RuntimeNotAdmitted,
    RuntimeProfileMismatch {
        required_runtime_profile: String,
        admitted_runtime_profiles: Vec<String>,
    },
    MissingTargetCapabilities {
        requirements: Vec<TargetCapabilityRequirement>,
    },
    InvalidTargetCapabilityRequirement {
        feature: String,
    },
    InvalidTargetCapabilityProfile {
        profile: ExecutionTargetCapabilityProfile,
    },
    /// Candidate demands controls absent from the Host's verified runtime admission.
    /// Target mechanism markers never count as sandbox or resource evidence.
    ExecutionRequirementsUnverified {
        requirements: Vec<ExecutionAdmissionRequirementV6>,
    },
    AmbiguousRuntimeAdmission {
        matching_implementation_ids: Vec<String>,
    },
}

/// A rejected candidate from an implementation-resolution explanation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RejectedPluginImplementation {
    pub implementation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
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
    /// Aggregate Guest linear-memory bound actually configured and rechecked
    /// by this Host's Wasm Component Adapter. A mechanism alone is not proof.
    pub enforced_wasm_memory_ceiling_bytes: Option<u64>,
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
            enforced_wasm_memory_ceiling_bytes: None,
        }
    }

    /// Records the aggregate Guest linear-memory ceiling this Host actually
    /// configures and rechecks for the selected Wasm Component instance.
    /// Invalid or non-Wasm admissions remain fail closed during selection.
    #[must_use]
    pub fn with_enforced_wasm_memory_ceiling(mut self, max_bytes: u64) -> Self {
        self.enforced_wasm_memory_ceiling_bytes = Some(max_bytes);
        self
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
    pub variant_id: Option<String>,
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
        PluginManifest::V5(_) => "V5",
        PluginManifest::V6(_) => "V6",
    };
    let explanation = explain_implementation(manifest, policy)?;
    let failure_detail = explanation.failure_detail(schema);
    explanation
        .selected
        .ok_or(BundleError::InvalidBundle(failure_detail))
}

/// Explains a Host's exact implementation choice or all rejected candidates.
///
/// A missing target feature remains a rejection even if another implementation
/// can be selected. Unverified execution controls instead veto the selection:
/// a lower-priority or same-priority variant may not silently relax them.
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
                variant_id: None,
                host_targets: vec![value.artifact.target.clone()],
                input: CandidateInput::Artifact(value.artifact.clone()),
                descriptor,
                artifact_matches_wasm: value.artifact.media_type == "application/wasm",
                enforce_artifact_capability: false,
                execution_requirements: Vec::new(),
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
        PluginManifest::V5(value) => {
            let candidates = value
                .implementations
                .iter()
                .flat_map(|implementation| {
                    implementation.variants.iter().map(|variant| Candidate {
                        implementation_id: implementation.id.clone(),
                        variant_id: Some(variant.id.clone()),
                        host_targets: variant.host_targets.clone(),
                        input: CandidateInput::Artifact(variant.artifact.clone()),
                        descriptor: value.contract.resolve(&variant.runtime),
                        artifact_matches_wasm: false,
                        enforce_artifact_capability: true,
                        execution_requirements: Vec::new(),
                    })
                })
                .collect::<Vec<_>>();
            Ok(explain_candidates(&candidates, policy))
        }
        PluginManifest::V6(value) => {
            let candidates = value
                .implementations
                .iter()
                .flat_map(|implementation| {
                    implementation.variants.iter().map(|variant| Candidate {
                        implementation_id: implementation.id.clone(),
                        variant_id: Some(variant.id.clone()),
                        host_targets: variant.host_targets.clone(),
                        input: match &variant.input {
                            PluginVariantInputV6::Artifact { artifact } => {
                                CandidateInput::Artifact(artifact.clone())
                            }
                            PluginVariantInputV6::CargoBuildInput { build_input } => {
                                CandidateInput::CargoBuildInput(build_input.clone())
                            }
                        },
                        descriptor: value.contract.resolve(&variant.runtime),
                        artifact_matches_wasm: false,
                        enforce_artifact_capability: true,
                        execution_requirements: variant.execution_requirements.clone(),
                    })
                })
                .collect::<Vec<_>>();
            Ok(explain_candidates(&candidates, policy))
        }
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    implementation_id: String,
    variant_id: Option<String>,
    host_targets: Vec<String>,
    input: CandidateInput,
    descriptor: PluginDescriptor,
    artifact_matches_wasm: bool,
    enforce_artifact_capability: bool,
    execution_requirements: Vec<ExecutionAdmissionRequirementV6>,
}

#[derive(Clone, Debug)]
enum CandidateInput {
    Artifact(PluginArtifactV2),
    CargoBuildInput(PluginCargoBuildInputV6),
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
            variant_id: None,
            host_targets: targets.clone(),
            input: CandidateInput::Artifact(artifact.clone()),
            descriptor: contract.resolve(runtime),
            artifact_matches_wasm: false,
            enforce_artifact_capability: false,
            execution_requirements: Vec::new(),
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
        let (compatible, unverified_controls) =
            candidates_for_admission(candidates, policy, admission, &mut rejected);
        if unverified_controls {
            return ImplementationSelectionExplanation {
                selected: None,
                rejected,
            };
        }

        match compatible.as_slice() {
            [] => {}
            [candidate] => {
                let CandidateInput::Artifact(artifact) = &candidate.input else {
                    // Keep the runtime-only API fail closed even if a future
                    // admission change accidentally lets a build input through.
                    rejected.push(rejected_candidate(
                        candidate,
                        build_input_rejection(&candidate.input),
                    ));
                    return ImplementationSelectionExplanation {
                        selected: None,
                        rejected,
                    };
                };
                return ImplementationSelectionExplanation {
                    selected: Some(ResolvedPluginImplementation {
                        implementation_id: candidate.implementation_id.clone(),
                        variant_id: candidate.variant_id.clone(),
                        descriptor: candidate.descriptor.clone(),
                        artifact: artifact.clone(),
                    }),
                    rejected,
                };
            }
            matches => {
                rejected.push(RejectedPluginImplementation {
                    implementation_id: "<host-policy>".to_owned(),
                    variant_id: None,
                    execution_class: admission.execution_class.clone(),
                    runtime_profile: admission.runtime_profile.clone(),
                    reason: ImplementationRejectionReason::AmbiguousRuntimeAdmission {
                        matching_implementation_ids: matches
                            .iter()
                            .map(|candidate| candidate.identity())
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
) -> (Vec<&'a Candidate>, bool) {
    let mut compatible = Vec::new();
    let mut unverified_controls = false;
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
        if !record_target_match(candidate, policy, rejected) {
            continue;
        }
        let unverified = unverified_execution_requirements(candidate, admission);
        if !unverified.is_empty() {
            rejected.push(rejected_candidate(
                candidate,
                ImplementationRejectionReason::ExecutionRequirementsUnverified {
                    requirements: unverified,
                },
            ));
            unverified_controls = true;
            continue;
        }
        if record_artifact_format_match(candidate, admission, rejected)
            && record_capability_match(candidate, admission, rejected)
        {
            compatible.push(candidate);
        }
    }
    (compatible, unverified_controls)
}

fn unverified_execution_requirements(
    candidate: &Candidate,
    admission: &RuntimeAdmission,
) -> Vec<ExecutionAdmissionRequirementV6> {
    let memory_ceiling = (admission.execution_class.as_str() == "lenso.wasm-component@1"
        && admission
            .capabilities
            .supports(ExecutionTargetCapability::WasmComponent))
    .then_some(admission.enforced_wasm_memory_ceiling_bytes)
    .flatten();
    candidate
        .execution_requirements
        .iter()
        .filter(|requirement| match requirement {
            ExecutionAdmissionRequirementV6::MemoryCeiling { max_bytes } => {
                !memory_ceiling.is_some_and(|enforced| enforced > 0 && enforced <= *max_bytes)
            }
            // In-process Host imports may block synchronously. Epoch interrupts
            // cannot prove a total wall-clock turn deadline across those calls.
            ExecutionAdmissionRequirementV6::TurnDeadline { .. }
            | ExecutionAdmissionRequirementV6::PermissionGrant { .. }
            | ExecutionAdmissionRequirementV6::OsSandbox => true,
        })
        .cloned()
        .collect()
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
    if !candidate.artifact_matches_wasm
        && !candidate
            .host_targets
            .iter()
            .any(|target| target == "*" || target == &policy.host_target)
    {
        rejected.push(rejected_candidate(
            candidate,
            ImplementationRejectionReason::HostTargetMismatch {
                host_target: policy.host_target.clone(),
                candidate_targets: candidate.host_targets.clone(),
            },
        ));
        return false;
    }
    if let CandidateInput::Artifact(artifact) = &candidate.input
        && artifact.media_type == "application/vnd.lenso.process"
        && artifact.target != policy.host_target
    {
        rejected.push(rejected_candidate(
            candidate,
            ImplementationRejectionReason::ArtifactTargetMismatch {
                host_target: policy.host_target.clone(),
                artifact_target: artifact.target.clone(),
            },
        ));
        return false;
    }
    true
}

fn record_artifact_format_match(
    candidate: &Candidate,
    admission: &RuntimeAdmission,
    rejected: &mut Vec<RejectedPluginImplementation>,
) -> bool {
    if !candidate.enforce_artifact_capability {
        return true;
    }
    if matches!(&candidate.input, CandidateInput::CargoBuildInput(_)) {
        rejected.push(rejected_candidate(
            candidate,
            build_input_rejection(&candidate.input),
        ));
        return false;
    }
    if admission.execution_class.as_str() == "lenso.native-rust@1" {
        rejected.push(rejected_candidate(
            candidate,
            ImplementationRejectionReason::HostLinkedBuildRequired,
        ));
        return false;
    }
    // Official versioned Execution Classes fix their Artifact format. Third-
    // party classes remain open and must enforce their own Adapter contract.
    let expected = match admission.execution_class.as_str() {
        "lenso.process@1" => "application/vnd.lenso.process",
        "lenso.wasm-component@1" => "application/wasm",
        "lenso.bun-process@1" | "lenso.quickjs@1" => "application/javascript",
        _ => return true,
    };
    let CandidateInput::Artifact(artifact) = &candidate.input else {
        return false;
    };
    if artifact.media_type == expected {
        return true;
    }
    rejected.push(rejected_candidate(
        candidate,
        ImplementationRejectionReason::ArtifactFormatMismatch {
            execution_class: admission.execution_class.clone(),
            artifact_media_type: artifact.media_type.clone(),
            expected_media_type: expected.to_owned(),
        },
    ));
    false
}

fn record_capability_match(
    candidate: &Candidate,
    admission: &RuntimeAdmission,
    rejected: &mut Vec<RejectedPluginImplementation>,
) -> bool {
    let requirements = match target_requirements(candidate) {
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
            let admitted_profiles = policy
                .runtimes
                .iter()
                .filter(|admission| {
                    admission.execution_class == *candidate.descriptor.execution_class()
                })
                .map(|admission| admission.runtime_profile.clone())
                .collect::<BTreeSet<_>>();
            let reason = if admitted_profiles.is_empty() {
                ImplementationRejectionReason::RuntimeNotAdmitted
            } else {
                ImplementationRejectionReason::RuntimeProfileMismatch {
                    required_runtime_profile: candidate.descriptor.runtime_profile().to_owned(),
                    admitted_runtime_profiles: admitted_profiles.into_iter().collect(),
                }
            };
            rejected.push(rejected_candidate(candidate, reason));
        }
    }
}

fn target_requirements(candidate: &Candidate) -> Result<Vec<TargetCapabilityRequirement>, String> {
    let descriptor = &candidate.descriptor;
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
    if candidate.enforce_artifact_capability {
        let inherent = match &candidate.input {
            CandidateInput::Artifact(artifact) => match artifact.media_type.as_str() {
                "application/wasm" => Some(ExecutionTargetCapability::WasmComponent),
                "application/vnd.lenso.process" => Some(ExecutionTargetCapability::NativeProcess),
                _ => None,
            },
            CandidateInput::CargoBuildInput(_) => None,
        };
        if let Some(feature) = inherent {
            requirements
                .entry(feature)
                .or_insert(TargetCapabilityRequirement {
                    capability_id: descriptor.plugin_id().to_owned(),
                    operation: "<artifact>".to_owned(),
                    feature,
                });
        }
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
        variant_id: candidate.variant_id.clone(),
        execution_class: candidate.descriptor.execution_class().clone(),
        runtime_profile: candidate.descriptor.runtime_profile().to_owned(),
        reason,
    }
}

fn build_input_rejection(input: &CandidateInput) -> ImplementationRejectionReason {
    match input {
        CandidateInput::CargoBuildInput(build_input) => {
            ImplementationRejectionReason::CargoBuildInputRequiresHostRebuild {
                package: build_input.package.clone(),
                version: build_input.version.clone(),
                digest: build_input.digest.clone(),
            }
        }
        CandidateInput::Artifact(_) => ImplementationRejectionReason::HostLinkedBuildRequired,
    }
}

fn render_rejection(rejection: &RejectedPluginImplementation) -> String {
    let reason = match &rejection.reason {
        ImplementationRejectionReason::HostTargetMismatch { host_target, .. } => {
            format!("host target `{host_target}` does not match")
        }
        ImplementationRejectionReason::ArtifactTargetMismatch {
            host_target,
            artifact_target,
        } => format!(
            "Process artifact target `{artifact_target}` does not match host target `{host_target}`"
        ),
        ImplementationRejectionReason::ArtifactFormatMismatch {
            execution_class,
            artifact_media_type,
            expected_media_type,
        } => format!(
            "Artifact format `{artifact_media_type}` does not match execution class `{execution_class}` (requires `{expected_media_type}`)"
        ),
        ImplementationRejectionReason::HostLinkedBuildRequired =>
            "native-linked Plugin is a static build input; adopt its exact source and rebuild the Host, not a runtime-loadable Bundle Artifact".to_owned(),
        ImplementationRejectionReason::CargoBuildInputRequiresHostRebuild {
            package,
            version,
            digest,
        } => format!(
            "Cargo build input `{package}@{version}` ({digest}) requires a new linked Host build; it is not a runtime-loadable Bundle Artifact"
        ),
        ImplementationRejectionReason::RuntimeNotAdmitted => "runtime is not admitted".to_owned(),
        ImplementationRejectionReason::RuntimeProfileMismatch {
            required_runtime_profile,
            admitted_runtime_profiles,
        } => format!(
            "runtime ABI/profile `{required_runtime_profile}` is not admitted (Host admits {})",
            admitted_runtime_profiles
                .iter()
                .map(|profile| format!("`{profile}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
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
                    } else if requirement.operation == "<artifact>" {
                        format!(
                            "`{}` required by artifact format",
                            requirement.feature.as_str()
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
        ImplementationRejectionReason::ExecutionRequirementsUnverified { requirements } => {
            render_unverified_requirements(rejection, requirements)
        }
        ImplementationRejectionReason::AmbiguousRuntimeAdmission {
            matching_implementation_ids,
        } => format!(
            "ambiguous candidates {}",
            matching_implementation_ids.join(", ")
        ),
    };
    match &rejection.variant_id {
        Some(variant) => format!("{}/{}: {reason}", rejection.implementation_id, variant),
        None => format!("{}: {reason}", rejection.implementation_id),
    }
}

fn render_unverified_requirements(
    rejection: &RejectedPluginImplementation,
    requirements: &[ExecutionAdmissionRequirementV6],
) -> String {
    let mut detail = format!(
        "Host has no verified enforcement for {}; no weaker variant was selected",
        requirements
            .iter()
            .map(render_execution_requirement)
            .collect::<Vec<_>>()
            .join(", ")
    );
    if rejection.execution_class.as_str() != "lenso.wasm-component@1" {
        return detail;
    }
    if requirements.iter().any(|requirement| {
        matches!(
            requirement,
            ExecutionAdmissionRequirementV6::MemoryCeiling { .. }
        )
    }) {
        detail.push_str(
            "; this Host admission did not prove an aggregate Guest linear-memory bound at or below the requested ceiling",
        );
    }
    if requirements.iter().any(|requirement| {
        matches!(
            requirement,
            ExecutionAdmissionRequirementV6::TurnDeadline { .. }
        )
    }) {
        detail.push_str("; the current Wasm turn timer pauses during Host imports");
    }
    if requirements.iter().any(|requirement| {
        matches!(
            requirement,
            ExecutionAdmissionRequirementV6::MemoryCeiling { .. }
                | ExecutionAdmissionRequirementV6::TurnDeadline { .. }
        )
    }) {
        detail.push_str("; V6 limits require a selected, durably bound Adapter configuration rechecked at startup");
    }
    detail
}

fn render_execution_requirement(requirement: &ExecutionAdmissionRequirementV6) -> String {
    match requirement {
        ExecutionAdmissionRequirementV6::PermissionGrant { permission } => {
            let permission = match permission {
                RequiredPermissionV6::OutboundNetwork => "outbound network",
                RequiredPermissionV6::FilesystemRead => "filesystem read",
                RequiredPermissionV6::FilesystemWrite => "filesystem write",
                RequiredPermissionV6::SpawnProcess => "process spawn",
                RequiredPermissionV6::ReadEnvironment => "environment read",
            };
            format!("permission grant `{permission}`")
        }
        ExecutionAdmissionRequirementV6::OsSandbox => "OS sandbox isolation".to_owned(),
        ExecutionAdmissionRequirementV6::MemoryCeiling { max_bytes } => {
            format!("memory ceiling `{max_bytes}` bytes")
        }
        ExecutionAdmissionRequirementV6::TurnDeadline { max_millis } => {
            format!("turn deadline `{max_millis}` ms")
        }
    }
}

impl Candidate {
    fn identity(&self) -> String {
        self.variant_id.as_ref().map_or_else(
            || self.implementation_id.clone(),
            |variant| format!("{}/{variant}", self.implementation_id),
        )
    }
}
