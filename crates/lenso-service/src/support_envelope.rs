use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;

pub const SUPPORT_ENVELOPE_PROTOCOL: &str = "lenso.support-envelope.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SupportEnvelopeDecision {
    Passed,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SupportEnvelopeIssueCode {
    ScalePointMissing,
    TopologyInvalid,
    MeasurementIncomplete,
    BudgetExceeded,
    EnvironmentDrift,
    SaturationUnknown,
    HiddenCentralDependency,
    CleanupIncomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupportEnvelopeIssue {
    pub code: SupportEnvelopeIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupportScalePoint {
    pub service_count: u32,
    pub workload_count: u32,
    pub store_count: u32,
    pub contract_count: u32,
    pub workflow_count: u32,
    pub tenant_count: u32,
    pub topology_digest: String,
    pub environment_digest: String,
    pub compatible_baseline_digest: String,
    pub environment_verification: bool,
    pub environment_drift_detected: bool,
    pub measurement_digests: BTreeMap<String, String>,
    pub budgets_passed: bool,
    pub repeated_run_count: u32,
    pub variance_basis_points: u32,
    pub system_plane_data_plane_requests: u64,
    pub runtime_console_data_plane_requests: u64,
    pub telemetry_data_plane_requests: u64,
    pub policy_data_plane_requests: u64,
    pub registry_data_plane_requests: u64,
    pub saturation_signal: String,
    pub bottlenecks: Vec<String>,
    pub cleanup_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupportEnvelopeInput {
    pub support_manifest_digest: String,
    pub adapter_versions: BTreeMap<String, String>,
    pub points: Vec<SupportScalePoint>,
    pub recommended_service_limit: u32,
    pub variance_tolerance_basis_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupportEnvelope {
    pub protocol: String,
    pub envelope_id: String,
    pub envelope_digest: String,
    pub support_manifest_digest: String,
    pub adapter_versions: BTreeMap<String, String>,
    pub points: Vec<SupportScalePoint>,
    pub recommended_service_limit: u32,
    pub decision: SupportEnvelopeDecision,
    pub issues: Vec<SupportEnvelopeIssue>,
    pub next_actions: Vec<String>,
}

#[must_use]
pub fn evaluate_support_envelope(mut input: SupportEnvelopeInput) -> SupportEnvelope {
    input.points.sort_by_key(|point| point.service_count);
    let mut issues = Vec::new();
    let counts = input
        .points
        .iter()
        .map(|point| point.service_count)
        .collect::<BTreeSet<_>>();
    if counts != BTreeSet::from([3, 10, 20]) {
        issues.push(issue(
            SupportEnvelopeIssueCode::ScalePointMissing,
            "Support evidence must include the three-, ten-, and twenty-Service scale points.",
            "Run the same pinned profile at all declared support points.",
            "Collect the missing scale point before publishing the envelope.",
        ));
    }
    if input.recommended_service_limit != 20
        || !valid_digest(&input.support_manifest_digest)
        || input.adapter_versions.is_empty()
        || input.variance_tolerance_basis_points == 0
    {
        issues.push(issue(
            SupportEnvelopeIssueCode::TopologyInvalid,
            "Support envelope metadata does not describe the bounded 3–20 Service product scope.",
            "Bind the envelope to exact adapters and the reviewed M6 limit.",
            "Correct the envelope metadata.",
        ));
    }
    for point in &input.points {
        if point.workload_count < point.service_count
            || point.store_count != point.service_count
            || point.contract_count < point.service_count
            || point.workflow_count == 0
            || point.tenant_count == 0
            || !valid_digest(&point.topology_digest)
            || !valid_digest(&point.environment_digest)
            || !valid_digest(&point.compatible_baseline_digest)
            || !point.environment_verification
        {
            issues.push(issue(
                SupportEnvelopeIssueCode::TopologyInvalid,
                format!(
                    "The {}-Service point does not represent distinct Service workloads and Stores.",
                    point.service_count
                ),
                "Scale logical Services, background work, tenants, Contracts, and Workflows together.",
                "Correct the scale fixture and repeat the profile.",
            ));
        }
        let required_measurements = [
            "startup",
            "rollout",
            "direct_calls",
            "events",
            "inbox_outbox",
            "workflows",
            "timers",
            "compensation",
            "story_federation",
            "policy",
            "console",
            "failure_recovery",
            "connections",
            "backlog",
            "resources",
            "evidence_freshness",
        ];
        if required_measurements
            .iter()
            .any(|name| !point.measurement_digests.contains_key(*name))
            || point
                .measurement_digests
                .values()
                .any(|digest| !valid_digest(digest))
            || point.repeated_run_count < 3
            || point.variance_basis_points > input.variance_tolerance_basis_points
        {
            issues.push(issue(
                SupportEnvelopeIssueCode::MeasurementIncomplete,
                format!(
                    "The {}-Service point lacks complete repeated evidence.",
                    point.service_count
                ),
                "Record load, resource, failure, recovery, and convergence evidence with variance.",
                "Repeat the pinned environment runs.",
            ));
        }
        if point.environment_drift_detected {
            issues.push(issue(
                SupportEnvelopeIssueCode::EnvironmentDrift,
                format!(
                    "The {}-Service point drifted from its compatible pinned baseline.",
                    point.service_count
                ),
                "Report infrastructure drift separately from product budget regressions.",
                "Restore the pinned environment and repeat the profile.",
            ));
        }
        if !point.budgets_passed {
            issues.push(issue(
                SupportEnvelopeIssueCode::BudgetExceeded,
                format!(
                    "The {}-Service point exceeded its reviewed budget.",
                    point.service_count
                ),
                "Keep the result environment-specific and diagnose the limiting resource.",
                "Correct the bottleneck or lower the reviewed support limit.",
            ));
        }
        if point.saturation_signal.trim().is_empty() || point.bottlenecks.is_empty() {
            issues.push(issue(
                SupportEnvelopeIssueCode::SaturationUnknown,
                format!(
                    "The {}-Service point does not identify saturation or bottlenecks.",
                    point.service_count
                ),
                "State which resource limits the point and how it is observed.",
                "Add the evidence-backed saturation signal.",
            ));
        }
        if point.system_plane_data_plane_requests > 0
            || point.runtime_console_data_plane_requests > 0
            || point.telemetry_data_plane_requests > 0
            || point.policy_data_plane_requests > 0
            || point.registry_data_plane_requests > 0
        {
            issues.push(issue(
                SupportEnvelopeIssueCode::HiddenCentralDependency,
                "A scale point depends on a central coordination or observability service for established traffic.",
                "Keep those surfaces outside the Data Plane after convergence.",
                "Withhold the central surfaces and repeat the scale point.",
            ));
        }
        if !point.cleanup_complete {
            issues.push(issue(
                SupportEnvelopeIssueCode::CleanupIncomplete,
                "Scale-point cleanup is incomplete.",
                "Remove or isolate disposable Stores, streams, identities, and Workloads.",
                "Finish cleanup before accepting the envelope.",
            ));
        }
    }
    let decision = if issues.is_empty() {
        SupportEnvelopeDecision::Passed
    } else {
        SupportEnvelopeDecision::Blocked
    };
    let next_actions = if issues.is_empty() {
        vec!["Publish the observed 3–20 Service envelope as a bounded GA claim.".into()]
    } else {
        issues
            .iter()
            .flat_map(|issue| issue.next_actions.iter().cloned())
            .collect()
    };
    let mut envelope = SupportEnvelope {
        protocol: SUPPORT_ENVELOPE_PROTOCOL.to_owned(),
        envelope_id: String::new(),
        envelope_digest: String::new(),
        support_manifest_digest: input.support_manifest_digest,
        adapter_versions: input.adapter_versions,
        points: input.points,
        recommended_service_limit: input.recommended_service_limit,
        decision,
        issues,
        next_actions,
    };
    envelope.envelope_digest = digest_without_identity(&envelope);
    envelope.envelope_id = format!("support-envelope:{}", &envelope.envelope_digest[7..23]);
    envelope
}

#[must_use]
pub fn support_envelope_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(SupportEnvelope))
        .expect("support envelope schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/ga/lenso.support-envelope.v1.schema.json".to_owned(),
    );
    schema
}

fn issue(
    code: SupportEnvelopeIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> SupportEnvelopeIssue {
    SupportEnvelopeIssue {
        code,
        message: message.into(),
        remediation: remediation.into(),
        next_actions: vec![next_action.into()],
    }
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

fn digest_without_identity(envelope: &SupportEnvelope) -> String {
    let mut canonical = envelope.clone();
    canonical.envelope_id.clear();
    canonical.envelope_digest.clear();
    extraction_input_digest(&serde_json::to_vec(&canonical).expect("support envelope serializes"))
}

#[must_use]
pub fn support_envelope_integrity_is_valid(envelope: &SupportEnvelope) -> bool {
    valid_digest(&envelope.envelope_digest)
        && envelope.envelope_digest == digest_without_identity(envelope)
        && envelope.envelope_id == format!("support-envelope:{}", &envelope.envelope_digest[7..23])
}
