use crate::{EffectiveReliabilityValues, ReliabilityProfile, ReliabilityProfileOverrides};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

pub const RELIABILITY_REPORT_PROTOCOL: &str = "lenso.reliability-report.v1";

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDegradedMode {
    pub dependency_id: String,
    pub mode: String,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityHealthResult {
    pub healthy: bool,
    pub semantics: String,
    pub issue_codes: Vec<ReliabilityIssueCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityReport {
    pub protocol: String,
    pub service_id: String,
    pub contract_id: String,
    pub contract_version: String,
    pub profile: ReliabilityProfile,
    #[serde(default)]
    pub overrides: ReliabilityProfileOverrides,
    pub effective_values: EffectiveReliabilityValues,
    pub state: ReliabilityServiceState,
    pub liveness: ReliabilityHealthResult,
    pub readiness: ReliabilityHealthResult,
    pub active_degraded_modes: Vec<ActiveDegradedMode>,
    pub checks: Vec<ReliabilityCheck>,
    pub enforcement: ReliabilityEnforcementBoundary,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v1_report_without_explicit_overrides_remains_readable() {
        let report = serde_json::from_value::<ReliabilityReport>(json!({
            "protocol": RELIABILITY_REPORT_PROTOCOL,
            "serviceId": "support-sla",
            "contractId": "support-reliability",
            "contractVersion": "v1",
            "profile": "standard",
            "effectiveValues": {
                "availabilityTargetBasisPoints": 9950,
                "latencyTargetMs": 1000,
                "queueBacklogLimit": 100,
                "workflowBacklogLimit": 100,
                "timerLagLimitMs": 1000,
                "retryExhaustionLimit": 5,
                "compensationPressureLimit": 5,
                "errorBudget": "rolling_30d",
                "errorBudgetConsumedLimitBasisPoints": 10000,
                "readiness": "serving",
                "liveness": "process_running"
            },
            "state": "healthy",
            "liveness": {
                "healthy": true,
                "semantics": "process_running",
                "issueCodes": []
            },
            "readiness": {
                "healthy": true,
                "semantics": "serving",
                "issueCodes": []
            },
            "activeDegradedModes": [],
            "checks": [],
            "enforcement": {
                "reportsOnly": true,
                "blocksProductionPromotion": false,
                "executesCanaryPolicy": false,
                "triggersAutomatedRollback": false
            }
        }))
        .expect("legacy v1 report should deserialize");

        assert_eq!(report.overrides, ReliabilityProfileOverrides::default());
    }
}
