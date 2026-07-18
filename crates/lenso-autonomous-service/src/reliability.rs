use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lenso_service::{
    EffectiveReliabilityValues, ReliabilityContract, ReliabilityLivenessSemantics,
    ReliabilityProfile, ReliabilityReadinessSemantics,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::ToSchema;

use super::{RuntimePhase, ServiceRuntimeState};

pub const RELIABILITY_REPORT_PROTOCOL: &str = "lenso.reliability-report.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityDependencyState {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityDependencyObservation {
    pub state: ReliabilityDependencyState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_references: Vec<String>,
}

impl ReliabilityDependencyObservation {
    #[must_use]
    pub fn new(state: ReliabilityDependencyState, evidence_references: Vec<String>) -> Self {
        Self {
            state,
            evidence_references,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityMetricObservation {
    pub value: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_references: Vec<String>,
}

impl ReliabilityMetricObservation {
    #[must_use]
    pub fn new(value: u64, evidence_references: Vec<String>) -> Self {
        Self {
            value,
            evidence_references,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityExternalObservations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, ReliabilityDependencyObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_basis_points: Option<ReliabilityMetricObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<ReliabilityMetricObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_budget_consumed_basis_points: Option<ReliabilityMetricObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReliabilityObservationError {
    pub message: String,
    pub evidence_references: Vec<String>,
}

impl ReliabilityObservationError {
    #[must_use]
    pub fn new(message: impl Into<String>, evidence_references: Vec<String>) -> Self {
        Self {
            message: message.into(),
            evidence_references,
        }
    }
}

#[async_trait]
pub trait ReliabilityObservationSource: std::fmt::Debug + Send + Sync {
    async fn observe(
        &self,
        service_id: &str,
    ) -> Result<ReliabilityExternalObservations, ReliabilityObservationError>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReliabilityPressureObservations {
    pub queue_backlog: Option<ReliabilityMetricObservation>,
    pub workflow_backlog: Option<ReliabilityMetricObservation>,
    pub timer_lag_ms: Option<ReliabilityMetricObservation>,
    pub retry_exhaustion: Option<ReliabilityMetricObservation>,
    pub compensation_pressure: Option<ReliabilityMetricObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityServiceState {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityCheckState {
    Met,
    Breached,
    Unknown,
    Allowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityIssueCode {
    ServiceRuntimeUnavailable,
    DependencyCriticalUnavailable,
    DependencyDegradableUnavailable,
    DependencyObservationMissing,
    QueueBacklogLimitExceeded,
    WorkflowBacklogLimitExceeded,
    TimerLagLimitExceeded,
    RetryExhaustionLimitExceeded,
    CompensationPressureLimitExceeded,
    AvailabilityTargetBreached,
    LatencyTargetBreached,
    ErrorBudgetLimitExceeded,
    ReliabilityObservationMissing,
    ReliabilityObservationSourceUnavailable,
    ReliabilityStoreObservationUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityCheck {
    pub code: String,
    pub state: ReliabilityCheckState,
    pub observed: Value,
    pub expected: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_references: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_code: Option<ReliabilityIssueCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDegradedMode {
    pub dependency_id: String,
    pub mode: String,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityHealthResult {
    pub healthy: bool,
    pub semantics: String,
    pub issue_codes: Vec<ReliabilityIssueCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityEnforcementBoundary {
    pub reports_only: bool,
    pub blocks_production_promotion: bool,
    pub executes_canary_policy: bool,
    pub triggers_automated_rollback: bool,
}

impl Default for ReliabilityEnforcementBoundary {
    fn default() -> Self {
        Self {
            reports_only: true,
            blocks_production_promotion: false,
            executes_canary_policy: false,
            triggers_automated_rollback: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityReport {
    pub protocol: String,
    pub service_id: String,
    pub contract_id: String,
    pub contract_version: String,
    pub profile: ReliabilityProfile,
    pub effective_values: EffectiveReliabilityValues,
    pub state: ReliabilityServiceState,
    pub liveness: ReliabilityHealthResult,
    pub readiness: ReliabilityHealthResult,
    pub active_degraded_modes: Vec<ActiveDegradedMode>,
    pub checks: Vec<ReliabilityCheck>,
    pub enforcement: ReliabilityEnforcementBoundary,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReliabilityCollectionIssues {
    source: Option<ReliabilityObservationError>,
    store: Option<String>,
}

#[must_use]
pub fn evaluate_reliability(
    service_id: &str,
    phase: RuntimePhase,
    worker_phase: RuntimePhase,
    contract: &ReliabilityContract,
    external: &ReliabilityExternalObservations,
    pressure: &ReliabilityPressureObservations,
) -> ReliabilityReport {
    evaluate_reliability_with_issues(
        service_id,
        phase,
        worker_phase,
        contract,
        external,
        pressure,
        &ReliabilityCollectionIssues::default(),
    )
}

fn evaluate_reliability_with_issues(
    service_id: &str,
    phase: RuntimePhase,
    worker_phase: RuntimePhase,
    contract: &ReliabilityContract,
    external: &ReliabilityExternalObservations,
    pressure: &ReliabilityPressureObservations,
    collection_issues: &ReliabilityCollectionIssues,
) -> ReliabilityReport {
    let effective_values = contract
        .effective_values()
        .expect("validated Reliability Contract must have an effective profile");
    let mut state = ReliabilityServiceState::Healthy;
    let mut checks = Vec::new();
    let mut active_degraded_modes = Vec::new();

    let runtime_healthy = phase == RuntimePhase::Ready && worker_phase == RuntimePhase::Ready;
    checks.push(ReliabilityCheck {
        code: "service_runtime".to_owned(),
        state: if runtime_healthy {
            ReliabilityCheckState::Met
        } else {
            ReliabilityCheckState::Breached
        },
        observed: json!({"apiPhase": phase, "workerPhase": worker_phase}),
        expected: json!({"apiPhase": "ready", "workerPhase": "ready"}),
        evidence_references: vec!["runtime:service-workloads".to_owned()],
        issue_code: (!runtime_healthy).then_some(ReliabilityIssueCode::ServiceRuntimeUnavailable),
        next_actions: (!runtime_healthy)
            .then(|| vec!["restore_service_workloads".to_owned()])
            .unwrap_or_default(),
    });
    if !runtime_healthy {
        state = ReliabilityServiceState::Unavailable;
    }

    for (dependency_id, criticality) in &contract.dependency_criticality {
        let observation = external.dependencies.get(dependency_id);
        let evidence = observation
            .map(|observation| observation.evidence_references.clone())
            .unwrap_or_else(|| vec![format!("reliability-observer:dependency:{dependency_id}")]);
        let available = observation
            .is_some_and(|observation| observation.state == ReliabilityDependencyState::Available);
        let observed = observation.map_or_else(
            || json!("unobserved"),
            |observation| json!(observation.state),
        );
        let (check_state, issue_code, next_actions) = match (criticality.as_str(), observation) {
            ("optional", Some(observation))
                if observation.state == ReliabilityDependencyState::Unavailable =>
            {
                (ReliabilityCheckState::Allowed, None, Vec::new())
            }
            ("optional", None) => (ReliabilityCheckState::Allowed, None, Vec::new()),
            (_, Some(_)) if available => (ReliabilityCheckState::Met, None, Vec::new()),
            ("critical", Some(_)) => {
                state = ReliabilityServiceState::Unavailable;
                (
                    ReliabilityCheckState::Breached,
                    Some(ReliabilityIssueCode::DependencyCriticalUnavailable),
                    vec![format!("restore_dependency:{dependency_id}")],
                )
            }
            ("degradable", Some(_)) => {
                degrade(&mut state);
                if let Some(mode) = contract.degraded_mode_by_dependency.get(dependency_id) {
                    active_degraded_modes.push(ActiveDegradedMode {
                        dependency_id: dependency_id.clone(),
                        mode: mode.clone(),
                        evidence_references: evidence.clone(),
                    });
                }
                (
                    ReliabilityCheckState::Breached,
                    Some(ReliabilityIssueCode::DependencyDegradableUnavailable),
                    vec![format!("operate_degraded_mode:{dependency_id}")],
                )
            }
            ("critical", None) => {
                state = ReliabilityServiceState::Unavailable;
                (
                    ReliabilityCheckState::Unknown,
                    Some(ReliabilityIssueCode::DependencyObservationMissing),
                    vec![format!("observe_dependency:{dependency_id}")],
                )
            }
            ("degradable", None) => {
                degrade(&mut state);
                (
                    ReliabilityCheckState::Unknown,
                    Some(ReliabilityIssueCode::DependencyObservationMissing),
                    vec![format!("observe_dependency:{dependency_id}")],
                )
            }
            _ => (ReliabilityCheckState::Unknown, None, Vec::new()),
        };
        checks.push(ReliabilityCheck {
            code: format!("dependency.{dependency_id}"),
            state: check_state,
            observed,
            expected: json!({"criticality": criticality, "state": "available"}),
            evidence_references: evidence,
            issue_code,
            next_actions,
        });
    }

    push_limit_check(
        &mut checks,
        &mut state,
        "queue_backlog",
        pressure.queue_backlog.as_ref(),
        effective_values.queue_backlog_limit,
        ReliabilityIssueCode::QueueBacklogLimitExceeded,
        "drain_service_queues",
    );
    push_limit_check(
        &mut checks,
        &mut state,
        "workflow_backlog",
        pressure.workflow_backlog.as_ref(),
        effective_values.workflow_backlog_limit,
        ReliabilityIssueCode::WorkflowBacklogLimitExceeded,
        "drain_workflow_backlog",
    );
    push_limit_check(
        &mut checks,
        &mut state,
        "timer_lag_ms",
        pressure.timer_lag_ms.as_ref(),
        effective_values.timer_lag_limit_ms,
        ReliabilityIssueCode::TimerLagLimitExceeded,
        "restore_workflow_timer_processing",
    );
    push_limit_check(
        &mut checks,
        &mut state,
        "retry_exhaustion",
        pressure.retry_exhaustion.as_ref(),
        effective_values.retry_exhaustion_limit,
        ReliabilityIssueCode::RetryExhaustionLimitExceeded,
        "inspect_exhausted_workflow_steps",
    );
    push_limit_check(
        &mut checks,
        &mut state,
        "compensation_pressure",
        pressure.compensation_pressure.as_ref(),
        effective_values.compensation_pressure_limit,
        ReliabilityIssueCode::CompensationPressureLimitExceeded,
        "inspect_workflow_compensations",
    );
    push_target_check(
        &mut checks,
        &mut state,
        "availability_basis_points",
        external.availability_basis_points.as_ref(),
        effective_values.availability_target_basis_points.into(),
        TargetDirection::Minimum,
        ReliabilityIssueCode::AvailabilityTargetBreached,
        "restore_service_availability",
    );
    push_target_check(
        &mut checks,
        &mut state,
        "latency_ms",
        external.latency_ms.as_ref(),
        effective_values.latency_target_ms,
        TargetDirection::Maximum,
        ReliabilityIssueCode::LatencyTargetBreached,
        "reduce_service_latency",
    );
    push_target_check(
        &mut checks,
        &mut state,
        "error_budget_consumed_basis_points",
        external.error_budget_consumed_basis_points.as_ref(),
        effective_values
            .error_budget_consumed_limit_basis_points
            .into(),
        TargetDirection::Maximum,
        ReliabilityIssueCode::ErrorBudgetLimitExceeded,
        "protect_remaining_error_budget",
    );

    if let Some(error) = &collection_issues.source {
        degrade(&mut state);
        checks.push(ReliabilityCheck {
            code: "reliability_observation_source".to_owned(),
            state: ReliabilityCheckState::Unknown,
            observed: json!({"error": error.message}),
            expected: json!("available"),
            evidence_references: error.evidence_references.clone(),
            issue_code: Some(ReliabilityIssueCode::ReliabilityObservationSourceUnavailable),
            next_actions: vec!["restore_reliability_observation_source".to_owned()],
        });
    }
    if let Some(error) = &collection_issues.store {
        degrade(&mut state);
        checks.push(ReliabilityCheck {
            code: "reliability_store_observation".to_owned(),
            state: ReliabilityCheckState::Unknown,
            observed: json!({"error": error}),
            expected: json!("available"),
            evidence_references: vec!["service-store:reliability-pressure".to_owned()],
            issue_code: Some(ReliabilityIssueCode::ReliabilityStoreObservationUnavailable),
            next_actions: vec!["restore_service_store_observation".to_owned()],
        });
    }

    let issue_codes = checks
        .iter()
        .filter_map(|check| check.issue_code)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let liveness_healthy = match effective_values.liveness {
        ReliabilityLivenessSemantics::ProcessRunning => phase != RuntimePhase::Stopped,
        ReliabilityLivenessSemantics::RuntimeOperational => {
            !matches!(phase, RuntimePhase::Failed | RuntimePhase::Stopped)
        }
    };
    let readiness_healthy = runtime_healthy
        && match effective_values.readiness {
            ReliabilityReadinessSemantics::Serving => state != ReliabilityServiceState::Unavailable,
            ReliabilityReadinessSemantics::Healthy => state == ReliabilityServiceState::Healthy,
        };

    ReliabilityReport {
        protocol: RELIABILITY_REPORT_PROTOCOL.to_owned(),
        service_id: service_id.to_owned(),
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version.clone(),
        profile: contract.profile,
        effective_values: effective_values.clone(),
        state,
        liveness: ReliabilityHealthResult {
            healthy: liveness_healthy,
            semantics: serde_json::to_value(effective_values.liveness)
                .expect("liveness semantics serialize")
                .as_str()
                .expect("liveness semantics serialize as a string")
                .to_owned(),
            issue_codes: issue_codes.clone(),
        },
        readiness: ReliabilityHealthResult {
            healthy: readiness_healthy,
            semantics: serde_json::to_value(effective_values.readiness)
                .expect("readiness semantics serialize")
                .as_str()
                .expect("readiness semantics serialize as a string")
                .to_owned(),
            issue_codes,
        },
        active_degraded_modes,
        checks,
        enforcement: ReliabilityEnforcementBoundary::default(),
    }
}

#[derive(Debug, Clone, Copy)]
enum TargetDirection {
    Minimum,
    Maximum,
}

fn push_limit_check(
    checks: &mut Vec<ReliabilityCheck>,
    state: &mut ReliabilityServiceState,
    code: &str,
    observation: Option<&ReliabilityMetricObservation>,
    limit: u64,
    breach_code: ReliabilityIssueCode,
    next_action: &str,
) {
    push_target_check(
        checks,
        state,
        code,
        observation,
        limit,
        TargetDirection::Maximum,
        breach_code,
        next_action,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_target_check(
    checks: &mut Vec<ReliabilityCheck>,
    service_state: &mut ReliabilityServiceState,
    code: &str,
    observation: Option<&ReliabilityMetricObservation>,
    target: u64,
    direction: TargetDirection,
    breach_code: ReliabilityIssueCode,
    next_action: &str,
) {
    let Some(observation) = observation else {
        degrade(service_state);
        checks.push(ReliabilityCheck {
            code: code.to_owned(),
            state: ReliabilityCheckState::Unknown,
            observed: json!("unobserved"),
            expected: target_expectation(target, direction),
            evidence_references: vec![format!("reliability-observer:{code}")],
            issue_code: Some(ReliabilityIssueCode::ReliabilityObservationMissing),
            next_actions: vec![format!("observe_{code}")],
        });
        return;
    };
    let breached = match direction {
        TargetDirection::Minimum => observation.value < target,
        TargetDirection::Maximum => observation.value > target,
    };
    if breached {
        degrade(service_state);
    }
    checks.push(ReliabilityCheck {
        code: code.to_owned(),
        state: if breached {
            ReliabilityCheckState::Breached
        } else {
            ReliabilityCheckState::Met
        },
        observed: json!(observation.value),
        expected: target_expectation(target, direction),
        evidence_references: observation.evidence_references.clone(),
        issue_code: breached.then_some(breach_code),
        next_actions: breached
            .then(|| vec![next_action.to_owned()])
            .unwrap_or_default(),
    });
}

fn target_expectation(target: u64, direction: TargetDirection) -> Value {
    match direction {
        TargetDirection::Minimum => json!({"minimum": target}),
        TargetDirection::Maximum => json!({"maximum": target}),
    }
}

fn degrade(state: &mut ReliabilityServiceState) {
    if *state == ReliabilityServiceState::Healthy {
        *state = ReliabilityServiceState::Degraded;
    }
}

pub(crate) async fn collect_reliability_report(
    state: &ServiceRuntimeState,
) -> Option<ReliabilityReport> {
    let contract = state.reliability_contract.as_deref()?;
    let mut issues = ReliabilityCollectionIssues::default();
    let external = if let Some(source) = &state.reliability_observation_source {
        match source.observe(&state.identity.service_id).await {
            Ok(observations) => observations,
            Err(error) => {
                issues.source = Some(error);
                ReliabilityExternalObservations::default()
            }
        }
    } else {
        ReliabilityExternalObservations::default()
    };
    let pressure = match collect_pressure(state).await {
        Ok(pressure) => pressure,
        Err(error) => {
            issues.store = Some(error.to_string());
            ReliabilityPressureObservations::default()
        }
    };
    Some(evaluate_reliability_with_issues(
        &state.identity.service_id,
        state.phase(),
        state.worker_phase(),
        contract,
        &external,
        &pressure,
        &issues,
    ))
}

async fn collect_pressure(
    state: &ServiceRuntimeState,
) -> Result<ReliabilityPressureObservations, sqlx::Error> {
    let Some(pool) = state.pool.as_ref() else {
        return Ok(ReliabilityPressureObservations::default());
    };
    let now = state.workflow_clock.now();
    let (queue_backlog, workflow_backlog, retry_exhaustion, compensation_pressure, oldest_due_at): (
        i64,
        i64,
        i64,
        i64,
        Option<DateTime<Utc>>,
    ) = sqlx::query_as(
        r#"
        select
            (
                select count(*) from platform.outbox
                where status in ('pending', 'processing', 'failed')
            ) + (
                select count(*) from runtime.function_runs
                where status in ('pending', 'processing', 'failed')
            ) as queue_backlog,
            (
                select count(*) from platform.service_workflow_instances
                where state in ('running', 'compensating')
            ) as workflow_backlog,
            (
                select count(*) from platform.service_workflow_steps
                where state = 'exhausted'
            ) as retry_exhaustion,
            (
                select count(*) from platform.service_workflow_compensations
                where state in ('pending', 'dispatched', 'failed')
            ) as compensation_pressure,
            (
                select min(due_at) from platform.service_workflow_timers
                where state in ('pending', 'claimed') and due_at < $1
            ) as oldest_due_at
        "#,
    )
    .bind(now)
    .fetch_one(pool)
    .await?;
    let timer_lag_ms = oldest_due_at.map_or(0, |due_at| {
        u64::try_from((now - due_at).num_milliseconds().max(0)).unwrap_or(u64::MAX)
    });
    Ok(ReliabilityPressureObservations {
        queue_backlog: Some(ReliabilityMetricObservation::new(
            u64::try_from(queue_backlog).unwrap_or(u64::MAX),
            vec![
                "service-store:platform.outbox".to_owned(),
                "service-store:runtime.function_runs".to_owned(),
            ],
        )),
        workflow_backlog: Some(ReliabilityMetricObservation::new(
            u64::try_from(workflow_backlog).unwrap_or(u64::MAX),
            vec!["service-store:platform.service_workflow_instances".to_owned()],
        )),
        timer_lag_ms: Some(ReliabilityMetricObservation::new(
            timer_lag_ms,
            vec!["service-store:platform.service_workflow_timers".to_owned()],
        )),
        retry_exhaustion: Some(ReliabilityMetricObservation::new(
            u64::try_from(retry_exhaustion).unwrap_or(u64::MAX),
            vec!["service-store:platform.service_workflow_steps".to_owned()],
        )),
        compensation_pressure: Some(ReliabilityMetricObservation::new(
            u64::try_from(compensation_pressure).unwrap_or(u64::MAX),
            vec!["service-store:platform.service_workflow_compensations".to_owned()],
        )),
    })
}

#[cfg(test)]
mod tests {
    use lenso_service::{
        ReliabilityContract, ReliabilityLivenessSemantics, ReliabilityProfile,
        ReliabilityProfileOverrides, ReliabilityReadinessSemantics, SchemaArtifactReference,
    };

    use super::*;

    fn contract(profile: ReliabilityProfile) -> ReliabilityContract {
        let mut contract = ReliabilityContract::new(
            "support-reliability",
            "v1",
            SchemaArtifactReference::new("contracts/reliability/support.v1.schema.json"),
            "99.9%",
            "43m per 30d",
        );
        contract.profile = profile;
        contract.latency_target_ms = 300;
        contract.backlog_limit = 10;
        contract.overrides = ReliabilityProfileOverrides {
            workflow_backlog_limit: Some(5),
            timer_lag_limit_ms: Some(1_000),
            retry_exhaustion_limit: Some(1),
            compensation_pressure_limit: Some(1),
            ..ReliabilityProfileOverrides::default()
        };
        contract.dependency_criticality = BTreeMap::from([
            ("database".to_owned(), "critical".to_owned()),
            ("notification-gateway".to_owned(), "degradable".to_owned()),
            ("analytics".to_owned(), "optional".to_owned()),
        ]);
        contract.degraded_modes = vec!["queue notifications".to_owned()];
        contract.degraded_mode_by_dependency = BTreeMap::from([(
            "notification-gateway".to_owned(),
            "queue_notifications".to_owned(),
        )]);
        contract
    }

    fn metric(value: u64, evidence: &str) -> ReliabilityMetricObservation {
        ReliabilityMetricObservation::new(value, vec![evidence.to_owned()])
    }

    #[test]
    fn reports_dependency_modes_workflow_pressure_and_slo_breaches_deterministically() {
        let contract = contract(ReliabilityProfile::Critical);
        let external = ReliabilityExternalObservations {
            observed_at: Some(Utc::now()),
            dependencies: BTreeMap::from([
                (
                    "database".to_owned(),
                    ReliabilityDependencyObservation::new(
                        ReliabilityDependencyState::Available,
                        vec!["probe:database".to_owned()],
                    ),
                ),
                (
                    "notification-gateway".to_owned(),
                    ReliabilityDependencyObservation::new(
                        ReliabilityDependencyState::Unavailable,
                        vec!["probe:notification-gateway".to_owned()],
                    ),
                ),
                (
                    "analytics".to_owned(),
                    ReliabilityDependencyObservation::new(
                        ReliabilityDependencyState::Unavailable,
                        vec!["probe:analytics".to_owned()],
                    ),
                ),
            ]),
            availability_basis_points: Some(metric(9_980, "slo:availability:30d")),
            latency_ms: Some(metric(450, "slo:latency:p99:5m")),
            error_budget_consumed_basis_points: Some(metric(8_500, "slo:error-budget:30d")),
        };
        let pressure = ReliabilityPressureObservations {
            queue_backlog: Some(metric(11, "store:queue")),
            workflow_backlog: Some(metric(6, "store:workflows")),
            timer_lag_ms: Some(metric(1_001, "store:timers")),
            retry_exhaustion: Some(metric(2, "store:retries")),
            compensation_pressure: Some(metric(2, "store:compensations")),
        };

        let first = evaluate_reliability(
            "support",
            RuntimePhase::Ready,
            RuntimePhase::Ready,
            &contract,
            &external,
            &pressure,
        );
        let second = evaluate_reliability(
            "support",
            RuntimePhase::Ready,
            RuntimePhase::Ready,
            &contract,
            &external,
            &pressure,
        );

        assert_eq!(first, second);
        assert_eq!(first.state, ReliabilityServiceState::Degraded);
        assert!(
            !first.readiness.healthy,
            "critical profile requires healthy"
        );
        assert!(first.liveness.healthy);
        assert_eq!(
            first.active_degraded_modes,
            vec![ActiveDegradedMode {
                dependency_id: "notification-gateway".to_owned(),
                mode: "queue_notifications".to_owned(),
                evidence_references: vec!["probe:notification-gateway".to_owned()],
            }]
        );
        let issue_codes = first
            .checks
            .iter()
            .filter_map(|check| check.issue_code)
            .collect::<BTreeSet<_>>();
        assert!(issue_codes.contains(&ReliabilityIssueCode::DependencyDegradableUnavailable));
        assert!(issue_codes.contains(&ReliabilityIssueCode::QueueBacklogLimitExceeded));
        assert!(issue_codes.contains(&ReliabilityIssueCode::WorkflowBacklogLimitExceeded));
        assert!(issue_codes.contains(&ReliabilityIssueCode::TimerLagLimitExceeded));
        assert!(issue_codes.contains(&ReliabilityIssueCode::RetryExhaustionLimitExceeded));
        assert!(issue_codes.contains(&ReliabilityIssueCode::CompensationPressureLimitExceeded));
        assert!(issue_codes.contains(&ReliabilityIssueCode::AvailabilityTargetBreached));
        assert!(issue_codes.contains(&ReliabilityIssueCode::LatencyTargetBreached));
        assert!(issue_codes.contains(&ReliabilityIssueCode::ErrorBudgetLimitExceeded));
        assert_eq!(
            first
                .checks
                .iter()
                .find(|check| check.code == "dependency.analytics")
                .unwrap()
                .state,
            ReliabilityCheckState::Allowed
        );
        assert!(first.enforcement.reports_only);
        assert!(!first.enforcement.blocks_production_promotion);
        assert!(!first.enforcement.executes_canary_policy);
        assert!(!first.enforcement.triggers_automated_rollback);
    }

    #[test]
    fn critical_dependency_and_declared_health_semantics_control_health_results() {
        let mut contract = contract(ReliabilityProfile::Development);
        contract.overrides.readiness = Some(ReliabilityReadinessSemantics::Serving);
        contract.overrides.liveness = Some(ReliabilityLivenessSemantics::ProcessRunning);
        let mut external = ReliabilityExternalObservations {
            dependencies: BTreeMap::from([
                (
                    "database".to_owned(),
                    ReliabilityDependencyObservation::new(
                        ReliabilityDependencyState::Unavailable,
                        vec!["probe:database".to_owned()],
                    ),
                ),
                (
                    "notification-gateway".to_owned(),
                    ReliabilityDependencyObservation::new(
                        ReliabilityDependencyState::Available,
                        Vec::new(),
                    ),
                ),
            ]),
            availability_basis_points: Some(metric(10_000, "slo:availability")),
            latency_ms: Some(metric(1, "slo:latency")),
            error_budget_consumed_basis_points: Some(metric(0, "slo:error-budget")),
            ..ReliabilityExternalObservations::default()
        };
        external.dependencies.insert(
            "analytics".to_owned(),
            ReliabilityDependencyObservation::new(
                ReliabilityDependencyState::Available,
                Vec::new(),
            ),
        );
        let pressure = ReliabilityPressureObservations {
            queue_backlog: Some(metric(0, "store:queue")),
            workflow_backlog: Some(metric(0, "store:workflows")),
            timer_lag_ms: Some(metric(0, "store:timers")),
            retry_exhaustion: Some(metric(0, "store:retries")),
            compensation_pressure: Some(metric(0, "store:compensations")),
        };

        let unavailable = evaluate_reliability(
            "support",
            RuntimePhase::Ready,
            RuntimePhase::Ready,
            &contract,
            &external,
            &pressure,
        );
        assert_eq!(unavailable.state, ReliabilityServiceState::Unavailable);
        assert!(!unavailable.readiness.healthy);

        let failed_process = evaluate_reliability(
            "support",
            RuntimePhase::Failed,
            RuntimePhase::Failed,
            &contract,
            &external,
            &pressure,
        );
        assert!(
            failed_process.liveness.healthy,
            "process_running semantics remain live until the process stops"
        );

        contract.overrides.liveness = Some(ReliabilityLivenessSemantics::RuntimeOperational);
        let failed_runtime = evaluate_reliability(
            "support",
            RuntimePhase::Failed,
            RuntimePhase::Failed,
            &contract,
            &external,
            &pressure,
        );
        assert!(!failed_runtime.liveness.healthy);
    }
}
