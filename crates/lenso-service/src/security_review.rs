use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;

pub const SECURITY_REVIEW_PROTOCOL: &str = "lenso.security-review-evidence.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ThreatSurface {
    WorkloadIdentity,
    TransportBinding,
    Delegation,
    Tenancy,
    EventReplayAndPoisoning,
    ExtractionAndCutover,
    WorkflowControls,
    ReleaseSigning,
    Secrets,
    BackupAndRestore,
    AdminActions,
    EmbeddedConsole,
    PolicyBypass,
    StaleEvidence,
    AgentBoundaries,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecuritySeverity {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FindingDisposition {
    Open,
    Remediated,
    AcceptedRisk,
    FalsePositive,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecurityReviewDecision {
    Passed,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecurityReviewIssueCode {
    ThreatModelIncomplete,
    ReleaseSubjectInvalid,
    FindingUnresolved,
    RiskAcceptanceInvalid,
    ScanEvidenceIncomplete,
    ReviewStale,
    SensitiveMaterialPresent,
}

impl SecurityReviewIssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThreatModelIncomplete => "security_threat_model_incomplete",
            Self::ReleaseSubjectInvalid => "security_release_subject_invalid",
            Self::FindingUnresolved => "security_finding_unresolved",
            Self::RiskAcceptanceInvalid => "security_risk_acceptance_invalid",
            Self::ScanEvidenceIncomplete => "security_scan_evidence_incomplete",
            Self::ReviewStale => "security_review_stale",
            Self::SensitiveMaterialPresent => "security_sensitive_material_present",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityReviewIssue {
    pub code: SecurityReviewIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThreatModelEvidence {
    pub surface: ThreatSurface,
    pub model_version: String,
    pub model_digest: String,
    pub reviewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityReleaseSubject {
    pub component_id: String,
    pub version: String,
    pub source_commit: String,
    pub artifact_digest: String,
    pub provenance_digest: String,
    pub sbom_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RiskAcceptance {
    pub approver: String,
    pub reason: String,
    pub expires_at_unix_ms: u64,
    pub finding_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityFinding {
    pub finding_id: String,
    pub finding_digest: String,
    pub severity: SecuritySeverity,
    pub surface: ThreatSurface,
    pub affected_subject_digests: Vec<String>,
    pub owner: String,
    pub disposition: FindingDisposition,
    pub remediation_reference: Option<String>,
    pub risk_acceptance: Option<RiskAcceptance>,
    pub contains_sensitive_material: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScanEvidence {
    pub scanner_id: String,
    pub scanner_version: String,
    pub subject_digest: String,
    pub result_digest: String,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityReviewInput {
    pub support_manifest_digest: String,
    pub release_subjects: Vec<SecurityReleaseSubject>,
    pub threat_models: Vec<ThreatModelEvidence>,
    pub findings: Vec<SecurityFinding>,
    pub scans: Vec<SecurityScanEvidence>,
    pub reviewer: String,
    pub reviewed_at_unix_ms: u64,
    pub freshness_horizon_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecurityReviewEvidence {
    pub protocol: String,
    pub review_id: String,
    pub review_digest: String,
    pub support_manifest_digest: String,
    pub release_subjects: Vec<SecurityReleaseSubject>,
    pub threat_models: Vec<ThreatModelEvidence>,
    pub findings: Vec<SecurityFinding>,
    pub scans: Vec<SecurityScanEvidence>,
    pub reviewer: String,
    pub reviewed_at_unix_ms: u64,
    pub freshness_horizon_unix_ms: u64,
    pub decision: SecurityReviewDecision,
    pub issues: Vec<SecurityReviewIssue>,
    pub next_actions: Vec<String>,
}

#[must_use]
pub fn evaluate_security_review(
    mut input: SecurityReviewInput,
    observed_at_unix_ms: u64,
) -> SecurityReviewEvidence {
    input
        .release_subjects
        .sort_by(|left, right| left.component_id.cmp(&right.component_id));
    input.threat_models.sort_by_key(|evidence| evidence.surface);
    input
        .findings
        .sort_by(|left, right| left.finding_id.cmp(&right.finding_id));
    input
        .scans
        .sort_by(|left, right| left.scanner_id.cmp(&right.scanner_id));

    let mut issues = Vec::new();
    let required_surfaces = all_threat_surfaces();
    let observed_surfaces = input
        .threat_models
        .iter()
        .filter(|model| model.reviewed && valid_digest(&model.model_digest))
        .map(|model| model.surface)
        .collect::<BTreeSet<_>>();
    if observed_surfaces != required_surfaces
        || input.threat_models.len() != required_surfaces.len()
    {
        issues.push(issue(
            SecurityReviewIssueCode::ThreatModelIncomplete,
            "The versioned threat model does not cover every M6 security surface.",
            "Review every declared surface and bind it to immutable model evidence.",
            "Complete the missing threat-model review.",
        ));
    }

    let release_digests = input
        .release_subjects
        .iter()
        .map(|subject| subject.artifact_digest.as_str())
        .collect::<BTreeSet<_>>();
    if input.release_subjects.is_empty()
        || !valid_digest(&input.support_manifest_digest)
        || input.release_subjects.iter().any(|subject| {
            subject.component_id.trim().is_empty()
                || subject.version.trim().is_empty()
                || subject.source_commit.len() != 40
                || !subject
                    .source_commit
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || !valid_digest(&subject.artifact_digest)
                || !valid_digest(&subject.provenance_digest)
                || !valid_digest(&subject.sbom_digest)
        })
        || release_digests.len() != input.release_subjects.len()
    {
        issues.push(issue(
            SecurityReviewIssueCode::ReleaseSubjectInvalid,
            "Security review subjects are missing exact version, source, artifact, provenance, or SBOM identity.",
            "Review the immutable release subjects from the GA Support Manifest.",
            "Refresh the exact release subject set.",
        ));
    }

    for finding in &input.findings {
        let subject_set_valid = !finding.affected_subject_digests.is_empty()
            && finding
                .affected_subject_digests
                .iter()
                .all(|digest| release_digests.contains(digest.as_str()));
        if finding.finding_id.trim().is_empty()
            || !valid_digest(&finding.finding_digest)
            || finding.owner.trim().is_empty()
            || !subject_set_valid
        {
            issues.push(issue(
                SecurityReviewIssueCode::ReleaseSubjectInvalid,
                format!(
                    "Finding `{}` is not bound to exact release subjects.",
                    finding.finding_id
                ),
                "Bind every finding to stable identity, owner, surface, and artifact subjects.",
                "Correct the finding record.",
            ));
        }
        if matches!(
            finding.severity,
            SecuritySeverity::Critical | SecuritySeverity::High
        ) && finding.disposition == FindingDisposition::Open
        {
            issues.push(issue(
                SecurityReviewIssueCode::FindingUnresolved,
                format!("Finding `{}` remains unresolved.", finding.finding_id),
                "Remediate or explicitly accept the exact reviewed risk before GA.",
                "Block the component release until the finding is disposed.",
            ));
        }
        if finding.disposition == FindingDisposition::AcceptedRisk {
            let valid_acceptance = finding.risk_acceptance.as_ref().is_some_and(|acceptance| {
                acceptance.finding_digest == finding.finding_digest
                    && !acceptance.approver.trim().is_empty()
                    && !acceptance.reason.trim().is_empty()
                    && acceptance.expires_at_unix_ms > observed_at_unix_ms
            });
            if !valid_acceptance {
                issues.push(issue(
                    SecurityReviewIssueCode::RiskAcceptanceInvalid,
                    format!(
                        "Finding `{}` lacks current exact risk acceptance.",
                        finding.finding_id
                    ),
                    "Bind named approval and expiry to the finding digest.",
                    "Renew or remove the invalid acceptance.",
                ));
            }
        }
        if finding.contains_sensitive_material {
            issues.push(issue(
                SecurityReviewIssueCode::SensitiveMaterialPresent,
                format!(
                    "Finding `{}` contains sensitive material.",
                    finding.finding_id
                ),
                "Store only redacted evidence and protected references.",
                "Remove the sensitive content and rotate it if exposed.",
            ));
        }
    }

    let required_scanners = BTreeSet::from([
        "dependency-audit",
        "provenance-verification",
        "secret-scan",
        "static-analysis",
    ]);
    let scans_complete = required_scanners.iter().all(|scanner_id| {
        release_digests.iter().all(|release_digest| {
            input.scans.iter().any(|scan| {
                scan.scanner_id == *scanner_id
                    && scan.subject_digest == *release_digest
                    && scan.completed
                    && valid_digest(&scan.result_digest)
                    && !scan.scanner_version.trim().is_empty()
            })
        })
    });
    if !scans_complete {
        issues.push(issue(
            SecurityReviewIssueCode::ScanEvidenceIncomplete,
            "Required dependency, provenance, secret, or static-analysis evidence is missing.",
            "Run the versioned scanners against the exact release subject set.",
            "Collect the missing scan result.",
        ));
    }
    if input.reviewer.trim().is_empty()
        || input.reviewed_at_unix_ms == 0
        || input.freshness_horizon_unix_ms < observed_at_unix_ms
        || input.reviewed_at_unix_ms > observed_at_unix_ms
    {
        issues.push(issue(
            SecurityReviewIssueCode::ReviewStale,
            "Security review is stale, future-dated, or lacks a named reviewer.",
            "Repeat the review for the current component set and time window.",
            "Refresh the review before GA evaluation.",
        ));
    }

    let decision = if issues.is_empty() {
        SecurityReviewDecision::Passed
    } else {
        SecurityReviewDecision::Blocked
    };
    let next_actions = if issues.is_empty() {
        vec!["Bind this security gate to the exact shadow release plan.".to_owned()]
    } else {
        issues
            .iter()
            .flat_map(|issue| issue.next_actions.iter().cloned())
            .collect()
    };
    let mut evidence = SecurityReviewEvidence {
        protocol: SECURITY_REVIEW_PROTOCOL.to_owned(),
        review_id: String::new(),
        review_digest: String::new(),
        support_manifest_digest: input.support_manifest_digest,
        release_subjects: input.release_subjects,
        threat_models: input.threat_models,
        findings: input.findings,
        scans: input.scans,
        reviewer: input.reviewer,
        reviewed_at_unix_ms: input.reviewed_at_unix_ms,
        freshness_horizon_unix_ms: input.freshness_horizon_unix_ms,
        decision,
        issues,
        next_actions,
    };
    evidence.review_digest = digest_without_identity(&evidence);
    evidence.review_id = format!("security-review:{}", &evidence.review_digest[7..23]);
    evidence
}

#[must_use]
pub fn security_review_evidence_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(SecurityReviewEvidence))
        .expect("security review schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/ga/lenso.security-review-evidence.v1.schema.json".to_owned(),
    );
    schema
}

fn all_threat_surfaces() -> BTreeSet<ThreatSurface> {
    [
        ThreatSurface::WorkloadIdentity,
        ThreatSurface::TransportBinding,
        ThreatSurface::Delegation,
        ThreatSurface::Tenancy,
        ThreatSurface::EventReplayAndPoisoning,
        ThreatSurface::ExtractionAndCutover,
        ThreatSurface::WorkflowControls,
        ThreatSurface::ReleaseSigning,
        ThreatSurface::Secrets,
        ThreatSurface::BackupAndRestore,
        ThreatSurface::AdminActions,
        ThreatSurface::EmbeddedConsole,
        ThreatSurface::PolicyBypass,
        ThreatSurface::StaleEvidence,
        ThreatSurface::AgentBoundaries,
    ]
    .into_iter()
    .collect()
}

fn issue(
    code: SecurityReviewIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> SecurityReviewIssue {
    SecurityReviewIssue {
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

fn digest_without_identity(evidence: &SecurityReviewEvidence) -> String {
    let mut canonical = evidence.clone();
    canonical.review_id.clear();
    canonical.review_digest.clear();
    extraction_input_digest(
        &serde_json::to_vec(&canonical).expect("security review evidence serializes"),
    )
}
