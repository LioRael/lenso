use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::Resource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    LensoAutonomousService, LensoAutonomousServiceCondition, LensoAutonomousServiceState,
    LensoAutonomousServiceStatus, OperatorDeliveryIssue, OperatorWorkloadRole,
    OperatorWorkloadStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousWorkloadObservation {
    pub workload_id: String,
    pub ready: bool,
    pub failed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_release_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_config_revision_id: Option<String>,
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousServiceObservation {
    #[serde(default)]
    pub migrations: Vec<AutonomousWorkloadObservation>,
    #[serde(default)]
    pub workloads: Vec<AutonomousWorkloadObservation>,
    pub fresh: bool,
}

#[must_use]
pub fn observed_autonomous_service_status(
    service: &LensoAutonomousService,
    observation: &AutonomousServiceObservation,
) -> LensoAutonomousServiceStatus {
    let migrations = observation
        .migrations
        .iter()
        .map(|item| (item.workload_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let workloads = observation
        .workloads
        .iter()
        .map(|item| (item.workload_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let statuses = service
        .spec
        .workloads
        .iter()
        .map(|workload| {
            let observed = if workload.role == OperatorWorkloadRole::Migration {
                migrations.get(workload.workload_id.as_str()).copied()
            } else {
                workloads.get(workload.workload_id.as_str()).copied()
            };
            let desired_digest = workload
                .image
                .rsplit_once('@')
                .map_or_else(String::new, |(_, digest)| digest.to_owned());
            OperatorWorkloadStatus {
                workload_id: workload.workload_id.clone(),
                role: workload.role,
                desired_digest: desired_digest.clone(),
                observed_digest: observed.and_then(|item| item.observed_digest.clone()),
                ready: observed.is_some_and(|item| item.ready),
                failed: observed.is_some_and(|item| item.failed),
            }
        })
        .collect::<Vec<_>>();
    let migration_statuses = statuses
        .iter()
        .filter(|status| status.role == OperatorWorkloadRole::Migration)
        .collect::<Vec<_>>();
    let dependent_statuses = statuses
        .iter()
        .filter(|status| status.role != OperatorWorkloadRole::Migration)
        .collect::<Vec<_>>();
    let drifted = statuses
        .iter()
        .any(|status| status.observed_digest.as_deref() != Some(status.desired_digest.as_str()));
    let all_observations = observation
        .migrations
        .iter()
        .chain(observation.workloads.iter())
        .collect::<Vec<_>>();
    let observed_release_id = consistent_observed_value(
        all_observations
            .iter()
            .map(|item| item.observed_release_id.as_deref()),
    );
    let observed_release_digest = consistent_observed_value(
        all_observations
            .iter()
            .map(|item| item.observed_release_digest.as_deref()),
    );
    // Migration Jobs are immutable release receipts. A later config-only rollout must not
    // invalidate or patch the completed Job, so config convergence is established by the
    // long-running API/Worker observations only.
    let observed_config_revision_id = consistent_observed_value(
        observation
            .workloads
            .iter()
            .map(|item| item.observed_config_revision_id.as_deref()),
    );
    let identity_drifted = observed_release_id.as_deref() != Some(service.spec.release_id.as_str())
        || observed_release_digest.as_deref() != Some(service.spec.release_digest.as_str())
        || observed_config_revision_id.as_deref() != Some(service.spec.config_revision_id.as_str());
    let drifted = drifted || identity_drifted;
    let (state, phase, reason, message, issue) =
        if migration_statuses.iter().any(|status| status.failed) {
            (
                LensoAutonomousServiceState::Failed,
                "migration_failed",
                "MigrationFailed",
                "A Migration Workload failed; dependent Workloads remain blocked.",
                Some(operator_issue(
                    "migration_failed",
                    "A release-bound Migration Workload failed.",
                    "Correct the Migration artifact without bypassing release identity.",
                    "Publish a corrected Service Release or choose a verified rollback target.",
                )),
            )
        } else if migration_statuses.iter().any(|status| !status.ready) {
            (
                LensoAutonomousServiceState::Migrating,
                "migrating",
                "MigrationIncomplete",
                "Migration Workloads must complete before API and Worker convergence.",
                Some(operator_issue(
                    "migration_incomplete",
                    "Migration completion has not been observed for this release.",
                    "Keep dependent Workloads gated until the release-bound Job succeeds.",
                    "Inspect the Migration Job and wait for a fresh Operator observation.",
                )),
            )
        } else if !observation.fresh || drifted {
            (
                LensoAutonomousServiceState::Progressing,
                "observing",
                "ObservationStaleOrDrifted",
                "Operator observations are stale or differ from the desired release.",
                Some(operator_issue(
                    "observation_stale",
                    "Observed Workload state is stale or differs from the desired release digest.",
                    "Reconcile only the resources owned by this Autonomous Service.",
                    "Refresh Operator observations and inspect the reported Workload digest.",
                )),
            )
        } else if dependent_statuses.iter().all(|status| status.ready)
            && !dependent_statuses.is_empty()
        {
            (
                LensoAutonomousServiceState::Ready,
                "ready",
                "WorkloadsReady",
                "Migration, API, and Worker Workloads match the Service Release.",
                None,
            )
        } else {
            (
                LensoAutonomousServiceState::Progressing,
                "converging",
                "WorkloadsProgressing",
                "Dependent Workloads are converging after Migration completion.",
                None,
            )
        };
    let issues = issue.into_iter().collect::<Vec<_>>();
    let next_actions = issues
        .iter()
        .flat_map(|issue| issue.next_actions.iter().cloned())
        .collect();

    LensoAutonomousServiceStatus {
        state,
        observed_generation: service.meta().generation,
        observed_release_id: observed_release_id.unwrap_or_else(|| "unknown".to_owned()),
        observed_release_digest: observed_release_digest.unwrap_or_else(|| "unknown".to_owned()),
        config_revision_id: observed_config_revision_id.unwrap_or_else(|| "unknown".to_owned()),
        rollout_phase: phase.to_owned(),
        policy_evidence_references: service.spec.policy_evidence_references.clone(),
        evidence_references: service.spec.evidence_references.clone(),
        workloads: statuses,
        drifted,
        rollback_state: service
            .spec
            .rollback_release_id
            .as_ref()
            .map_or_else(|| "unavailable".to_owned(), |_| "available".to_owned()),
        issues,
        next_actions,
        conditions: vec![LensoAutonomousServiceCondition {
            type_: "Ready".to_owned(),
            status: if state == LensoAutonomousServiceState::Ready {
                "True".to_owned()
            } else {
                "False".to_owned()
            },
            reason: reason.to_owned(),
            message: message.to_owned(),
            last_transition_time: Time(k8s_openapi::jiff::Timestamp::now()),
        }],
    }
}

fn consistent_observed_value<'a>(values: impl Iterator<Item = Option<&'a str>>) -> Option<String> {
    let mut values = values;
    let first = values.next().flatten()?;
    if values.all(|value| value == Some(first)) {
        Some(first.to_owned())
    } else {
        None
    }
}

fn operator_issue(
    code: &str,
    message: &str,
    remediation: &str,
    next_action: &str,
) -> OperatorDeliveryIssue {
    OperatorDeliveryIssue {
        code: code.to_owned(),
        message: message.to_owned(),
        evidence_references: Vec::new(),
        remediation: remediation.to_owned(),
        next_actions: vec![next_action.to_owned()],
    }
}
