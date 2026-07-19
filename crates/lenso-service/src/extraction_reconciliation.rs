use crate::{
    ExtractionBackfillRecord, ExtractionBackfillRun, ExtractionBackfillStatus,
    extraction_backfill_integrity_is_valid, extraction_input_digest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const EXTRACTION_RECONCILIATION_PROTOCOL: &str = "lenso.extraction-reconciliation.v1";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRelationshipCount {
    pub relationship: String,
    pub count: u64,
}

impl ExtractionRelationshipCount {
    #[must_use]
    pub fn new(relationship: impl Into<String>, count: u64) -> Self {
        Self {
            relationship: relationship.into(),
            count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionNormalizedField {
    pub json_pointer: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBusinessInvariant {
    pub invariant_id: String,
    pub passed: bool,
    pub evidence: String,
}

impl ExtractionBusinessInvariant {
    #[must_use]
    pub fn passed(invariant_id: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self {
            invariant_id: invariant_id.into(),
            passed: true,
            evidence: evidence.into(),
        }
    }

    #[must_use]
    pub fn failed(invariant_id: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self {
            invariant_id: invariant_id.into(),
            passed: false,
            evidence: evidence.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionSourceSnapshot {
    pub source_high_water_mark: String,
    pub records: Vec<ExtractionBackfillRecord>,
    #[serde(default)]
    pub relationship_counts: Vec<ExtractionRelationshipCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReconciliationInputs {
    pub backfill: ExtractionBackfillRun,
    pub source: ExtractionSourceSnapshot,
    #[serde(default)]
    pub destination_relationship_counts: Vec<ExtractionRelationshipCount>,
    #[serde(default)]
    pub normalized_fields: Vec<ExtractionNormalizedField>,
    #[serde(default)]
    pub business_invariants: Vec<ExtractionBusinessInvariant>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionReconciliationStatus {
    Matched,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionReconciliationIssueCode {
    BackfillIncomplete,
    SourceStateChanged,
    RecordCountMismatch,
    StableIdentityMismatch,
    FieldDigestMismatch,
    RelationshipCountMismatch,
    BusinessInvariantMismatch,
    NormalizationReasonMissing,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReconciliationIssue {
    pub code: ExtractionReconciliationIssueCode,
    pub subject: String,
    pub detail: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReconciliationEvidence {
    pub kind: String,
    pub subject: String,
    pub digest: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReconciliationEffects {
    pub reads_source_snapshot: bool,
    pub reads_candidate_snapshot: bool,
    pub mutates_source: bool,
    pub mutates_candidate: bool,
    pub changes_authority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReconciliationResult {
    pub protocol: String,
    pub reconciliation_id: String,
    pub reconciliation_digest: String,
    pub status: ExtractionReconciliationStatus,
    pub plan_id: String,
    pub plan_digest: String,
    pub source_high_water_mark: String,
    pub destination_checkpoint: String,
    pub source_record_count: u64,
    pub destination_record_count: u64,
    pub issues: Vec<ExtractionReconciliationIssue>,
    pub evidence: Vec<ExtractionReconciliationEvidence>,
    pub normalized_fields: Vec<ExtractionNormalizedField>,
    pub linked_authority_remains_authoritative: bool,
    pub candidate_writes_admitted: bool,
    pub effects: ExtractionReconciliationEffects,
}

#[must_use]
pub fn reconcile_extraction_data(
    mut inputs: ExtractionReconciliationInputs,
) -> ExtractionReconciliationResult {
    inputs
        .source
        .records
        .sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    inputs
        .source
        .relationship_counts
        .sort_by(|left, right| left.relationship.cmp(&right.relationship));
    inputs
        .destination_relationship_counts
        .sort_by(|left, right| left.relationship.cmp(&right.relationship));
    inputs
        .normalized_fields
        .sort_by(|left, right| left.json_pointer.cmp(&right.json_pointer));
    inputs
        .business_invariants
        .sort_by(|left, right| left.invariant_id.cmp(&right.invariant_id));

    let backfill = &inputs.backfill;
    let checkpoint = backfill
        .progress
        .destination_checkpoint
        .clone()
        .unwrap_or_default();
    let mut issues = Vec::new();
    let mut evidence = Vec::new();
    if !extraction_backfill_integrity_is_valid(backfill)
        || backfill.status != ExtractionBackfillStatus::Succeeded
    {
        push_issue(
            &mut issues,
            ExtractionReconciliationIssueCode::BackfillIncomplete,
            "backfill",
            "Backfill is not complete and integrity-valid.",
            "Resume the checkpointed backfill before reconciliation.",
        );
    }
    if inputs.source.source_high_water_mark != backfill.progress.source_high_water_mark {
        push_issue(
            &mut issues,
            ExtractionReconciliationIssueCode::SourceStateChanged,
            "source-high-water-mark",
            "The source snapshot no longer matches the backfill high-water mark.",
            "Capture a new bounded source snapshot and backfill its delta.",
        );
    }
    for field in &inputs.normalized_fields {
        if field.json_pointer.trim().is_empty() || field.reason.trim().is_empty() {
            push_issue(
                &mut issues,
                ExtractionReconciliationIssueCode::NormalizationReasonMissing,
                &field.json_pointer,
                "Every normalized or ignored field needs a specific reason.",
                "Declare the exact field and reviewable normalization reason.",
            );
        }
    }
    let source_count = u64::try_from(inputs.source.records.len()).unwrap_or(u64::MAX);
    let destination_count = u64::try_from(backfill.destination_records.len()).unwrap_or(u64::MAX);
    if source_count != destination_count {
        push_issue(
            &mut issues,
            ExtractionReconciliationIssueCode::RecordCountMismatch,
            "records",
            &format!("Source has {source_count} records; destination has {destination_count}."),
            "Repair or resume backfill, then retry reconciliation.",
        );
    }
    let source_ids = stable_ids(&inputs.source.records);
    let destination_ids = stable_ids(&backfill.destination_records);
    if source_ids != destination_ids {
        push_issue(
            &mut issues,
            ExtractionReconciliationIssueCode::StableIdentityMismatch,
            "stable-identities",
            "Source and destination stable record identities differ.",
            "Compare missing and unexpected identities before retrying.",
        );
    }
    let normalized_source = normalize_records(&inputs.source.records, &inputs.normalized_fields);
    let normalized_destination =
        normalize_records(&backfill.destination_records, &inputs.normalized_fields);
    let source_digest = digest(&normalized_source);
    let destination_digest = digest(&normalized_destination);
    evidence.push(ExtractionReconciliationEvidence {
        kind: "record_digest".to_owned(),
        subject: "source".to_owned(),
        digest: source_digest.clone(),
        detail: format!(
            "{source_count} records at {}",
            inputs.source.source_high_water_mark
        ),
    });
    evidence.push(ExtractionReconciliationEvidence {
        kind: "record_digest".to_owned(),
        subject: "destination".to_owned(),
        digest: destination_digest.clone(),
        detail: format!("{destination_count} records at {checkpoint}"),
    });
    if source_ids == destination_ids && source_digest != destination_digest {
        push_issue(
            &mut issues,
            ExtractionReconciliationIssueCode::FieldDigestMismatch,
            "declared-fields",
            "Stable records have different declared field digests.",
            "Inspect per-record differences without broadening normalization.",
        );
    }
    let source_relationships = relationship_map(&inputs.source.relationship_counts);
    let destination_relationships = relationship_map(&inputs.destination_relationship_counts);
    if source_relationships != destination_relationships {
        push_issue(
            &mut issues,
            ExtractionReconciliationIssueCode::RelationshipCountMismatch,
            "relationships",
            "Declared source and destination relationship counts differ.",
            "Repair the missing relationship copy and retry reconciliation.",
        );
    }
    for invariant in &inputs.business_invariants {
        evidence.push(ExtractionReconciliationEvidence {
            kind: "business_invariant".to_owned(),
            subject: invariant.invariant_id.clone(),
            digest: extraction_input_digest(invariant.evidence.as_bytes()),
            detail: invariant.evidence.clone(),
        });
        if !invariant.passed {
            push_issue(
                &mut issues,
                ExtractionReconciliationIssueCode::BusinessInvariantMismatch,
                &invariant.invariant_id,
                &invariant.evidence,
                "Remediate the declared business invariant and retry reconciliation.",
            );
        }
    }
    issues.sort();
    evidence.sort();
    let status = if issues.is_empty() {
        ExtractionReconciliationStatus::Matched
    } else {
        ExtractionReconciliationStatus::Blocked
    };
    let identity_digest = digest(&(
        backfill.scope.plan_id.as_str(),
        inputs.source.source_high_water_mark.as_str(),
        checkpoint.as_str(),
    ));
    let mut result = ExtractionReconciliationResult {
        protocol: EXTRACTION_RECONCILIATION_PROTOCOL.to_owned(),
        reconciliation_id: format!("extraction-reconciliation:{identity_digest}"),
        reconciliation_digest: String::new(),
        status,
        plan_id: backfill.scope.plan_id.clone(),
        plan_digest: backfill.scope.plan_digest.clone(),
        source_high_water_mark: inputs.source.source_high_water_mark,
        destination_checkpoint: checkpoint,
        source_record_count: source_count,
        destination_record_count: destination_count,
        issues,
        evidence,
        normalized_fields: inputs.normalized_fields,
        linked_authority_remains_authoritative: true,
        candidate_writes_admitted: false,
        effects: ExtractionReconciliationEffects {
            reads_source_snapshot: true,
            reads_candidate_snapshot: true,
            ..ExtractionReconciliationEffects::default()
        },
    };
    result.reconciliation_digest = digest(&result_without_digest(&result));
    result
}

fn normalize_records(
    records: &[ExtractionBackfillRecord],
    normalized_fields: &[ExtractionNormalizedField],
) -> Vec<serde_json::Value> {
    records
        .iter()
        .map(|record| {
            let mut value = record.value.clone();
            for field in normalized_fields {
                remove_pointer(&mut value, &field.json_pointer);
            }
            value
        })
        .collect()
}

fn remove_pointer(value: &mut serde_json::Value, pointer: &str) {
    let Some((parent, key)) = pointer.rsplit_once('/') else {
        return;
    };
    if let Some(serde_json::Value::Object(object)) = value.pointer_mut(parent) {
        object.remove(key);
    }
}

fn stable_ids(records: &[ExtractionBackfillRecord]) -> BTreeSet<&str> {
    records
        .iter()
        .map(|record| record.stable_id.as_str())
        .collect()
}

fn relationship_map(counts: &[ExtractionRelationshipCount]) -> BTreeMap<&str, u64> {
    counts
        .iter()
        .map(|count| (count.relationship.as_str(), count.count))
        .collect()
}

fn push_issue(
    issues: &mut Vec<ExtractionReconciliationIssue>,
    code: ExtractionReconciliationIssueCode,
    subject: impl Into<String>,
    detail: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(ExtractionReconciliationIssue {
        code,
        subject: subject.into(),
        detail: detail.into(),
        next_actions: vec![next_action.into()],
    });
}

fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(
        &serde_json::to_vec(value).expect("Extraction reconciliation values must serialize"),
    )
}

fn result_without_digest(
    result: &ExtractionReconciliationResult,
) -> ExtractionReconciliationResult {
    let mut value = result.clone();
    value.reconciliation_digest.clear();
    value
}
