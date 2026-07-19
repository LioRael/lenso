use crate::extraction_input_digest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

pub const EXTRACTION_CONSOLE_PROJECTION_PROTOCOL: &str = "lenso.extraction-console.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleArtifacts {
    pub readiness: Option<Value>,
    pub plan: Option<Value>,
    #[serde(default)]
    pub phase_artifacts: Vec<Value>,
    pub authority: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionConsoleState {
    Planned,
    Preparing,
    Blocked,
    Quiesced,
    Provisional,
    RolledBack,
    Committed,
    PostCommitRollbackBlocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleAuthority {
    pub kind: String,
    pub owner_id: String,
    pub revision: String,
}

impl Default for ExtractionConsoleAuthority {
    fn default() -> Self {
        Self {
            kind: "unknown".into(),
            owner_id: "unknown".into(),
            revision: "unknown".into(),
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleBlocker {
    pub code: String,
    pub subject: String,
    pub detail: String,
    #[serde(default)]
    pub next_actions: Vec<String>,
    pub artifact_id: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleEvidence {
    pub kind: String,
    pub subject: String,
    pub digest: String,
    pub detail: String,
    pub artifact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleTimelineEntry {
    pub phase_id: String,
    pub kind: String,
    pub state: String,
    pub artifact_id: String,
    #[serde(default)]
    pub blockers: Vec<ExtractionConsoleBlocker>,
    #[serde(default)]
    pub evidence: Vec<ExtractionConsoleEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleApprovalBoundary {
    pub boundary_id: String,
    pub phase_id: String,
    pub action: String,
    pub reason: String,
    #[serde(default)]
    pub required_pins: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsoleProjection {
    pub protocol: String,
    pub projection_digest: String,
    pub state: ExtractionConsoleState,
    pub plan_id: Option<String>,
    pub plan_digest: Option<String>,
    pub readiness_summary: String,
    pub current_authority: ExtractionConsoleAuthority,
    #[serde(default)]
    pub timeline: Vec<ExtractionConsoleTimelineEntry>,
    #[serde(default)]
    pub blockers: Vec<ExtractionConsoleBlocker>,
    #[serde(default)]
    pub evidence: Vec<ExtractionConsoleEvidence>,
    #[serde(default)]
    pub approval_boundaries: Vec<ExtractionConsoleApprovalBoundary>,
    pub read_only: bool,
    #[serde(default)]
    pub apply_actions: Vec<String>,
    pub protected_workflow: String,
}

#[must_use]
pub fn project_extraction_console(
    mut artifacts: ExtractionConsoleArtifacts,
) -> ExtractionConsoleProjection {
    let plan_id = artifacts.plan.as_ref().and_then(|v| text(v, "planId"));
    let plan_digest = artifacts.plan.as_ref().and_then(|v| text(v, "planDigest"));
    let readiness_summary = match artifacts.readiness.as_ref() {
        None => "Readiness evidence has not been recorded.".to_owned(),
        Some(v) if v.get("ready").and_then(Value::as_bool) == Some(true) => {
            "Extraction readiness passed with no blocking findings.".to_owned()
        }
        Some(v) => format!(
            "Extraction readiness is blocked by {} finding(s).",
            array(v, "findings").len()
        ),
    };
    let current_authority = artifacts
        .authority
        .as_ref()
        .map(|v| ExtractionConsoleAuthority {
            kind: text(v, "kind").unwrap_or_else(|| "unknown".into()),
            owner_id: text(v, "ownerId").unwrap_or_else(|| "unknown".into()),
            revision: text(v, "revision").unwrap_or_else(|| "unknown".into()),
        })
        .unwrap_or_default();
    let mut timeline = planned_timeline(artifacts.plan.as_ref());
    let mut blockers = blockers_from(artifacts.readiness.as_ref(), "readiness", "findings");
    let mut evidence = Vec::new();
    for artifact in &artifacts.phase_artifacts {
        let id = artifact_id(artifact);
        let mut phase_blockers = blockers_from(Some(artifact), &id, "issues");
        phase_blockers.extend(blockers_from(Some(artifact), &id, "errors"));
        let phase_evidence = evidence_from(artifact, &id);
        blockers.extend(phase_blockers.iter().cloned());
        evidence.extend(phase_evidence.iter().cloned());
        timeline.push(ExtractionConsoleTimelineEntry {
            phase_id: phase_for(protocol(artifact)).into(),
            kind: protocol(artifact).into(),
            state: status(artifact).into(),
            artifact_id: id,
            blockers: phase_blockers,
            evidence: phase_evidence,
        });
    }
    timeline.sort_by_key(|entry| (phase_order(&entry.phase_id), entry.artifact_id.clone()));
    blockers.sort();
    blockers.dedup();
    evidence.sort();
    evidence.dedup();
    let state = state(&artifacts, !blockers.is_empty());
    artifacts.phase_artifacts.sort_by_key(artifact_id);
    let mut projection = ExtractionConsoleProjection {
        protocol: EXTRACTION_CONSOLE_PROJECTION_PROTOCOL.into(),
        projection_digest: String::new(),
        state,
        plan_id,
        plan_digest,
        readiness_summary,
        current_authority,
        timeline,
        blockers,
        evidence,
        approval_boundaries: approvals(artifacts.plan.as_ref()),
        read_only: true,
        apply_actions: vec![],
        protected_workflow: "lenso service extract".into(),
    };
    projection.projection_digest = digest(&projection);
    projection
}

/// Persist one authoritative workflow artifact for read-only operator projection.
pub async fn record_extraction_artifact(
    pool: &sqlx::PgPool,
    plan_id: &str,
    artifact: &Value,
) -> Result<(), sqlx::Error> {
    let persisted = sqlx::query(
        r#"
        insert into platform.extraction_artifacts
            (plan_id, artifact_id, protocol, artifact_digest, artifact_json)
        values ($1, $2, $3, $4, $5)
        on conflict (plan_id, artifact_id, artifact_digest) do nothing
        "#,
    )
    .bind(plan_id)
    .bind(artifact_id(artifact))
    .bind(protocol(artifact))
    .bind(digest(artifact))
    .bind(artifact)
    .execute(pool)
    .await?;
    let _ = persisted;
    Ok(())
}

/// Load persisted artifacts and evaluate the backend-owned projection.
pub async fn load_extraction_console_projection(
    pool: &sqlx::PgPool,
    requested_plan_id: Option<&str>,
) -> Result<ExtractionConsoleProjection, sqlx::Error> {
    let exists = sqlx::query_scalar::<_, Option<String>>(
        "select to_regclass('platform.extraction_artifacts')::text",
    )
    .fetch_one(pool)
    .await?
    .is_some();
    if !exists {
        return Ok(empty_projection());
    }
    let plan_id = match requested_plan_id {
        Some(plan_id) => Some(plan_id.to_owned()),
        None => sqlx::query_scalar::<_, String>(
            "select plan_id from platform.extraction_artifacts order by recorded_at desc, plan_id desc limit 1",
        )
        .fetch_optional(pool)
        .await?,
    };
    let Some(plan_id) = plan_id else {
        return Ok(empty_projection());
    };
    let rows = sqlx::query_as::<_, (String, Value)>(
        "select protocol, artifact_json from platform.extraction_artifacts where plan_id = $1 order by recorded_at, artifact_id",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;
    let mut artifacts = ExtractionConsoleArtifacts {
        readiness: None,
        plan: None,
        phase_artifacts: Vec::new(),
        authority: None,
    };
    for (protocol, artifact) in rows {
        match protocol.as_str() {
            "lenso.extraction-readiness-report.v1" => artifacts.readiness = Some(artifact),
            "lenso.extraction-plan.v1" => artifacts.plan = Some(artifact),
            "lenso.extraction-authority.v1" => artifacts.authority = Some(artifact),
            _ => artifacts.phase_artifacts.push(artifact),
        }
    }
    Ok(project_extraction_console(artifacts))
}

/// Load one persisted workflow artifact by its stable identifier.
pub async fn load_extraction_artifact(
    pool: &sqlx::PgPool,
    plan_id: &str,
    artifact_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(
        "select artifact_json from platform.extraction_artifacts where plan_id = $1 and artifact_id = $2 order by recorded_at desc limit 1",
    )
    .bind(plan_id)
    .bind(artifact_id)
    .fetch_optional(pool)
    .await
}

fn empty_projection() -> ExtractionConsoleProjection {
    project_extraction_console(ExtractionConsoleArtifacts {
        readiness: None,
        plan: None,
        phase_artifacts: Vec::new(),
        authority: None,
    })
}

fn state(a: &ExtractionConsoleArtifacts, blocked: bool) -> ExtractionConsoleState {
    let has = |p: &str, s: &str| {
        a.phase_artifacts
            .iter()
            .any(|v| protocol(v) == p && status(v) == s)
    };
    if a.phase_artifacts.iter().rev().any(|v| {
        protocol(v) == "lenso.extraction-authority-commit.v1"
            && v.get("fastRollbackBlocked").and_then(Value::as_bool) == Some(true)
    }) {
        ExtractionConsoleState::PostCommitRollbackBlocked
    } else if has("lenso.extraction-authority-commit.v1", "committed") {
        ExtractionConsoleState::Committed
    } else if let Some(cutover) = a
        .phase_artifacts
        .iter()
        .rev()
        .find(|v| protocol(v) == "lenso.extraction-provisional-cutover.v1")
    {
        if status(cutover) == "rolled_back" {
            ExtractionConsoleState::RolledBack
        } else {
            ExtractionConsoleState::Provisional
        }
    } else if has("lenso.extraction-quiescence.v1", "quiesced") {
        ExtractionConsoleState::Quiesced
    } else if blocked || a.phase_artifacts.iter().any(|v| status(v) == "blocked") {
        ExtractionConsoleState::Blocked
    } else if a.phase_artifacts.is_empty() {
        ExtractionConsoleState::Planned
    } else {
        ExtractionConsoleState::Preparing
    }
}

fn planned_timeline(plan: Option<&Value>) -> Vec<ExtractionConsoleTimelineEntry> {
    let plan_artifact_id = plan.map(artifact_id).unwrap_or_else(|| "plan".into());
    plan.map(|v| array(v, "phases"))
        .unwrap_or_default()
        .into_iter()
        .map(|v| ExtractionConsoleTimelineEntry {
            phase_id: text(v, "phaseId").unwrap_or_else(|| "unknown-phase".into()),
            kind: text(v, "kind").unwrap_or_else(|| "unknown".into()),
            state: "planned".into(),
            artifact_id: plan_artifact_id.clone(),
            blockers: vec![],
            evidence: vec![],
        })
        .collect()
}

fn blockers_from(
    value: Option<&Value>,
    artifact_id: &str,
    field: &str,
) -> Vec<ExtractionConsoleBlocker> {
    value
        .map(|v| array(v, field))
        .unwrap_or_default()
        .into_iter()
        .map(|v| ExtractionConsoleBlocker {
            code: text(v, "code")
                .or_else(|| text(v, "issueCode"))
                .unwrap_or_else(|| "blocked".into()),
            subject: text(v, "subject").unwrap_or_else(|| artifact_id.into()),
            detail: text(v, "detail")
                .or_else(|| text(v, "message"))
                .unwrap_or_else(|| "Extraction phase is blocked.".into()),
            next_actions: texts(v, "nextActions"),
            artifact_id: artifact_id.into(),
        })
        .collect()
}

fn evidence_from(value: &Value, artifact_id: &str) -> Vec<ExtractionConsoleEvidence> {
    array(value, "evidence")
        .into_iter()
        .map(|v| ExtractionConsoleEvidence {
            kind: text(v, "kind").unwrap_or_else(|| "evidence".into()),
            subject: text(v, "subject").unwrap_or_else(|| artifact_id.into()),
            digest: text(v, "digest").unwrap_or_else(|| digest(v)),
            detail: text(v, "detail").unwrap_or_default(),
            artifact_id: artifact_id.into(),
        })
        .collect()
}

fn approvals(plan: Option<&Value>) -> Vec<ExtractionConsoleApprovalBoundary> {
    let mut out = plan
        .map(|v| array(v, "phases"))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|phase| {
            let b = phase.get("approvalBoundary")?;
            Some(ExtractionConsoleApprovalBoundary {
                boundary_id: text(b, "boundaryId")?,
                phase_id: text(b, "phaseId")
                    .or_else(|| text(phase, "phaseId"))
                    .unwrap_or_default(),
                action: text(b, "action").unwrap_or_default(),
                reason: text(b, "reason").unwrap_or_default(),
                required_pins: texts(b, "requiredPins"),
            })
        })
        .collect::<Vec<_>>();
    out.sort_by(|l, r| l.boundary_id.cmp(&r.boundary_id));
    out
}

fn artifact_id(v: &Value) -> String {
    [
        "commitId",
        "evidenceId",
        "cutoverId",
        "quiescenceId",
        "verificationId",
        "reconciliationId",
        "runId",
        "planId",
    ]
    .into_iter()
    .find_map(|f| text(v, f))
    .unwrap_or_else(|| digest(v))
}
fn protocol(v: &Value) -> &str {
    v.get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
}
fn status(v: &Value) -> &str {
    v.get("status")
        .and_then(Value::as_str)
        .unwrap_or("recorded")
}
fn phase_for(p: &str) -> &str {
    match p {
        "lenso.extraction-run.v1" => "03-destination-expansion",
        "lenso.extraction-backfill.v1" => "04-backfill",
        "lenso.extraction-reconciliation.v1" => "05-reconciliation",
        "lenso.extraction-verification.v1" => "06-verification",
        "lenso.extraction-quiescence.v1" => "07-drain",
        "lenso.extraction-provisional-cutover.v1" => "08-provisional-cutover",
        "lenso.extraction-authority-commit.v1" => "09-rollback-or-commit",
        "lenso.extraction-candidate-health.v1" => "09-rollback-or-commit",
        _ => "unknown-phase",
    }
}
fn phase_order(v: &str) -> u16 {
    v.split('-')
        .next()
        .and_then(|x| x.parse().ok())
        .unwrap_or(u16::MAX)
}
fn text(v: &Value, f: &str) -> Option<String> {
    v.get(f).and_then(Value::as_str).map(str::to_owned)
}
fn texts(v: &Value, f: &str) -> Vec<String> {
    array(v, f)
        .into_iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn array<'a>(v: &'a Value, f: &str) -> Vec<&'a Value> {
    v.get(f)
        .and_then(Value::as_array)
        .map(|x| x.iter().collect())
        .unwrap_or_default()
}
fn digest(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("projection values serialize"))
}
