//! Read-only execution-target capability preflight for the current resolved App.
//!
//! The Host remains the sole owner of implementation selection and App
//! resolution. This command only checks a fully explicit target profile against
//! the already-resolved Plan; it neither starts a Host nor selects a fallback.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use clap::Parser;
use lenso_app_plan::{CapabilityOperationKind, ResolvedAppPlan};
use serde::{Deserialize, Serialize};

use lenso_engine_app::plugins;

/// Canonical cross-repository contract identifier owned by `lenso-protocols`.
const EXECUTION_TARGET_CAPABILITY_PROFILE: &str = "lenso.execution-target-capability-profile@1";

/// Parses and runs `lenso app explain` before the legacy App command dispatcher.
pub(crate) fn run_from(arguments: Vec<String>) -> anyhow::Result<()> {
    let mut parser_arguments = vec!["lenso app explain".to_owned()];
    parser_arguments.extend(arguments.into_iter().skip(2));
    let args = AppExplainArgs::parse_from(parser_arguments);
    let report = report_for(args);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.status == ExplainStatus::Rejected {
        anyhow::bail!("execution-target preflight rejected; inspect the JSON report")
    }
    Ok(())
}

/// Whether the root invocation belongs to this CLI-owned read-only preflight.
pub(crate) fn is_invocation(arguments: &[String]) -> bool {
    matches!(arguments, [app, explain, ..] if app == "app" && explain == "explain")
}

#[derive(Clone, Debug, Parser)]
#[command(
    name = "lenso app explain",
    about = "Preflight explicit execution-target capability profiles against the current resolved App",
    long_about = "Reads the current Host-derived App Plan and verifies that every selected runtime profile has one canonical target capability profile. This command is read-only: it does not choose implementations, mutate the Plugin Root, start a Host, or fall back to another target. Output is always stable JSON for CI."
)]
struct AppExplainArgs {
    /// Canonical `lenso.execution-target-capability-profile@1` JSON file. Repeat once for each selected runtime profile.
    #[arg(long = "profile", required = true)]
    profiles: Vec<PathBuf>,
    /// App project root. Defaults to the current directory.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Extra exact target requirement in `<target-profile>:<capability>` form. Repeat as needed.
    #[arg(long = "require")]
    requirements: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ExecutionTargetCapability {
    Browser,
    Event,
    HostImports,
    NativeProcess,
    Remote,
    Request,
    Stream,
    WasmComponent,
    WebSocket,
    Workers,
}

impl ExecutionTargetCapability {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Event => "event",
            Self::HostImports => "host-imports",
            Self::NativeProcess => "native-process",
            Self::Remote => "remote",
            Self::Request => "request",
            Self::Stream => "stream",
            Self::WasmComponent => "wasm-component",
            Self::WebSocket => "websocket",
            Self::Workers => "workers",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "browser" => Self::Browser,
            "event" => Self::Event,
            "host-imports" => Self::HostImports,
            "native-process" => Self::NativeProcess,
            "remote" => Self::Remote,
            "request" => Self::Request,
            "stream" => Self::Stream,
            "wasm-component" => Self::WasmComponent,
            "websocket" => Self::WebSocket,
            "workers" => Self::Workers,
            _ => return None,
        })
    }

    const fn for_operation(kind: CapabilityOperationKind) -> Self {
        match kind {
            CapabilityOperationKind::Request => Self::Request,
            CapabilityOperationKind::Stream => Self::Stream,
            CapabilityOperationKind::Event => Self::Event,
        }
    }
}

/// Exact portable wire contract emitted by target adapters and accepted by this CLI.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExecutionTargetCapabilityProfile {
    profile: String,
    target_profile: String,
    capabilities: Vec<ExecutionTargetCapability>,
}

impl ExecutionTargetCapabilityProfile {
    fn is_valid(&self) -> bool {
        self.profile == EXECUTION_TARGET_CAPABILITY_PROFILE
            && is_target_profile_token(&self.target_profile)
            && self
                .capabilities
                .windows(2)
                .all(|pair| pair[0].as_str() < pair[1].as_str())
    }

    fn supports(&self, capability: ExecutionTargetCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

fn is_target_profile_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'@'))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ExplainStatus {
    Admitted,
    Rejected,
}

#[derive(Debug, Serialize)]
struct ExplainReport {
    schema_version: u32,
    kind: &'static str,
    status: ExplainStatus,
    execution_target_profiles: Vec<ExecutionTargetCapabilityProfile>,
    requirements: Vec<TargetRequirement>,
    reasons: Vec<ExplainReason>,
}

impl ExplainReport {
    fn new(profiles: Vec<ExecutionTargetCapabilityProfile>) -> Self {
        Self {
            schema_version: 1,
            kind: "lenso.app-explain",
            status: ExplainStatus::Admitted,
            execution_target_profiles: profiles,
            requirements: Vec::new(),
            reasons: Vec::new(),
        }
    }

    fn reject(&mut self, reason: ExplainReason) {
        self.status = ExplainStatus::Rejected;
        self.reasons.push(reason);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
enum TargetRequirement {
    CapabilityOperation {
        target_profile: String,
        instance: String,
        execution_class: String,
        capability_id: String,
        descriptor_version: String,
        operation: String,
        operation_kind: &'static str,
        feature: ExecutionTargetCapability,
    },
    Explicit {
        target_profile: String,
        feature: ExecutionTargetCapability,
    },
}

impl TargetRequirement {
    fn target_profile(&self) -> &str {
        match self {
            Self::CapabilityOperation { target_profile, .. }
            | Self::Explicit { target_profile, .. } => target_profile,
        }
    }

    const fn feature(&self) -> ExecutionTargetCapability {
        match self {
            Self::CapabilityOperation { feature, .. } | Self::Explicit { feature, .. } => *feature,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ExplainReason {
    InvalidTargetProfile {
        profile_path: String,
        message: String,
    },
    DuplicateTargetProfile {
        target_profile: String,
        profile_paths: Vec<String>,
    },
    InvalidExplicitRequirement {
        requirement: String,
        message: String,
    },
    DuplicateExplicitRequirement {
        target_profile: String,
        feature: ExecutionTargetCapability,
    },
    AppResolutionFailed {
        root: String,
        message: String,
    },
    MissingTargetProfile {
        target_profile: String,
        requirements: Vec<TargetRequirement>,
    },
    UnusedTargetProfile {
        target_profile: String,
    },
    MissingTargetCapability {
        target_profile: String,
        feature: ExecutionTargetCapability,
        requirements: Vec<TargetRequirement>,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ExplicitRequirement {
    target_profile: String,
    feature: ExecutionTargetCapability,
}

fn report_for(args: AppExplainArgs) -> ExplainReport {
    let (profiles, profile_sources, mut report) = parse_profiles(&args.profiles);
    let explicit = parse_explicit_requirements(&args.requirements, &mut report);
    if report.status == ExplainStatus::Rejected {
        return report;
    }

    let root = match plugins::project_root(args.root) {
        Ok(root) => root,
        Err(error) => {
            report.reject(ExplainReason::AppResolutionFailed {
                root: "<current-directory>".to_owned(),
                message: error.to_string(),
            });
            return report;
        }
    };
    let resolved = match plugins::load_resolved_app(&root) {
        Ok(resolved) => resolved,
        Err(error) => {
            report.reject(ExplainReason::AppResolutionFailed {
                root: root.display().to_string(),
                message: format!("{error:#}"),
            });
            return report;
        }
    };

    report.requirements = requirements_for_plan(resolved.plan());
    report
        .requirements
        .extend(
            explicit
                .into_iter()
                .map(|requirement| TargetRequirement::Explicit {
                    target_profile: requirement.target_profile,
                    feature: requirement.feature,
                }),
        );
    validate_requirements(&mut report, &profiles, &profile_sources);
    report
}

fn parse_profiles(
    paths: &[PathBuf],
) -> (
    BTreeMap<String, ExecutionTargetCapabilityProfile>,
    BTreeMap<String, Vec<String>>,
    ExplainReport,
) {
    let mut profiles = BTreeMap::new();
    let mut profile_sources = BTreeMap::<String, Vec<String>>::new();
    let mut visible_profiles = Vec::new();
    let mut report = ExplainReport::new(Vec::new());

    for path in paths {
        let display = path.display().to_string();
        let profile = match read_profile(path) {
            Ok(profile) => profile,
            Err(message) => {
                report.reject(ExplainReason::InvalidTargetProfile {
                    profile_path: display,
                    message,
                });
                continue;
            }
        };
        if !profile.is_valid() {
            report.reject(ExplainReason::InvalidTargetProfile {
                profile_path: display,
                message: format!(
                    "expected `{EXECUTION_TARGET_CAPABILITY_PROFILE}`, a target profile token of up to 128 ASCII [A-Za-z0-9._@-] characters, and a strictly sorted unique capability list"
                ),
            });
            continue;
        }
        profile_sources
            .entry(profile.target_profile.clone())
            .or_default()
            .push(display);
        profiles
            .entry(profile.target_profile.clone())
            .or_insert_with(|| profile.clone());
        visible_profiles.push(profile);
    }

    for (target_profile, sources) in &profile_sources {
        if sources.len() > 1 {
            report.reject(ExplainReason::DuplicateTargetProfile {
                target_profile: target_profile.clone(),
                profile_paths: sources.clone(),
            });
        }
    }
    visible_profiles.sort_by(|left, right| left.target_profile.cmp(&right.target_profile));
    report.execution_target_profiles = visible_profiles;
    (profiles, profile_sources, report)
}

fn read_profile(path: &PathBuf) -> Result<ExecutionTargetCapabilityProfile, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn parse_explicit_requirements(
    values: &[String],
    report: &mut ExplainReport,
) -> BTreeSet<ExplicitRequirement> {
    let mut requirements = BTreeSet::new();
    for value in values {
        let requirement = match parse_explicit_requirement(value) {
            Ok(requirement) => requirement,
            Err(message) => {
                report.reject(ExplainReason::InvalidExplicitRequirement {
                    requirement: value.clone(),
                    message,
                });
                continue;
            }
        };
        if !requirements.insert(requirement.clone()) {
            report.reject(ExplainReason::DuplicateExplicitRequirement {
                target_profile: requirement.target_profile,
                feature: requirement.feature,
            });
        }
    }
    requirements
}

fn parse_explicit_requirement(value: &str) -> Result<ExplicitRequirement, String> {
    let (target_profile, capability) = value
        .split_once(':')
        .ok_or("use `<target-profile>:<capability>`")?;
    if !is_target_profile_token(target_profile) {
        return Err("target profile must be 1..=128 ASCII [A-Za-z0-9._@-] characters".to_owned());
    }
    let feature = ExecutionTargetCapability::parse(capability).ok_or_else(|| {
        format!(
            "unknown capability `{capability}`; expected browser, event, host-imports, native-process, remote, request, stream, wasm-component, websocket, or workers"
        )
    })?;
    Ok(ExplicitRequirement {
        target_profile: target_profile.to_owned(),
        feature,
    })
}

fn requirements_for_plan(plan: &ResolvedAppPlan) -> Vec<TargetRequirement> {
    let mut requirements = Vec::new();
    for instance in plan.plugin_instances() {
        for endpoint in instance.provided_capabilities() {
            for operation in endpoint.operations() {
                let kind = endpoint
                    .operation_kind(operation)
                    .expect("a declared endpoint operation has a kind");
                requirements.push(TargetRequirement::CapabilityOperation {
                    target_profile: instance.runtime_profile().to_owned(),
                    instance: instance.instance_key().to_owned(),
                    execution_class: instance.execution_class().as_str().to_owned(),
                    capability_id: endpoint.capability_id().to_owned(),
                    descriptor_version: endpoint.descriptor_version().to_owned(),
                    operation: operation.clone(),
                    operation_kind: operation_kind_name(kind),
                    feature: ExecutionTargetCapability::for_operation(kind),
                });
            }
        }
    }
    requirements
}

const fn operation_kind_name(kind: CapabilityOperationKind) -> &'static str {
    match kind {
        CapabilityOperationKind::Request => "request",
        CapabilityOperationKind::Stream => "stream",
        CapabilityOperationKind::Event => "event",
    }
}

fn validate_requirements(
    report: &mut ExplainReport,
    profiles: &BTreeMap<String, ExecutionTargetCapabilityProfile>,
    profile_sources: &BTreeMap<String, Vec<String>>,
) {
    let mut grouped =
        BTreeMap::<(String, ExecutionTargetCapability), Vec<TargetRequirement>>::new();
    for requirement in &report.requirements {
        grouped
            .entry((
                requirement.target_profile().to_owned(),
                requirement.feature(),
            ))
            .or_default()
            .push(requirement.clone());
    }

    let selected_profiles = report
        .requirements
        .iter()
        .map(|requirement| requirement.target_profile().to_owned())
        .collect::<BTreeSet<_>>();
    for target_profile in profiles.keys() {
        if !selected_profiles.contains(target_profile) {
            report.reject(ExplainReason::UnusedTargetProfile {
                target_profile: target_profile.clone(),
            });
        }
    }

    for ((target_profile, feature), requirements) in grouped {
        let Some(profile) = profiles.get(&target_profile) else {
            report.reject(ExplainReason::MissingTargetProfile {
                target_profile,
                requirements,
            });
            continue;
        };
        if !profile.supports(feature) {
            report.reject(ExplainReason::MissingTargetCapability {
                target_profile,
                feature,
                requirements,
            });
        }
    }

    // A duplicate declaration has already produced a distinct ambiguity reason.
    // Keep the sources parameter in this function's contract to make that
    // precedence explicit where all profile validation is coordinated.
    debug_assert!(profile_sources.values().all(|sources| !sources.is_empty()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::{CapabilityEndpointPlan, ExecutionClassId, PluginInstancePlan};

    fn profile(capabilities: Vec<ExecutionTargetCapability>) -> ExecutionTargetCapabilityProfile {
        ExecutionTargetCapabilityProfile {
            profile: EXECUTION_TARGET_CAPABILITY_PROFILE.to_owned(),
            target_profile: "lenso.native-rust@1".to_owned(),
            capabilities,
        }
    }

    fn plan() -> ResolvedAppPlan {
        ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("orders", "company.orders")
                    .with_authoring(2, "lenso.native-rust@1")
                    .with_execution_class(ExecutionClassId::native_rust())
                    .with_capability(
                        CapabilityEndpointPlan::new(
                            "company.orders@1",
                            "1.0.0",
                            ["create", "watch"],
                        )
                        .with_stream_operation("watch"),
                    ),
            ],
            vec![],
        )
    }

    #[test]
    fn canonical_profile_is_required_without_repairing_input() {
        assert!(
            profile(vec![
                ExecutionTargetCapability::Request,
                ExecutionTargetCapability::Stream,
            ])
            .is_valid()
        );
        assert!(
            !profile(vec![
                ExecutionTargetCapability::Stream,
                ExecutionTargetCapability::Request,
            ])
            .is_valid()
        );
        assert!(
            !profile(vec![
                ExecutionTargetCapability::Request,
                ExecutionTargetCapability::Request,
            ])
            .is_valid()
        );
    }

    #[test]
    fn plan_requirements_preserve_operation_context() {
        let requirements = requirements_for_plan(&plan());
        assert_eq!(requirements.len(), 2);
        assert!(matches!(
            &requirements[0],
            TargetRequirement::CapabilityOperation { operation, feature: ExecutionTargetCapability::Request, .. }
                if operation == "create"
        ));
        assert!(matches!(
            &requirements[1],
            TargetRequirement::CapabilityOperation { operation, feature: ExecutionTargetCapability::Stream, .. }
                if operation == "watch"
        ));
    }

    #[test]
    fn explicit_requirements_reject_unknown_tokens_and_duplicates() {
        let mut report = ExplainReport::new(Vec::new());
        let requirements = parse_explicit_requirements(
            &[
                "lenso.native-rust@1:host-imports".to_owned(),
                "lenso.native-rust@1:host-imports".to_owned(),
                "lenso.native-rust@1:future-thing".to_owned(),
            ],
            &mut report,
        );
        assert_eq!(requirements.len(), 1);
        assert_eq!(report.status, ExplainStatus::Rejected);
        assert!(
            report
                .reasons
                .iter()
                .any(|reason| matches!(reason, ExplainReason::DuplicateExplicitRequirement { .. }))
        );
        assert!(
            report
                .reasons
                .iter()
                .any(|reason| matches!(reason, ExplainReason::InvalidExplicitRequirement { .. }))
        );
    }

    #[test]
    fn profile_rejection_groups_missing_feature_by_target() {
        let mut report =
            ExplainReport::new(vec![profile(vec![ExecutionTargetCapability::Request])]);
        report.requirements = requirements_for_plan(&plan());
        let profiles = BTreeMap::from([(
            "lenso.native-rust@1".to_owned(),
            profile(vec![ExecutionTargetCapability::Request]),
        )]);
        let sources = BTreeMap::from([(
            "lenso.native-rust@1".to_owned(),
            vec!["target.json".to_owned()],
        )]);
        validate_requirements(&mut report, &profiles, &sources);
        assert_eq!(report.status, ExplainStatus::Rejected);
        assert!(report.reasons.iter().any(|reason| matches!(
            reason,
            ExplainReason::MissingTargetCapability { feature: ExecutionTargetCapability::Stream, requirements, .. }
                if requirements.len() == 1
        )));
    }
}
