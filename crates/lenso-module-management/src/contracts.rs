use chrono::{DateTime, Utc};
use lenso_contracts::{
    ArtifactReference, ModuleDelivery, ModuleLifecycleState, ModuleVerificationCell,
    VerificationEvaluation, digest_json,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DESIRED_MODULE_COMPOSITION_PROTOCOL: &str = "lenso.desired-module-composition.v1";
pub const APPLICATION_MODULE_LOCK_PROTOCOL: &str = "lenso.application-module-lock.v1";
pub const MODULE_CHANGE_PLAN_PROTOCOL: &str = "lenso.module-change-plan.v1";
pub const MODULE_REPAIR_PLAN_PROTOCOL: &str = "lenso.module-repair-plan.v1";
pub const MODULE_ENVIRONMENT_POLICY_PROTOCOL: &str = "lenso.module-environment-policy.v1";
pub const MODULE_APPROVAL_PROTOCOL: &str = "lenso.module-approval.v1";
pub const MODULE_OPERATION_PROTOCOL: &str = "lenso.module-operation.v1";
pub const MODULE_OPERATION_JOURNAL_PROTOCOL: &str = "lenso.module-operation-journal.v1";
pub const LINKED_COMPOSITION_SEAM_PROTOCOL: &str = "lenso.linked-composition-seam.v1";
pub const MODULE_PLANNING_CONTEXT_PROTOCOL: &str = "lenso.module-planning-context.v1";
pub const MODULE_MANAGEMENT_SNAPSHOT_PROTOCOL: &str = "lenso.module-management-snapshot.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkedCompositionSeam {
    pub protocol: String,
    pub host_manifest_path: String,
    pub host_source_path: String,
    pub generated_crate_path: String,
    pub dependency_name: String,
    pub lenso_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModulePlanningContext {
    pub protocol: String,
    pub system_id: String,
    pub application_id: String,
    pub environment_id: String,
    pub expected_target_revision: u64,
    pub catalog_snapshot_digest: String,
    pub trust_policy_digest: String,
    pub compatibility_evidence_digest: String,
    pub resolver_version: String,
    pub candidates: Vec<crate::ModuleResolutionCandidate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_deployments: Vec<ServiceDeploymentBinding>,
    #[serde(default)]
    pub cargo_offline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentAdapterKind {
    Local,
    ExternallyManaged,
    Kubernetes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceDeploymentAction {
    Command {
        program: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_directory: Option<String>,
    },
    Evidence {
        receipt: ArtifactReference,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceDeploymentBinding {
    pub service_id: String,
    pub service_release_digest: String,
    pub adapter: ServiceDeploymentAdapterKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<crate::ServiceInstallation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<ServiceDeploymentAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove: Option<ServiceDeploymentAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<ServiceDeploymentAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleManagementSnapshotStatus {
    Ready,
    Unconfigured,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleManagementSnapshot {
    pub protocol: String,
    pub status: ModuleManagementSnapshotStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired: Option<DesiredModuleComposition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_lock: Option<ApplicationModuleLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_lock_digest: Option<String>,
    pub planning_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning_context_digest: Option<String>,
    pub execution_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_policy: Option<ModuleEnvironmentPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesiredModuleComposition {
    pub protocol: String,
    pub application_id: String,
    pub revision: u64,
    pub selected: Vec<DesiredModuleSelection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_overrides: Vec<LocalModuleOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesiredModuleSelection {
    pub module_id: String,
    pub version_requirement: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional_requirements: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_release_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_preference: Option<ManagedDeliveryKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalModuleOverride {
    pub module_id: String,
    pub path: String,
    pub content_digest: String,
    pub acknowledged_unverified: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ManagedDeliveryKind {
    Linked,
    Provider,
    Autonomous,
}

impl From<&ModuleDelivery> for ManagedDeliveryKind {
    fn from(value: &ModuleDelivery) -> Self {
        match value {
            ModuleDelivery::Linked(_) => Self::Linked,
            ModuleDelivery::Service(service) => match service.responsibility_profile {
                lenso_contracts::ServiceResponsibilityProfile::Provider => Self::Provider,
                lenso_contracts::ServiceResponsibilityProfile::Autonomous => Self::Autonomous,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplicationModuleLock {
    pub protocol: String,
    pub application_id: String,
    pub desired_composition_digest: String,
    pub catalog_snapshot_digest: String,
    pub trust_policy_digest: String,
    pub resolver_version: String,
    pub modules: Vec<LockedModule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_bindings: Vec<LockedCapabilityBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedModule {
    pub module_id: String,
    pub version: String,
    pub release_digest: String,
    pub manifest_digest: String,
    pub delivery: ModuleDelivery,
    pub reason: LockedModuleReason,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependency_module_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crate_features: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migration_artifacts: Vec<ArtifactReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_ui_artifact: Option<LockedConsoleUiArtifact>,
    pub verification: VerificationEvaluation,
    pub verification_cell: ModuleVerificationCell,
    pub lifecycle: ModuleLifecycleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_override_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LockedModuleReason {
    Direct,
    Transitive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedCapabilityBinding {
    pub capability: String,
    pub provider_module_id: String,
    pub consumer_module_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LockedConsoleUiArtifact {
    pub locator: String,
    pub digest: String,
    pub format: lenso_contracts::ConsoleUiArtifactFormat,
    pub entries: Vec<lenso_contracts::ConsoleUiArtifactEntry>,
    pub bridge_protocol: String,
    pub requested_permissions: Vec<lenso_contracts::ConsolePermissionRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModuleRootChange {
    Install {
        selection: DesiredModuleSelection,
    },
    Update {
        module_id: String,
        version_requirement: String,
    },
    Uninstall {
        module_id: String,
    },
    SelectOptional {
        module_id: String,
        requirement: String,
        selected: bool,
    },
    SwitchDelivery {
        module_id: String,
        delivery: ManagedDeliveryKind,
    },
    Restore {
        target_lock_digest: String,
    },
    Repair {
        repair_plan_digest: String,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentManagementMode {
    Disabled,
    ReadOnly,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleEnvironmentPolicy {
    pub protocol: String,
    pub policy_id: String,
    pub revision: String,
    pub mode: EnvironmentManagementMode,
    pub require_distinct_approver: bool,
    pub maximum_approval_age_seconds: u64,
    pub maximum_lease_seconds: u64,
    pub require_backup_for_non_local_destructive_effects: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRiskClass {
    Ordinary,
    DestructiveMigration,
    DataDeletion,
    BackupRestore,
    BackupWaiver,
    TrustOverride,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleApprovalBoundary {
    pub boundary_id: String,
    pub risk_class: ModuleRiskClass,
    pub required_authority: String,
    pub effect_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_evidence_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleApproval {
    pub protocol: String,
    pub approval_id: String,
    pub plan_digest: String,
    pub application_id: String,
    pub environment_id: String,
    pub expected_target_revision: u64,
    pub boundary_id: String,
    pub risk_class: ModuleRiskClass,
    pub actor_id: String,
    pub verified_authorities: Vec<String>,
    pub reason: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleChangePlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub application_id: String,
    pub environment_id: String,
    pub expected_target_revision: u64,
    pub request: ModuleRootChange,
    pub current_desired_digest: String,
    pub target_desired: DesiredModuleComposition,
    pub target_desired_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_lock_digest: Option<String>,
    pub target_lock: ApplicationModuleLock,
    pub target_lock_digest: String,
    pub catalog_snapshot_digest: String,
    pub resolver_version: String,
    pub trust_policy_digest: String,
    pub compatibility_evidence_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo_lock_candidate: Option<crate::CargoLockCandidate>,
    pub read_set: Vec<ModulePathPrecondition>,
    pub effects: Vec<ModulePlanEffect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approval_boundaries: Vec<ModuleApprovalBoundary>,
    pub validation_commands: Vec<String>,
    pub next_actions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModulePathPrecondition {
    pub path: String,
    pub existence: PathExistence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    pub file_type: ManagedFileType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathExistence {
    Present,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagedFileType {
    Regular,
    Directory,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleFileOwnership {
    User,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModulePlanEffect {
    WorkspaceFile {
        effect_id: String,
        path: String,
        ownership: ModuleFileOwnership,
        change: ModuleFileChange,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before_digest: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_digest: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_content: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_mode: Option<u32>,
        patch: String,
        reversible_before_migration: bool,
    },
    Configuration {
        effect_id: String,
        config_revision_digest: String,
        unresolved_required_fields: Vec<String>,
    },
    Migration {
        effect_id: String,
        module_id: String,
        release_digest: String,
        migration_id: String,
        artifact_locator: String,
        artifact_digest: String,
        store_scope: String,
        execution: MigrationExecutionMode,
        risk_class: ModuleRiskClass,
    },
    Protected {
        effect_id: String,
        risk_class: ModuleRiskClass,
        subject: String,
        evidence_digest: String,
    },
    ConsoleComposition {
        effect_id: String,
        console_service_id: String,
        candidate_lock_digest: String,
    },
    ServiceInstallation {
        effect_id: String,
        service_id: String,
        service_release_digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        installation_plan: Option<crate::ServiceInstallationPlan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        adapter: Option<ServiceDeploymentAdapterKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<ServiceDeploymentAction>,
    },
    ServiceRemoval {
        effect_id: String,
        service_id: String,
        service_release_digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        adapter: Option<ServiceDeploymentAdapterKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<ServiceDeploymentAction>,
    },
    ServiceRestart {
        effect_id: String,
        service_id: String,
        service_release_digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        adapter: Option<ServiceDeploymentAdapterKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<ServiceDeploymentAction>,
    },
    Validate {
        effect_id: String,
        command: String,
        expected_evidence: String,
    },
    Restart {
        effect_id: String,
        target: String,
    },
    Activate {
        effect_id: String,
        target_lock_digest: String,
    },
}

impl ModulePlanEffect {
    #[must_use]
    pub fn effect_id(&self) -> &str {
        match self {
            Self::WorkspaceFile { effect_id, .. }
            | Self::Configuration { effect_id, .. }
            | Self::Migration { effect_id, .. }
            | Self::Protected { effect_id, .. }
            | Self::ConsoleComposition { effect_id, .. }
            | Self::ServiceInstallation { effect_id, .. }
            | Self::ServiceRemoval { effect_id, .. }
            | Self::ServiceRestart { effect_id, .. }
            | Self::Validate { effect_id, .. }
            | Self::Restart { effect_id, .. }
            | Self::Activate { effect_id, .. } => effect_id,
        }
    }

    #[must_use]
    pub fn risk_class(&self) -> ModuleRiskClass {
        match self {
            Self::Migration { risk_class, .. } | Self::Protected { risk_class, .. } => *risk_class,
            _ => ModuleRiskClass::Ordinary,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleFileChange {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationExecutionMode {
    Transactional,
    IdempotentExternal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleRepairPlan {
    pub protocol: String,
    pub repair_plan_id: String,
    pub repair_plan_digest: String,
    pub original_operation_id: String,
    pub original_operation_revision: u64,
    pub application_id: String,
    pub environment_id: String,
    pub observed_state_digest: String,
    pub completed_effect_ids: Vec<String>,
    pub actions: Vec<ModuleRepairAction>,
    pub approval_boundaries: Vec<ModuleApprovalBoundary>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModuleRepairAction {
    Resume {
        effect_ids: Vec<String>,
    },
    ForwardFix {
        module_release_digest: String,
    },
    RestoreWorkspace {
        path_digests: BTreeMap<String, String>,
    },
    ReconcileComposition {
        desired_digest: String,
        lock_digest: String,
    },
    RestoreBackup {
        backup_evidence_digest: String,
        store_scope: String,
    },
    RecordIntervention {
        evidence_digest: String,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleOperationState {
    Planned,
    AwaitingApproval,
    Ready,
    ApplyingFiles,
    FilesApplied,
    StagingConfiguration,
    Migrating,
    Verifying,
    Activating,
    Succeeded,
    Blocked,
    Cancelled,
    Restored,
    RepairRequired,
    Reconciled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleOperation {
    pub protocol: String,
    pub operation_id: String,
    pub idempotency_key: String,
    pub application_id: String,
    pub environment_id: String,
    pub plan_digest: String,
    pub operation_kind: ModuleOperationKind,
    pub expected_target_revision: u64,
    pub current_lock_digest: Option<String>,
    pub target_lock_digest: String,
    pub actor_id: String,
    pub verified_authorities: Vec<String>,
    pub policy_revision: String,
    pub state: ModuleOperationState,
    pub revision: u64,
    pub fencing_token: u64,
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub approvals: Vec<ModuleApproval>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effect_receipts: Vec<ModuleEffectReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspace_backups: Vec<ModuleWorkspaceBackup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ModuleOperationError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_operation_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleWorkspaceBackup {
    pub path: String,
    pub existence: PathExistence,
    pub file_type: ManagedFileType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleOperationKind {
    Install,
    Update,
    Uninstall,
    DeliveryTransition,
    Restore,
    Repair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleEffectReceipt {
    pub receipt_id: String,
    pub effect_id: String,
    pub effect_digest: String,
    pub operation_id: String,
    pub attempt: u32,
    pub fencing_token: u64,
    pub outcome: ModuleEffectOutcome,
    pub evidence_references: Vec<ArtifactReference>,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleEffectOutcome {
    Applied,
    AlreadyApplied,
    Verified,
    Restored,
    Activated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleOperationError {
    pub code: String,
    pub message: String,
    pub evidence_references: Vec<ArtifactReference>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleOperationLease {
    pub application_id: String,
    pub holder_id: String,
    pub fencing_token: u64,
    pub revision: u64,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleResumeEvidence {
    pub plan_digest: String,
    pub observed_target_digest: String,
    pub completed_effect_ids: Vec<String>,
    pub next_effect_id: String,
    pub next_effect_idempotent: bool,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleOperationJournalEvent {
    pub protocol: String,
    pub event_id: String,
    pub operation_id: String,
    pub attempt: u32,
    pub revision: u64,
    pub prior_event_digest: Option<String>,
    pub plan_digest: String,
    pub actor_id: String,
    pub fencing_token: u64,
    pub transition: ModuleOperationTransition,
    pub outcome_code: String,
    pub evidence_references: Vec<ArtifactReference>,
    pub operation_after: ModuleOperation,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleOperationTransition {
    pub from: ModuleOperationState,
    pub to: ModuleOperationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleOperationJournal {
    pub protocol: String,
    pub operation_id: String,
    pub events: Vec<ModuleOperationJournalEvent>,
}

pub fn desired_composition_digest(
    composition: &DesiredModuleComposition,
) -> Result<String, serde_json::Error> {
    digest_json(composition)
}

pub fn application_module_lock_digest(
    module_lock: &ApplicationModuleLock,
) -> Result<String, serde_json::Error> {
    digest_json(module_lock)
}

pub fn module_change_plan_digest(plan: &ModuleChangePlan) -> Result<String, serde_json::Error> {
    let mut unsigned = plan.clone();
    unsigned.plan_digest.clear();
    digest_json(&unsigned)
}

pub fn module_repair_plan_digest(plan: &ModuleRepairPlan) -> Result<String, serde_json::Error> {
    let mut unsigned = plan.clone();
    unsigned.repair_plan_digest.clear();
    digest_json(&unsigned)
}

pub fn journal_event_digest(
    event: &ModuleOperationJournalEvent,
) -> Result<String, serde_json::Error> {
    digest_json(event)
}
