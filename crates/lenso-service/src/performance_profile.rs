use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;

pub const PERFORMANCE_PROFILE_PROTOCOL: &str = "lenso.performance-profile.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceProfileScope {
    ReducedDeterministic,
    EnvironmentVerification,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceMetric {
    DirectCallLatency,
    DirectCallThroughput,
    ResolverClientOverhead,
    PublishToConsumeLatency,
    InboxOutboxLag,
    WorkflowTransitionLatency,
    WorkflowTimerDelay,
    StoryFreshness,
    ConsoleQueryLatency,
    ConvergenceLatency,
    CpuUtilization,
    MemoryBytes,
    DatabaseConnections,
    BrokerBytes,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceBudgetDirection {
    AtMost,
    AtLeast,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceDecision {
    Passed,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceIssueCode {
    TopologyInvalid,
    MetadataIncomplete,
    MetricMissing,
    BudgetExceeded,
    VarianceExceeded,
    HiddenDataPlaneDependency,
    EnvironmentEvidenceInsufficient,
}

impl PerformanceIssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopologyInvalid => "performance_topology_invalid",
            Self::MetadataIncomplete => "performance_metadata_incomplete",
            Self::MetricMissing => "performance_metric_missing",
            Self::BudgetExceeded => "performance_budget_exceeded",
            Self::VarianceExceeded => "performance_variance_exceeded",
            Self::HiddenDataPlaneDependency => "performance_hidden_data_plane_dependency",
            Self::EnvironmentEvidenceInsufficient => {
                "performance_environment_evidence_insufficient"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceIssue {
    pub code: PerformanceIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceService {
    pub service_id: String,
    pub contract_id: String,
    pub store_id: String,
    pub release_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSystemTopology {
    pub topology_digest: String,
    pub services: Vec<ReferenceService>,
    pub transport_adapter_version: String,
    pub identity_adapter_version: String,
    pub deployment_adapter_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceBudget {
    pub metric: PerformanceMetric,
    pub unit: String,
    pub direction: PerformanceBudgetDirection,
    pub threshold: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMeasurement {
    pub metric: PerformanceMetric,
    pub unit: String,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceRun {
    pub run_id: String,
    pub release_set_digest: String,
    pub dataset_digest: String,
    pub concurrency: u32,
    pub duration_ms: u64,
    pub warmup_ms: u64,
    pub machine: BTreeMap<String, String>,
    pub infrastructure: BTreeMap<String, String>,
    pub measurements: Vec<PerformanceMeasurement>,
    pub system_plane_data_plane_requests: u64,
    pub runtime_console_data_plane_requests: u64,
    pub telemetry_data_plane_requests: u64,
    pub policy_data_plane_requests: u64,
    pub registry_data_plane_requests: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceProfileInput {
    pub scope: PerformanceProfileScope,
    pub support_manifest_digest: String,
    pub topology: ReferenceSystemTopology,
    pub budgets: Vec<PerformanceBudget>,
    pub runs: Vec<PerformanceRun>,
    pub variance_tolerance_basis_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceProfile {
    pub protocol: String,
    pub profile_id: String,
    pub profile_digest: String,
    pub scope: PerformanceProfileScope,
    pub support_manifest_digest: String,
    pub topology: ReferenceSystemTopology,
    pub budgets: Vec<PerformanceBudget>,
    pub runs: Vec<PerformanceRun>,
    pub variance_basis_points: BTreeMap<PerformanceMetric, u32>,
    pub decision: PerformanceDecision,
    pub issues: Vec<PerformanceIssue>,
    pub next_actions: Vec<String>,
}

#[must_use]
pub fn evaluate_performance_profile(mut input: PerformanceProfileInput) -> PerformanceProfile {
    input
        .topology
        .services
        .sort_by(|left, right| left.service_id.cmp(&right.service_id));
    input.budgets.sort_by_key(|budget| budget.metric);
    input
        .runs
        .sort_by(|left, right| left.run_id.cmp(&right.run_id));
    for run in &mut input.runs {
        run.measurements
            .sort_by_key(|measurement| measurement.metric);
    }

    let mut issues = Vec::new();
    let services = &input.topology.services;
    let service_ids = services
        .iter()
        .map(|service| service.service_id.as_str())
        .collect::<BTreeSet<_>>();
    let contract_ids = services
        .iter()
        .map(|service| service.contract_id.as_str())
        .collect::<BTreeSet<_>>();
    let store_ids = services
        .iter()
        .map(|service| service.store_id.as_str())
        .collect::<BTreeSet<_>>();
    if services.len() != 3
        || service_ids.len() != 3
        || contract_ids.len() != 3
        || store_ids.len() != 3
        || services.iter().any(|service| {
            service.service_id.trim().is_empty()
                || service.contract_id.trim().is_empty()
                || service.store_id.trim().is_empty()
                || !valid_digest(&service.release_digest)
        })
        || !valid_digest(&input.topology.topology_digest)
    {
        issues.push(issue(
            PerformanceIssueCode::TopologyInvalid,
            "The reference System is not exactly three distinct logical Services, Contracts, and Stores.",
            "Use three independently identified Service boundaries rather than replicas.",
            "Correct the reference topology and repeat the profile.",
        ));
    }

    let required_metrics = required_metrics();
    let budget_metrics = input
        .budgets
        .iter()
        .map(|budget| budget.metric)
        .collect::<BTreeSet<_>>();
    if !valid_digest(&input.support_manifest_digest)
        || input.topology.transport_adapter_version.trim().is_empty()
        || input.topology.identity_adapter_version.trim().is_empty()
        || input.topology.deployment_adapter_version.trim().is_empty()
        || input.budgets.len() != required_metrics.len()
        || budget_metrics != required_metrics
        || input.variance_tolerance_basis_points == 0
    {
        issues.push(issue(
            PerformanceIssueCode::MetadataIncomplete,
            "Performance profile metadata or evidence-backed budgets are incomplete.",
            "Bind the profile to exact releases, topology, adapters, units, and tolerances.",
            "Complete the pinned-environment profile metadata.",
        ));
    }

    let required_run_count = match input.scope {
        PerformanceProfileScope::ReducedDeterministic => 1,
        PerformanceProfileScope::EnvironmentVerification => 3,
    };
    if input.runs.len() < required_run_count {
        issues.push(issue(
            PerformanceIssueCode::EnvironmentEvidenceInsufficient,
            "The selected profile scope does not include enough repeated runs.",
            "Use at least three runs for Environment Verification and one for reduced diagnosis.",
            "Collect the missing pinned-environment runs.",
        ));
    }

    let budgets = input
        .budgets
        .iter()
        .map(|budget| (budget.metric, budget))
        .collect::<BTreeMap<_, _>>();
    for run in &input.runs {
        let metrics = run
            .measurements
            .iter()
            .map(|measurement| (measurement.metric, measurement))
            .collect::<BTreeMap<_, _>>();
        let metadata_valid = !run.run_id.trim().is_empty()
            && valid_digest(&run.release_set_digest)
            && valid_digest(&run.dataset_digest)
            && run.concurrency > 0
            && run.duration_ms > 0
            && run.warmup_ms > 0
            && !run.machine.is_empty()
            && !run.infrastructure.is_empty();
        if !metadata_valid {
            issues.push(issue(
                PerformanceIssueCode::MetadataIncomplete,
                format!("Performance run `{}` has incomplete metadata.", run.run_id),
                "Record concurrency, duration, warm-up, machine, infrastructure, dataset, and release facts.",
                "Repeat the run with the complete versioned profile.",
            ));
        }
        if metrics.len() != required_metrics.len()
            || required_metrics
                .iter()
                .any(|metric| !metrics.contains_key(metric))
        {
            issues.push(issue(
                PerformanceIssueCode::MetricMissing,
                format!("Performance run `{}` does not cover every required path.", run.run_id),
                "Measure request, Event, Workflow, Story, Console, convergence, and resource paths together.",
                "Collect the missing measurements and rerun the profile.",
            ));
        }
        for (metric, measurement) in metrics {
            let Some(budget) = budgets.get(&metric) else {
                issues.push(issue(
                    PerformanceIssueCode::MetricMissing,
                    format!("Performance metric `{:?}` has no reviewed budget.", metric),
                    "Define one unique budget for every required metric.",
                    "Correct the budget set and repeat the profile.",
                ));
                continue;
            };
            if measurement.unit != budget.unit
                || match budget.direction {
                    PerformanceBudgetDirection::AtMost => measurement.value > budget.threshold,
                    PerformanceBudgetDirection::AtLeast => measurement.value < budget.threshold,
                }
            {
                issues.push(issue(
                    PerformanceIssueCode::BudgetExceeded,
                    format!(
                        "Performance run `{}` exceeded the {:?} budget.",
                        run.run_id, metric
                    ),
                    "Treat the pinned threshold as an evidence-backed environment budget.",
                    "Diagnose the regression or update the reviewed profile with new evidence.",
                ));
            }
        }
        if run.system_plane_data_plane_requests > 0
            || run.runtime_console_data_plane_requests > 0
            || run.telemetry_data_plane_requests > 0
            || run.policy_data_plane_requests > 0
            || run.registry_data_plane_requests > 0
        {
            issues.push(issue(
                PerformanceIssueCode::HiddenDataPlaneDependency,
                "Established Data Plane traffic depended on a coordination or observability surface.",
                "Keep System Plane, Console, telemetry, policy, and registry services outside established traffic.",
                "Correct the topology and repeat the profile with those surfaces withheld.",
            ));
        }
    }

    let variance_basis_points = calculate_variance(&input.runs);
    if variance_basis_points
        .values()
        .any(|variance| *variance > input.variance_tolerance_basis_points)
    {
        issues.push(issue(
            PerformanceIssueCode::VarianceExceeded,
            "Repeated runs exceed the declared variance tolerance.",
            "Separate environment drift from a product regression before accepting the profile.",
            "Stabilize or re-pin the environment and repeat the measurements.",
        ));
    }

    let decision = if issues.is_empty() {
        PerformanceDecision::Passed
    } else {
        PerformanceDecision::Blocked
    };
    let next_actions = if issues.is_empty() {
        vec!["Attach this profile to the M6 acceptance evidence set.".to_owned()]
    } else {
        issues
            .iter()
            .flat_map(|issue| issue.next_actions.iter().cloned())
            .collect()
    };
    let mut profile = PerformanceProfile {
        protocol: PERFORMANCE_PROFILE_PROTOCOL.to_owned(),
        profile_id: String::new(),
        profile_digest: String::new(),
        scope: input.scope,
        support_manifest_digest: input.support_manifest_digest,
        topology: input.topology,
        budgets: input.budgets,
        runs: input.runs,
        variance_basis_points,
        decision,
        issues,
        next_actions,
    };
    profile.profile_digest = digest_without_identity(&profile);
    profile.profile_id = format!("performance-profile:{}", &profile.profile_digest[7..23]);
    profile
}

#[must_use]
pub fn performance_profile_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(PerformanceProfile))
        .expect("performance profile schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/ga/lenso.performance-profile.v1.schema.json".to_owned(),
    );
    schema
}

fn required_metrics() -> BTreeSet<PerformanceMetric> {
    [
        PerformanceMetric::DirectCallLatency,
        PerformanceMetric::DirectCallThroughput,
        PerformanceMetric::ResolverClientOverhead,
        PerformanceMetric::PublishToConsumeLatency,
        PerformanceMetric::InboxOutboxLag,
        PerformanceMetric::WorkflowTransitionLatency,
        PerformanceMetric::WorkflowTimerDelay,
        PerformanceMetric::StoryFreshness,
        PerformanceMetric::ConsoleQueryLatency,
        PerformanceMetric::ConvergenceLatency,
        PerformanceMetric::CpuUtilization,
        PerformanceMetric::MemoryBytes,
        PerformanceMetric::DatabaseConnections,
        PerformanceMetric::BrokerBytes,
    ]
    .into_iter()
    .collect()
}

fn calculate_variance(runs: &[PerformanceRun]) -> BTreeMap<PerformanceMetric, u32> {
    let mut values = BTreeMap::<PerformanceMetric, Vec<u64>>::new();
    for run in runs {
        for measurement in &run.measurements {
            values
                .entry(measurement.metric)
                .or_default()
                .push(measurement.value);
        }
    }
    values
        .into_iter()
        .map(|(metric, values)| {
            let min = values.iter().copied().min().unwrap_or(0);
            let max = values.iter().copied().max().unwrap_or(0);
            let variance = if min == 0 {
                if max == 0 { 0 } else { u32::MAX }
            } else {
                u32::try_from(((max - min) as u128 * 10_000) / u128::from(min)).unwrap_or(u32::MAX)
            };
            (metric, variance)
        })
        .collect()
}

fn issue(
    code: PerformanceIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> PerformanceIssue {
    PerformanceIssue {
        code,
        message: message.into(),
        remediation: remediation.into(),
        next_actions: vec![next_action.into()],
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn digest_without_identity(profile: &PerformanceProfile) -> String {
    let mut canonical = profile.clone();
    canonical.profile_id.clear();
    canonical.profile_digest.clear();
    extraction_input_digest(
        &serde_json::to_vec(&canonical).expect("performance profile serializes"),
    )
}
