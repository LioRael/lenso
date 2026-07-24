use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;

pub const SERVICE_BACKUP_PROTOCOL: &str = "lenso.service-backup.v1";
pub const SERVICE_RESTORE_EVIDENCE_PROTOCOL: &str = "lenso.service-restore-evidence.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RestoreDecision {
    Passed,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RestoreIssueCode {
    BackupIntegrityInvalid,
    BackupIncomplete,
    EncryptionBoundaryInvalid,
    TargetStoreNotClean,
    RestoreStateMismatch,
    ReplayBoundaryInvalid,
    AuthorityConflict,
    EnvironmentEvidenceInvalid,
}

impl RestoreIssueCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BackupIntegrityInvalid => "restore_backup_integrity_invalid",
            Self::BackupIncomplete => "restore_backup_incomplete",
            Self::EncryptionBoundaryInvalid => "restore_encryption_boundary_invalid",
            Self::TargetStoreNotClean => "restore_target_store_not_clean",
            Self::RestoreStateMismatch => "restore_state_mismatch",
            Self::ReplayBoundaryInvalid => "restore_replay_boundary_invalid",
            Self::AuthorityConflict => "restore_authority_conflict",
            Self::EnvironmentEvidenceInvalid => "restore_environment_evidence_invalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreIssue {
    pub code: RestoreIssueCode,
    pub message: String,
    pub remediation: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceBackupInput {
    pub service_id: String,
    pub store_id: String,
    pub schema_version: String,
    pub release_digest: String,
    pub config_revision_digest: String,
    pub point_in_time_unix_ms: u64,
    pub snapshot_digest: String,
    pub encryption_key_reference: String,
    pub encryption_algorithm: String,
    pub state_digests: BTreeMap<String, String>,
    pub outbox_sequence: u64,
    pub inbox_sequence: u64,
    pub workflow_timer_sequence: u64,
    pub story_sequence: u64,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceBackup {
    pub protocol: String,
    pub backup_id: String,
    pub backup_digest: String,
    #[serde(flatten)]
    pub input: ServiceBackupInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostgresRestoreObservation {
    pub provider: String,
    pub version: String,
    pub instance_identity: String,
    pub used_real_instance: bool,
    pub observation_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRestoreInput {
    pub backup: ServiceBackup,
    pub target_store_id: String,
    pub target_was_clean: bool,
    pub restored_snapshot_digest: String,
    pub restored_state_digests: BTreeMap<String, String>,
    pub restored_release_digest: String,
    pub restored_config_revision_digest: String,
    pub replay_outbox_from_sequence: u64,
    pub replay_inbox_from_sequence: u64,
    pub restored_workflow_timer_sequence: u64,
    pub restored_story_sequence: u64,
    pub authoritative_workload_count: u32,
    pub postgres: PostgresRestoreObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRestoreEvidence {
    pub protocol: String,
    pub evidence_id: String,
    pub evidence_digest: String,
    pub backup_id: String,
    pub backup_digest: String,
    pub service_id: String,
    pub source_store_id: String,
    pub target_store_id: String,
    pub point_in_time_unix_ms: u64,
    pub restored_state_digests: BTreeMap<String, String>,
    pub decision: RestoreDecision,
    pub issues: Vec<RestoreIssue>,
    pub next_actions: Vec<String>,
    pub production_mutated: bool,
}

pub fn assemble_service_backup(input: ServiceBackupInput) -> Result<ServiceBackup, RestoreIssue> {
    if input.service_id.trim().is_empty()
        || input.store_id.trim().is_empty()
        || input.schema_version.trim().is_empty()
        || input.point_in_time_unix_ms == 0
        || !valid_digest(&input.release_digest)
        || !valid_digest(&input.config_revision_digest)
        || !valid_digest(&input.snapshot_digest)
        || input.state_digests.len() < 5
        || input
            .state_digests
            .values()
            .any(|digest| !valid_digest(digest))
    {
        return Err(issue(
            RestoreIssueCode::BackupIntegrityInvalid,
            "The backup is not bound to exact Service, Store, release, configuration, schema, and state identities.",
            "Capture one content-addressed backup manifest at the authoritative Store boundary.",
            "Correct the backup inputs and create a new immutable backup.",
        ));
    }
    if !input.completed {
        return Err(issue(
            RestoreIssueCode::BackupIncomplete,
            "The backup did not reach a durable completed state.",
            "Keep partial snapshots ineligible for restore.",
            "Finish or discard the partial backup before restore planning.",
        ));
    }
    if input.encryption_key_reference.trim().is_empty()
        || input.encryption_algorithm.trim().is_empty()
        || input.encryption_key_reference.contains("BEGIN ")
        || input.encryption_key_reference.contains("secret=")
    {
        return Err(issue(
            RestoreIssueCode::EncryptionBoundaryInvalid,
            "Backup encryption must use an opaque key reference without key material.",
            "Resolve encryption only inside the configured backup provider.",
            "Replace key material with an opaque provider reference.",
        ));
    }
    let digest = digest_json(&input);
    Ok(ServiceBackup {
        protocol: SERVICE_BACKUP_PROTOCOL.to_owned(),
        backup_id: format!("service-backup:{}", &digest[7..23]),
        backup_digest: digest,
        input,
    })
}

#[must_use]
pub fn evaluate_service_restore(input: ServiceRestoreInput) -> ServiceRestoreEvidence {
    let mut issues = Vec::new();
    if !service_backup_integrity_valid(&input.backup) {
        issues.push(issue(
            RestoreIssueCode::BackupIntegrityInvalid,
            "Backup content does not match its immutable identity.",
            "Reject modified or stale backup metadata.",
            "Load the exact verified backup and repeat restore.",
        ));
    }
    if !input.target_was_clean {
        issues.push(issue(
            RestoreIssueCode::TargetStoreNotClean,
            "Restore target contains pre-existing authoritative state.",
            "Restore only into an isolated empty Store or use a separately approved destructive plan.",
            "Provision a clean target Store.",
        ));
    }
    if input.restored_snapshot_digest != input.backup.input.snapshot_digest
        || input.restored_state_digests != input.backup.input.state_digests
        || input.restored_release_digest != input.backup.input.release_digest
        || input.restored_config_revision_digest != input.backup.input.config_revision_digest
        || input.restored_workflow_timer_sequence != input.backup.input.workflow_timer_sequence
        || input.restored_story_sequence != input.backup.input.story_sequence
    {
        issues.push(issue(
            RestoreIssueCode::RestoreStateMismatch,
            "Restored business, Workflow, Story, release, or configuration state differs from the backup.",
            "Verify every state partition and exact artifact identity before activation.",
            "Discard the target and repeat restore from verified bytes.",
        ));
    }
    if input.replay_outbox_from_sequence != input.backup.input.outbox_sequence.saturating_add(1)
        || input.replay_inbox_from_sequence != input.backup.input.inbox_sequence.saturating_add(1)
    {
        issues.push(issue(
            RestoreIssueCode::ReplayBoundaryInvalid,
            "Inbox or Outbox replay would skip or repeat a committed effect.",
            "Resume from the first sequence after the backup checkpoint.",
            "Correct the replay cursor before starting Workloads.",
        ));
    }
    if input.authoritative_workload_count != 0 {
        issues.push(issue(
            RestoreIssueCode::AuthorityConflict,
            "Restore verification ran while an authoritative Workload could still write.",
            "Keep the restored target passive until an explicit authority cutover.",
            "Fence active writers and repeat restore verification.",
        ));
    }
    if !input.postgres.used_real_instance
        || input.postgres.provider.trim().is_empty()
        || input.postgres.version.trim().is_empty()
        || input.postgres.instance_identity.trim().is_empty()
        || !valid_digest(&input.postgres.observation_digest)
    {
        issues.push(issue(
            RestoreIssueCode::EnvironmentEvidenceInvalid,
            "Restore evidence does not come from a real isolated Postgres instance.",
            "Use the pinned Environment Verification database lane.",
            "Repeat backup and restore against the supported Postgres adapter.",
        ));
    }

    let decision = if issues.is_empty() {
        RestoreDecision::Passed
    } else {
        RestoreDecision::Blocked
    };
    let next_actions = if issues.is_empty() {
        vec![
            "Keep the restored Store passive until the disaster-recovery Approval Boundary.".into(),
        ]
    } else {
        issues
            .iter()
            .flat_map(|issue| issue.next_actions.iter().cloned())
            .collect()
    };
    let mut evidence = ServiceRestoreEvidence {
        protocol: SERVICE_RESTORE_EVIDENCE_PROTOCOL.to_owned(),
        evidence_id: String::new(),
        evidence_digest: String::new(),
        backup_id: input.backup.backup_id,
        backup_digest: input.backup.backup_digest,
        service_id: input.backup.input.service_id,
        source_store_id: input.backup.input.store_id,
        target_store_id: input.target_store_id,
        point_in_time_unix_ms: input.backup.input.point_in_time_unix_ms,
        restored_state_digests: input.restored_state_digests,
        decision,
        issues,
        next_actions,
        production_mutated: false,
    };
    evidence.evidence_digest = digest_without_identity(&evidence);
    evidence.evidence_id = format!("service-restore:{}", &evidence.evidence_digest[7..23]);
    evidence
}

#[must_use]
pub fn service_restore_evidence_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ServiceRestoreEvidence))
        .expect("service restore schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/ga/lenso.service-restore-evidence.v1.schema.json".to_owned(),
    );
    schema
}

fn service_backup_integrity_valid(backup: &ServiceBackup) -> bool {
    backup.protocol == SERVICE_BACKUP_PROTOCOL
        && valid_digest(&backup.backup_digest)
        && backup.backup_digest == digest_json(&backup.input)
        && backup.backup_id == format!("service-backup:{}", &backup.backup_digest[7..23])
}

fn issue(
    code: RestoreIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> RestoreIssue {
    RestoreIssue {
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

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(&serde_json::to_vec(value).expect("backup evidence serializes"))
}

fn digest_without_identity(evidence: &ServiceRestoreEvidence) -> String {
    let mut canonical = evidence.clone();
    canonical.evidence_id.clear();
    canonical.evidence_digest.clear();
    digest_json(&canonical)
}
