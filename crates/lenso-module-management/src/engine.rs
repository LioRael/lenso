use crate::{
    APPLICATION_MODULE_LOCK_PROTOCOL, ApplicationModuleLock, CreateOperationResult,
    DESIRED_MODULE_COMPOSITION_PROTOCOL, DesiredModuleComposition, EnvironmentManagementMode,
    MODULE_APPROVAL_PROTOCOL, MODULE_CHANGE_PLAN_PROTOCOL, MODULE_ENVIRONMENT_POLICY_PROTOCOL,
    MODULE_OPERATION_JOURNAL_PROTOCOL, MODULE_OPERATION_PROTOCOL, MODULE_REPAIR_PLAN_PROTOCOL,
    ModuleApproval, ModuleApprovalBoundary, ModuleChangePlan, ModuleEffectReceipt,
    ModuleEnvironmentPolicy, ModuleOperation, ModuleOperationError, ModuleOperationJournalEvent,
    ModuleOperationKind, ModuleOperationLease, ModuleOperationState, ModuleOperationStore,
    ModuleOperationStoreError, ModuleOperationTransition, ModulePlanEffect, ModuleRepairAction,
    ModuleRepairPlan, ModuleResumeEvidence, ModuleRiskClass, ModuleRootChange,
    ModuleWorkspaceBackup, application_module_lock_digest, desired_composition_digest,
    journal_event_digest, module_change_plan_digest, module_repair_plan_digest,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Duration, Utc};
use lenso_contracts::ArtifactReference;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagementActor {
    pub actor_id: String,
    pub verified_authorities: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct StartModuleOperation<'a> {
    pub operation_id: &'a str,
    pub idempotency_key: &'a str,
    pub operation_kind: ModuleOperationKind,
    pub plan: &'a ModuleChangePlan,
    pub policy: &'a ModuleEnvironmentPolicy,
    pub actor: &'a ManagementActor,
    pub approvals: Vec<ModuleApproval>,
    pub holder_id: &'a str,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AdvanceModuleOperation {
    pub operation_id: String,
    pub expected_revision: u64,
    pub fencing_token: u64,
    pub next_state: ModuleOperationState,
    pub actor_id: String,
    pub outcome_code: String,
    pub evidence_references: Vec<ArtifactReference>,
    pub error: Option<ModuleOperationError>,
    pub next_actions: Vec<String>,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum ModuleManagementError {
    #[error("management contract is invalid: {0}")]
    InvalidContract(String),
    #[error("environment management is disabled or read-only")]
    EnvironmentNotWritable,
    #[error("actor lacks `{0}` authority")]
    MissingAuthority(String),
    #[error("approval boundary `{0}` is not satisfied")]
    ApprovalMissing(String),
    #[error("approval `{0}` is stale or does not bind the exact plan")]
    ApprovalStale(String),
    #[error("idempotency key is already bound to another plan")]
    IdempotencyConflict,
    #[error("application composition lease is held by `{0}`")]
    LeaseHeld(String),
    #[error("operation fencing token is stale")]
    StaleFencingToken,
    #[error("operation transition from {from:?} to {to:?} is not legal")]
    IllegalTransition {
        from: ModuleOperationState,
        to: ModuleOperationState,
    },
    #[error("effect receipt conflicts with retained evidence for `{0}`")]
    ReceiptConflict(String),
    #[error(transparent)]
    Store(#[from] ModuleOperationStoreError),
}

#[derive(Debug)]
pub struct ModuleManagementEngine<S> {
    store: S,
}

impl<S> ModuleManagementEngine<S>
where
    S: ModuleOperationStore,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn start(
        &self,
        request: StartModuleOperation<'_>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        validate_change_plan(request.plan)?;
        if request.policy.mode != EnvironmentManagementMode::Full {
            return Err(ModuleManagementError::EnvironmentNotWritable);
        }
        if request.policy.protocol != MODULE_ENVIRONMENT_POLICY_PROTOCOL {
            return invalid("unsupported Module Environment Policy protocol");
        }
        require_authority(request.actor, "module.manage")?;
        if request
            .plan
            .effects
            .iter()
            .any(|effect| matches!(effect, ModulePlanEffect::ServiceInstallation { .. }))
        {
            require_authority(request.actor, "service.manage")?;
        }
        if let Some(existing) = self
            .store
            .find_by_idempotency_key(&request.plan.application_id, request.idempotency_key)?
        {
            return if existing.plan_digest == request.plan.plan_digest {
                Ok(existing)
            } else {
                Err(ModuleManagementError::IdempotencyConflict)
            };
        }
        let approvals_satisfied = approvals_satisfied(
            request.plan,
            request.policy,
            request.actor,
            &request.approvals,
            request.now,
        )?;
        let initial_state = if approvals_satisfied {
            ModuleOperationState::Ready
        } else {
            ModuleOperationState::AwaitingApproval
        };
        let fencing_token = if initial_state == ModuleOperationState::Ready {
            self.acquire_lease(
                &request.plan.application_id,
                request.holder_id,
                request.policy.maximum_lease_seconds,
                request.now,
            )?
            .fencing_token
        } else {
            0
        };
        let operation = ModuleOperation {
            protocol: MODULE_OPERATION_PROTOCOL.to_owned(),
            operation_id: request.operation_id.to_owned(),
            idempotency_key: request.idempotency_key.to_owned(),
            application_id: request.plan.application_id.clone(),
            environment_id: request.plan.environment_id.clone(),
            plan_digest: request.plan.plan_digest.clone(),
            operation_kind: request.operation_kind,
            expected_target_revision: request.plan.expected_target_revision,
            current_lock_digest: request.plan.current_lock_digest.clone(),
            target_lock_digest: request.plan.target_lock_digest.clone(),
            actor_id: request.actor.actor_id.clone(),
            verified_authorities: request.actor.verified_authorities.iter().cloned().collect(),
            policy_revision: request.policy.revision.clone(),
            state: initial_state,
            revision: 0,
            fencing_token,
            attempt: 1,
            approvals: request.approvals,
            effect_receipts: Vec::new(),
            workspace_backups: Vec::new(),
            errors: Vec::new(),
            next_actions: if initial_state == ModuleOperationState::AwaitingApproval {
                vec!["obtain_plan_bound_approvals".to_owned()]
            } else {
                vec!["apply_reviewed_plan".to_owned()]
            },
            repair_operation_id: None,
            updated_at: request.now,
        };
        let event = initial_event(&operation);
        match self.store.create_idempotent(&operation, &event)? {
            CreateOperationResult::Created => Ok(operation),
            CreateOperationResult::Existing(existing)
                if existing.plan_digest == request.plan.plan_digest =>
            {
                Ok(*existing)
            }
            CreateOperationResult::Existing(_) => Err(ModuleManagementError::IdempotencyConflict),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn submit_approval(
        &self,
        operation_id: &str,
        expected_revision: u64,
        plan: &ModuleChangePlan,
        policy: &ModuleEnvironmentPolicy,
        requester: &ManagementActor,
        approval: ModuleApproval,
        holder_id: &str,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        validate_change_plan(plan)?;
        let current = self.store.load(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        if current.state != ModuleOperationState::AwaitingApproval
            || current.plan_digest != plan.plan_digest
        {
            return Err(ModuleManagementError::IllegalTransition {
                from: current.state,
                to: ModuleOperationState::Ready,
            });
        }
        let mut approvals = current.approvals.clone();
        approvals.retain(|existing| existing.boundary_id != approval.boundary_id);
        approvals.push(approval);
        approvals.sort_by(|left, right| left.boundary_id.cmp(&right.boundary_id));
        let ready = approvals_satisfied(plan, policy, requester, &approvals, now)?;
        let mut updated = current.clone();
        updated.approvals = approvals;
        updated.revision = updated.revision.saturating_add(1);
        updated.updated_at = now;
        if ready {
            let lease = self.acquire_lease(
                &current.application_id,
                holder_id,
                policy.maximum_lease_seconds,
                now,
            )?;
            updated.state = ModuleOperationState::Ready;
            updated.fencing_token = lease.fencing_token;
            updated.next_actions = vec!["apply_reviewed_plan".to_owned()];
        } else {
            updated.next_actions = vec!["obtain_plan_bound_approvals".to_owned()];
        }
        let event = next_event(
            &current,
            updated,
            requester.actor_id.clone(),
            "approval_recorded".to_owned(),
            Vec::new(),
            now,
            self.store.journal(operation_id)?.events.last(),
        )?;
        self.store.compare_and_append(expected_revision, &event)?;
        Ok(event.operation_after)
    }

    pub fn advance(
        &self,
        request: AdvanceModuleOperation,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        let current = self.store.load(&request.operation_id)?;
        if current.revision != request.expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: request.expected_revision,
                observed: current.revision,
            }
            .into());
        }
        if current.fencing_token != request.fencing_token {
            return Err(ModuleManagementError::StaleFencingToken);
        }
        if !legal_transition(current.state, request.next_state) {
            return Err(ModuleManagementError::IllegalTransition {
                from: current.state,
                to: request.next_state,
            });
        }
        let mut updated = current.clone();
        updated.state = request.next_state;
        updated.revision = updated.revision.saturating_add(1);
        updated.updated_at = request.now;
        updated.next_actions = request.next_actions;
        if let Some(error) = request.error {
            updated.errors.push(error);
        }
        let event = next_event(
            &current,
            updated,
            request.actor_id,
            request.outcome_code,
            request.evidence_references,
            request.now,
            self.store.journal(&request.operation_id)?.events.last(),
        )?;
        self.store
            .compare_and_append(request.expected_revision, &event)?;
        Ok(event.operation_after)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn retry_blocked(
        &self,
        operation_id: &str,
        expected_revision: u64,
        next_state: ModuleOperationState,
        holder_id: &str,
        maximum_lease_seconds: u64,
        actor_id: &str,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        let current = self.store.load(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        if current.state != ModuleOperationState::Blocked
            || !matches!(
                next_state,
                ModuleOperationState::StagingConfiguration
                    | ModuleOperationState::Migrating
                    | ModuleOperationState::Verifying
                    | ModuleOperationState::Activating
            )
        {
            return Err(ModuleManagementError::IllegalTransition {
                from: current.state,
                to: next_state,
            });
        }
        let lease = self.acquire_lease(
            &current.application_id,
            holder_id,
            maximum_lease_seconds,
            now,
        )?;
        let mut updated = current.clone();
        updated.state = next_state;
        updated.revision = updated.revision.saturating_add(1);
        updated.attempt = updated.attempt.saturating_add(1);
        updated.fencing_token = lease.fencing_token;
        updated.updated_at = now;
        updated.next_actions = vec!["retry_incomplete_effects".to_owned()];
        let event = next_event(
            &current,
            updated,
            actor_id.to_owned(),
            "blocked_operation_retry_started".to_owned(),
            Vec::new(),
            now,
            self.store.journal(operation_id)?.events.last(),
        )?;
        self.store.compare_and_append(expected_revision, &event)?;
        Ok(event.operation_after)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_workspace_application(
        &self,
        operation_id: &str,
        expected_revision: u64,
        fencing_token: u64,
        actor_id: &str,
        plan: &ModuleChangePlan,
        backups: Vec<ModuleWorkspaceBackup>,
        evidence_references: Vec<ArtifactReference>,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        validate_change_plan(plan)?;
        let current = self.store.load(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        if current.fencing_token != fencing_token {
            return Err(ModuleManagementError::StaleFencingToken);
        }
        if current.plan_digest != plan.plan_digest {
            return Err(ModuleManagementError::IdempotencyConflict);
        }
        if current.state != ModuleOperationState::Ready {
            return Err(ModuleManagementError::IllegalTransition {
                from: current.state,
                to: ModuleOperationState::ApplyingFiles,
            });
        }
        require_sorted_unique(
            backups.iter().map(|backup| backup.path.as_str()),
            "workspace backups",
        )?;
        let backup_paths = backups
            .iter()
            .map(|backup| backup.path.as_str())
            .collect::<BTreeSet<_>>();
        if plan.effects.iter().any(|effect| {
            matches!(effect, ModulePlanEffect::WorkspaceFile { path, .. } if !backup_paths.contains(path.as_str()))
        }) {
            return invalid("workspace backups must cover every planned file effect");
        }
        for backup in &backups {
            validate_workspace_backup(backup)?;
        }
        let mut updated = current.clone();
        updated.state = ModuleOperationState::ApplyingFiles;
        updated.revision = updated.revision.saturating_add(1);
        updated.updated_at = now;
        updated.workspace_backups = backups;
        updated.next_actions = vec!["apply_next_guarded_file".to_owned()];
        let event = next_event(
            &current,
            updated,
            actor_id.to_owned(),
            "workspace_backups_recorded".to_owned(),
            evidence_references,
            now,
            self.store.journal(operation_id)?.events.last(),
        )?;
        self.store.compare_and_append(expected_revision, &event)?;
        Ok(event.operation_after)
    }

    pub fn record_effect_receipt(
        &self,
        operation_id: &str,
        expected_revision: u64,
        fencing_token: u64,
        actor_id: &str,
        receipt: ModuleEffectReceipt,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        let current = self.store.load(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        if current.fencing_token != fencing_token || receipt.fencing_token != fencing_token {
            return Err(ModuleManagementError::StaleFencingToken);
        }
        if !matches!(
            current.state,
            ModuleOperationState::ApplyingFiles
                | ModuleOperationState::FilesApplied
                | ModuleOperationState::StagingConfiguration
                | ModuleOperationState::Migrating
                | ModuleOperationState::Verifying
                | ModuleOperationState::Activating
        ) {
            return invalid("effect receipts are only accepted while effects are being applied");
        }
        if receipt.operation_id != current.operation_id || receipt.attempt != current.attempt {
            return Err(ModuleManagementError::ReceiptConflict(
                receipt.effect_id.clone(),
            ));
        }
        if let Some(existing) = current
            .effect_receipts
            .iter()
            .find(|existing| existing.effect_id == receipt.effect_id)
        {
            return if existing == &receipt {
                Ok(current)
            } else {
                Err(ModuleManagementError::ReceiptConflict(receipt.effect_id))
            };
        }
        let mut updated = current.clone();
        updated.revision = updated.revision.saturating_add(1);
        updated.updated_at = now;
        updated.effect_receipts.push(receipt.clone());
        updated
            .effect_receipts
            .sort_by(|left, right| left.effect_id.cmp(&right.effect_id));
        let event = next_event(
            &current,
            updated,
            actor_id.to_owned(),
            "effect_receipt_recorded".to_owned(),
            receipt.evidence_references.clone(),
            now,
            self.store.journal(operation_id)?.events.last(),
        )?;
        self.store.compare_and_append(expected_revision, &event)?;
        Ok(event.operation_after)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resume_after_crash(
        &self,
        operation_id: &str,
        expected_revision: u64,
        evidence: &ModuleResumeEvidence,
        holder_id: &str,
        maximum_lease_seconds: u64,
        actor_id: &str,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        let current = self.store.load(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        if !matches!(
            current.state,
            ModuleOperationState::ApplyingFiles
                | ModuleOperationState::FilesApplied
                | ModuleOperationState::StagingConfiguration
                | ModuleOperationState::Migrating
                | ModuleOperationState::Verifying
                | ModuleOperationState::Activating
        ) || evidence.plan_digest != current.plan_digest
            || !evidence.next_effect_idempotent
            || evidence.observed_target_digest.is_empty()
        {
            return Err(ModuleManagementError::InvalidContract(
                "crash continuation is not proven safe".to_owned(),
            ));
        }
        let completed = current
            .effect_receipts
            .iter()
            .map(|receipt| receipt.effect_id.as_str())
            .collect::<Vec<_>>();
        if completed
            != evidence
                .completed_effect_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
            || completed.contains(&evidence.next_effect_id.as_str())
        {
            return Err(ModuleManagementError::InvalidContract(
                "resume evidence does not match completed effect receipts".to_owned(),
            ));
        }
        let lease = self.acquire_lease(
            &current.application_id,
            holder_id,
            maximum_lease_seconds,
            now,
        )?;
        let mut updated = current.clone();
        updated.revision = updated.revision.saturating_add(1);
        updated.attempt = updated.attempt.saturating_add(1);
        updated.fencing_token = lease.fencing_token;
        updated.updated_at = now;
        updated.next_actions = vec![format!("resume_effect:{}", evidence.next_effect_id)];
        let event = next_event(
            &current,
            updated,
            actor_id.to_owned(),
            "crash_recovery_attempt_started".to_owned(),
            Vec::new(),
            now,
            self.store.journal(operation_id)?.events.last(),
        )?;
        self.store.compare_and_append(expected_revision, &event)?;
        Ok(event.operation_after)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reconcile_with_succeeded_repair(
        &self,
        operation_id: &str,
        expected_revision: u64,
        repair_plan: &ModuleRepairPlan,
        repair_change_plan: &ModuleChangePlan,
        repair_operation_id: &str,
        actor_id: &str,
        evidence_references: Vec<ArtifactReference>,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, ModuleManagementError> {
        validate_repair_plan(repair_plan)?;
        validate_change_plan(repair_change_plan)?;
        let current = self.store.load(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: current.operation_id,
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        let repair_digest_matches = matches!(
            &repair_change_plan.request,
            ModuleRootChange::Repair { repair_plan_digest }
                if repair_plan_digest == &repair_plan.repair_plan_digest
        );
        if current.state != ModuleOperationState::RepairRequired
            || repair_plan.original_operation_id != current.operation_id
            || repair_plan.original_operation_revision != current.revision
            || repair_plan.application_id != current.application_id
            || repair_plan.environment_id != current.environment_id
            || repair_change_plan.application_id != current.application_id
            || repair_change_plan.environment_id != current.environment_id
            || !repair_digest_matches
        {
            return invalid("repair plan does not bind the exact repair-required operation");
        }
        let repair_operation = self.store.load(repair_operation_id)?;
        if repair_operation.operation_kind != ModuleOperationKind::Repair
            || repair_operation.state != ModuleOperationState::Succeeded
            || repair_operation.plan_digest != repair_change_plan.plan_digest
            || repair_operation.application_id != current.application_id
            || repair_operation.environment_id != current.environment_id
        {
            return invalid("reconciliation requires the bound repair operation to succeed");
        }
        let mut updated = current.clone();
        updated.state = ModuleOperationState::Reconciled;
        updated.revision = updated.revision.saturating_add(1);
        updated.updated_at = now;
        updated.next_actions.clear();
        updated.repair_operation_id = Some(repair_operation.operation_id);
        let event = next_event(
            &current,
            updated,
            actor_id.to_owned(),
            "repair_reconciled".to_owned(),
            evidence_references,
            now,
            self.store.journal(operation_id)?.events.last(),
        )?;
        self.store.compare_and_append(expected_revision, &event)?;
        Ok(event.operation_after)
    }

    pub fn acquire_lease(
        &self,
        application_id: &str,
        holder_id: &str,
        maximum_lease_seconds: u64,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperationLease, ModuleManagementError> {
        let current = self.store.load_lease()?;
        if let Some(lease) = &current
            && lease.expires_at > now
            && (lease.application_id != application_id || lease.holder_id != holder_id)
        {
            return Err(ModuleManagementError::LeaseHeld(lease.holder_id.clone()));
        }
        let duration = i64::try_from(maximum_lease_seconds).map_err(|_| {
            ModuleManagementError::InvalidContract("lease duration overflows".into())
        })?;
        let lease = ModuleOperationLease {
            application_id: application_id.to_owned(),
            holder_id: holder_id.to_owned(),
            fencing_token: current.as_ref().map_or(1, |lease| {
                if lease.application_id == application_id
                    && lease.holder_id == holder_id
                    && lease.expires_at > now
                {
                    lease.fencing_token
                } else {
                    lease.fencing_token.saturating_add(1)
                }
            }),
            revision: current
                .as_ref()
                .map_or(0, |lease| lease.revision.saturating_add(1)),
            acquired_at: now,
            expires_at: now + Duration::seconds(duration),
        };
        self.store
            .compare_and_set_lease(current.as_ref().map(|lease| lease.revision), Some(&lease))?;
        Ok(lease)
    }

    pub fn release_lease(
        &self,
        holder_id: &str,
        fencing_token: u64,
    ) -> Result<(), ModuleManagementError> {
        let current = self.store.load_lease()?;
        let Some(lease) = current else {
            return Ok(());
        };
        if lease.holder_id != holder_id || lease.fencing_token != fencing_token {
            return Err(ModuleManagementError::StaleFencingToken);
        }
        self.store
            .compare_and_set_lease(Some(lease.revision), None)?;
        Ok(())
    }
}

fn validate_workspace_backup(backup: &ModuleWorkspaceBackup) -> Result<(), ModuleManagementError> {
    if !safe_relative_path(&backup.path) {
        return invalid("workspace backup path is unsafe");
    }
    let valid = match (backup.existence, backup.file_type) {
        (crate::PathExistence::Absent, crate::ManagedFileType::Absent)
        | (crate::PathExistence::Present, crate::ManagedFileType::Directory) => {
            backup.content_base64.is_none() && backup.content_digest.is_none()
        }
        (crate::PathExistence::Present, crate::ManagedFileType::Regular) => {
            let Some(encoded) = &backup.content_base64 else {
                return invalid("regular-file backup has no exact bytes");
            };
            let Some(digest) = &backup.content_digest else {
                return invalid("regular-file backup has no content digest");
            };
            BASE64
                .decode(encoded)
                .is_ok_and(|bytes| raw_digest(&bytes) == *digest)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        invalid("workspace backup shape or digest is invalid")
    }
}

pub fn validate_change_plan(plan: &ModuleChangePlan) -> Result<(), ModuleManagementError> {
    if plan.protocol != MODULE_CHANGE_PLAN_PROTOCOL {
        return invalid("unsupported Module Change Plan protocol");
    }
    if module_change_plan_digest(plan).map_err(json_error)? != plan.plan_digest {
        return invalid("Module Change Plan digest mismatch");
    }
    validate_desired_composition(&plan.target_desired)?;
    if desired_composition_digest(&plan.target_desired).map_err(json_error)?
        != plan.target_desired_digest
        || plan.target_desired.application_id != plan.application_id
    {
        return invalid("target Desired Composition identity or digest mismatch");
    }
    validate_application_module_lock(&plan.target_lock)?;
    if application_module_lock_digest(&plan.target_lock).map_err(json_error)?
        != plan.target_lock_digest
        || plan.target_lock.application_id != plan.application_id
        || plan.target_lock.desired_composition_digest != plan.target_desired_digest
        || plan.target_lock.catalog_snapshot_digest != plan.catalog_snapshot_digest
        || plan.target_lock.trust_policy_digest != plan.trust_policy_digest
    {
        return invalid("target Application Module Lock binding or digest mismatch");
    }
    require_sorted_unique(
        plan.read_set.iter().map(|entry| entry.path.as_str()),
        "read set",
    )?;
    require_sorted_unique(
        plan.effects.iter().map(ModulePlanEffect::effect_id),
        "effect ids",
    )?;
    if plan.validation_commands.is_empty()
        || plan
            .read_set
            .iter()
            .any(|entry| entry.path == ".env" || entry.path.ends_with("/.env"))
    {
        return invalid("plans require validation commands and must never target .env");
    }
    for boundary in &plan.approval_boundaries {
        validate_boundary(boundary, &plan.effects)?;
    }
    for effect in &plan.effects {
        match effect {
            ModulePlanEffect::WorkspaceFile {
                path,
                change,
                before_digest,
                after_digest,
                after_content,
                after_mode,
                patch,
                ..
            } => validate_workspace_effect(
                path,
                *change,
                before_digest.as_deref(),
                after_digest.as_deref(),
                after_content.as_deref(),
                *after_mode,
                patch,
                &plan.read_set,
            )?,
            ModulePlanEffect::Migration {
                artifact_locator,
                artifact_digest,
                ..
            } => {
                if !safe_relative_path(artifact_locator) || !valid_digest(artifact_digest) {
                    return invalid(
                        "migration artifacts require a safe locator and SHA-256 digest",
                    );
                }
            }
            ModulePlanEffect::ServiceInstallation {
                service_id,
                service_release_digest,
                installation_plan,
                adapter,
                action,
                ..
            } => {
                validate_service_effect(
                    service_id,
                    service_release_digest,
                    *adapter,
                    action.as_ref(),
                )?;
                if let Some(installation_plan) = installation_plan {
                    crate::validate_service_installation_plan(installation_plan).map_err(
                        |error| ModuleManagementError::InvalidContract(error.to_string()),
                    )?;
                    let installation = match &installation_plan.change {
                        crate::ServiceInstallationChange::Install { installation } => installation,
                        crate::ServiceInstallationChange::Uninstall { .. } => {
                            return invalid(
                                "Module Service installation effect cannot contain an uninstall plan",
                            );
                        }
                    };
                    if installation_plan.environment_id != plan.environment_id
                        || installation.service_ref.service_id != *service_id
                        || installation.service_release.digest != *service_release_digest
                    {
                        return invalid(
                            "nested Service Installation Plan differs from the Module plan",
                        );
                    }
                }
            }
            ModulePlanEffect::ServiceRemoval {
                service_id,
                service_release_digest,
                adapter,
                action,
                ..
            }
            | ModulePlanEffect::ServiceRestart {
                service_id,
                service_release_digest,
                adapter,
                action,
                ..
            } => {
                validate_service_effect(
                    service_id,
                    service_release_digest,
                    *adapter,
                    action.as_ref(),
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_service_effect(
    service_id: &str,
    release_digest: &str,
    adapter: Option<crate::ServiceDeploymentAdapterKind>,
    action: Option<&crate::ServiceDeploymentAction>,
) -> Result<(), ModuleManagementError> {
    if service_id.trim().is_empty() || !valid_digest(release_digest) {
        return invalid("service effects require a Service identity and release digest");
    }
    if adapter.is_none() != action.is_none() {
        return invalid("service effects must bind adapter and action together");
    }
    let Some(action) = action else {
        return Ok(());
    };
    match action {
        crate::ServiceDeploymentAction::Command {
            program,
            args,
            working_directory,
        } => {
            let executable = program.rsplit(['/', '\\']).next().unwrap_or_default();
            if program.trim().is_empty()
                || program.contains('\0')
                || matches!(
                    executable.to_ascii_lowercase().as_str(),
                    "sh" | "bash"
                        | "dash"
                        | "zsh"
                        | "fish"
                        | "cmd"
                        | "cmd.exe"
                        | "powershell"
                        | "powershell.exe"
                        | "pwsh"
                        | "pwsh.exe"
                )
                || args.iter().any(|argument| argument.contains('\0'))
                || working_directory
                    .as_deref()
                    .is_some_and(|path| !safe_relative_path(path))
                || adapter == Some(crate::ServiceDeploymentAdapterKind::ExternallyManaged)
            {
                return invalid(
                    "service command action is unsafe or incompatible with its adapter",
                );
            }
        }
        crate::ServiceDeploymentAction::Evidence { receipt } => {
            if !safe_relative_path(&receipt.locator) || !valid_digest(&receipt.digest) {
                return invalid(
                    "service evidence action requires a safe content-addressed receipt",
                );
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_workspace_effect(
    path: &str,
    change: crate::ModuleFileChange,
    before_digest: Option<&str>,
    after_digest: Option<&str>,
    after_content: Option<&str>,
    after_mode: Option<u32>,
    exact_patch: &str,
    read_set: &[crate::ModulePathPrecondition],
) -> Result<(), ModuleManagementError> {
    if !safe_relative_path(path) || exact_patch.trim().is_empty() {
        return invalid("workspace effects require a safe relative path and exact patch");
    }
    let precondition = read_set
        .iter()
        .find(|precondition| precondition.path == path)
        .ok_or_else(|| {
            ModuleManagementError::InvalidContract(format!(
                "workspace effect `{path}` is absent from the read set"
            ))
        })?;
    let shape_matches = match change {
        crate::ModuleFileChange::Create => {
            precondition.existence == crate::PathExistence::Absent
                && before_digest.is_none()
                && after_digest.is_some()
                && after_content.is_some()
                && after_mode.is_some()
        }
        crate::ModuleFileChange::Modify => {
            precondition.existence == crate::PathExistence::Present
                && precondition.content_digest.as_deref() == before_digest
                && before_digest.is_some()
                && after_digest.is_some()
                && after_content.is_some()
                && after_mode.is_some()
        }
        crate::ModuleFileChange::Delete => {
            precondition.existence == crate::PathExistence::Present
                && precondition.content_digest.as_deref() == before_digest
                && before_digest.is_some()
                && after_digest.is_none()
                && after_content.is_none()
                && after_mode.is_none()
        }
    };
    if !shape_matches
        || before_digest.is_some_and(|digest| !valid_digest(digest))
        || after_digest.is_some_and(|digest| !valid_digest(digest))
        || after_mode.is_some_and(|mode| mode > 0o777)
        || after_content
            .is_some_and(|content| Some(raw_digest(content.as_bytes()).as_str()) != after_digest)
    {
        return invalid("workspace effect content does not match its exact precondition or digest");
    }
    Ok(())
}

pub fn validate_desired_composition(
    composition: &DesiredModuleComposition,
) -> Result<(), ModuleManagementError> {
    if composition.protocol != DESIRED_MODULE_COMPOSITION_PROTOCOL
        || composition.application_id.trim().is_empty()
    {
        return invalid("invalid Desired Module Composition identity or protocol");
    }
    require_sorted_unique(
        composition
            .selected
            .iter()
            .map(|entry| entry.module_id.as_str()),
        "selected Module identities",
    )?;
    for entry in &composition.selected {
        if !valid_module_id(&entry.module_id)
            || semver::VersionReq::parse(&entry.version_requirement).is_err()
        {
            return invalid("selected Module identity or version requirement is invalid");
        }
        require_sorted_unique(
            entry.optional_requirements.iter().map(String::as_str),
            "optional requirements",
        )?;
    }
    require_sorted_unique(
        composition
            .local_overrides
            .iter()
            .map(|entry| entry.module_id.as_str()),
        "local overrides",
    )?;
    if composition
        .local_overrides
        .iter()
        .any(|entry| !entry.acknowledged_unverified || !valid_digest(&entry.content_digest))
    {
        return invalid("local overrides must be content-addressed and explicitly unverified");
    }
    Ok(())
}

pub fn validate_application_module_lock(
    module_lock: &ApplicationModuleLock,
) -> Result<(), ModuleManagementError> {
    if module_lock.protocol != APPLICATION_MODULE_LOCK_PROTOCOL
        || module_lock.application_id.trim().is_empty()
        || !valid_digest(&module_lock.desired_composition_digest)
        || !valid_digest(&module_lock.catalog_snapshot_digest)
        || !valid_digest(&module_lock.trust_policy_digest)
    {
        return invalid("invalid Application Module Lock identity, protocol, or input digest");
    }
    require_sorted_unique(
        module_lock
            .modules
            .iter()
            .map(|entry| entry.module_id.as_str()),
        "locked Module identities",
    )?;
    let module_ids = module_lock
        .modules
        .iter()
        .map(|entry| entry.module_id.as_str())
        .collect::<BTreeSet<_>>();
    for module in &module_lock.modules {
        if !valid_module_id(&module.module_id)
            || semver::Version::parse(&module.version).is_err()
            || !valid_digest(&module.release_digest)
            || !valid_digest(&module.manifest_digest)
        {
            return invalid("locked Module identity, version, or digest is invalid");
        }
        require_sorted_unique(
            module.dependency_module_ids.iter().map(String::as_str),
            "locked dependencies",
        )?;
        if module
            .dependency_module_ids
            .iter()
            .any(|dependency| !module_ids.contains(dependency.as_str()))
        {
            return invalid("locked dependency references an absent Module");
        }
    }
    Ok(())
}

pub fn validate_repair_plan(plan: &ModuleRepairPlan) -> Result<(), ModuleManagementError> {
    if plan.protocol != MODULE_REPAIR_PLAN_PROTOCOL
        || plan.original_operation_id.trim().is_empty()
        || plan.actions.is_empty()
        || module_repair_plan_digest(plan).map_err(json_error)? != plan.repair_plan_digest
    {
        return invalid("invalid Module Repair Plan protocol, identity, actions, or digest");
    }
    require_sorted_unique(
        plan.completed_effect_ids.iter().map(String::as_str),
        "completed repair-plan effects",
    )?;
    let completed = plan
        .completed_effect_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for action in &plan.actions {
        if let ModuleRepairAction::Resume { effect_ids } = action {
            require_sorted_unique(
                effect_ids.iter().map(String::as_str),
                "repair resume effects",
            )?;
            if effect_ids
                .iter()
                .any(|effect_id| completed.contains(effect_id.as_str()))
            {
                return invalid("repair plan cannot repeat a completed effect");
            }
        }
    }
    Ok(())
}

fn approvals_satisfied(
    plan: &ModuleChangePlan,
    policy: &ModuleEnvironmentPolicy,
    requester: &ManagementActor,
    approvals: &[ModuleApproval],
    now: DateTime<Utc>,
) -> Result<bool, ModuleManagementError> {
    let mut satisfied = true;
    for boundary in &plan.approval_boundaries {
        if policy.require_backup_for_non_local_destructive_effects
            && plan.environment_id != "local"
            && matches!(
                boundary.risk_class,
                ModuleRiskClass::DestructiveMigration
                    | ModuleRiskClass::DataDeletion
                    | ModuleRiskClass::BackupRestore
            )
            && boundary.backup_evidence_digest.is_none()
        {
            return Err(ModuleManagementError::ApprovalMissing(format!(
                "{}:backup_evidence",
                boundary.boundary_id
            )));
        }
        let Some(approval) = approvals
            .iter()
            .find(|approval| approval.boundary_id == boundary.boundary_id)
        else {
            satisfied = false;
            continue;
        };
        if approval.protocol != MODULE_APPROVAL_PROTOCOL
            || approval.plan_digest != plan.plan_digest
            || approval.application_id != plan.application_id
            || approval.environment_id != plan.environment_id
            || approval.expected_target_revision != plan.expected_target_revision
            || approval.risk_class != boundary.risk_class
            || approval.expires_at <= now
            || approval.issued_at > now
            || approval.reason.trim().is_empty()
            || approval.nonce.trim().is_empty()
            || !approval
                .verified_authorities
                .contains(&boundary.required_authority)
            || policy.require_distinct_approver && approval.actor_id == requester.actor_id
        {
            return Err(ModuleManagementError::ApprovalStale(
                approval.approval_id.clone(),
            ));
        }
        let age = now.signed_duration_since(approval.issued_at).num_seconds();
        if age < 0 || u64::try_from(age).unwrap_or(u64::MAX) > policy.maximum_approval_age_seconds {
            return Err(ModuleManagementError::ApprovalStale(
                approval.approval_id.clone(),
            ));
        }
    }
    Ok(satisfied)
}

fn validate_boundary(
    boundary: &ModuleApprovalBoundary,
    effects: &[ModulePlanEffect],
) -> Result<(), ModuleManagementError> {
    if boundary.risk_class == ModuleRiskClass::Ordinary
        || boundary.boundary_id.trim().is_empty()
        || boundary.effect_ids.is_empty()
    {
        return invalid("approval boundaries must identify non-ordinary protected effects");
    }
    let expected_authority = match boundary.risk_class {
        ModuleRiskClass::Ordinary => unreachable!("ordinary boundaries rejected above"),
        ModuleRiskClass::DestructiveMigration => "module.migrate.destructive",
        ModuleRiskClass::DataDeletion
        | ModuleRiskClass::BackupRestore
        | ModuleRiskClass::BackupWaiver => "module.data.delete",
        ModuleRiskClass::TrustOverride => "module.trust.override",
    };
    if boundary.required_authority != expected_authority {
        return invalid("approval boundary uses the wrong scoped authority");
    }
    require_sorted_unique(
        boundary.effect_ids.iter().map(String::as_str),
        "boundary effects",
    )?;
    for effect_id in &boundary.effect_ids {
        let effect = effects
            .iter()
            .find(|effect| effect.effect_id() == effect_id)
            .ok_or_else(|| {
                ModuleManagementError::InvalidContract(format!(
                    "approval boundary references unknown effect `{effect_id}`"
                ))
            })?;
        if effect.risk_class() != boundary.risk_class {
            return invalid("approval risk class does not match its protected effect");
        }
    }
    Ok(())
}

fn require_authority(
    actor: &ManagementActor,
    authority: &str,
) -> Result<(), ModuleManagementError> {
    if actor.verified_authorities.contains(authority) {
        Ok(())
    } else {
        Err(ModuleManagementError::MissingAuthority(
            authority.to_owned(),
        ))
    }
}

fn initial_event(operation: &ModuleOperation) -> ModuleOperationJournalEvent {
    ModuleOperationJournalEvent {
        protocol: MODULE_OPERATION_JOURNAL_PROTOCOL.to_owned(),
        event_id: format!("{}-0", operation.operation_id),
        operation_id: operation.operation_id.clone(),
        attempt: operation.attempt,
        revision: 0,
        prior_event_digest: None,
        plan_digest: operation.plan_digest.clone(),
        actor_id: operation.actor_id.clone(),
        fencing_token: operation.fencing_token,
        transition: ModuleOperationTransition {
            from: ModuleOperationState::Planned,
            to: operation.state,
        },
        outcome_code: "operation_created".to_owned(),
        evidence_references: Vec::new(),
        operation_after: operation.clone(),
        recorded_at: operation.updated_at,
    }
}

#[allow(clippy::too_many_arguments)]
fn next_event(
    before: &ModuleOperation,
    after: ModuleOperation,
    actor_id: String,
    outcome_code: String,
    evidence_references: Vec<ArtifactReference>,
    now: DateTime<Utc>,
    prior: Option<&ModuleOperationJournalEvent>,
) -> Result<ModuleOperationJournalEvent, ModuleManagementError> {
    let prior_event_digest = prior
        .map(journal_event_digest)
        .transpose()
        .map_err(json_error)?;
    Ok(ModuleOperationJournalEvent {
        protocol: MODULE_OPERATION_JOURNAL_PROTOCOL.to_owned(),
        event_id: format!("{}-{}", after.operation_id, after.revision),
        operation_id: after.operation_id.clone(),
        attempt: after.attempt,
        revision: after.revision,
        prior_event_digest,
        plan_digest: after.plan_digest.clone(),
        actor_id,
        fencing_token: after.fencing_token,
        transition: ModuleOperationTransition {
            from: before.state,
            to: after.state,
        },
        outcome_code,
        evidence_references,
        operation_after: after,
        recorded_at: now,
    })
}

fn legal_transition(from: ModuleOperationState, to: ModuleOperationState) -> bool {
    use ModuleOperationState as State;
    matches!(
        (from, to),
        (
            State::Planned,
            State::AwaitingApproval | State::Ready | State::Blocked | State::Cancelled
        ) | (
            State::AwaitingApproval,
            State::Ready | State::Blocked | State::Cancelled
        ) | (
            State::Ready,
            State::ApplyingFiles | State::Blocked | State::Cancelled
        ) | (
            State::ApplyingFiles,
            State::FilesApplied | State::Restored | State::RepairRequired
        ) | (
            State::FilesApplied,
            State::StagingConfiguration
                | State::Migrating
                | State::Verifying
                | State::Blocked
                | State::RepairRequired
        ) | (
            State::StagingConfiguration,
            State::Migrating | State::Verifying | State::Blocked | State::RepairRequired
        ) | (
            State::Migrating,
            State::Verifying | State::Blocked | State::RepairRequired
        ) | (
            State::Verifying,
            State::Activating | State::Blocked | State::RepairRequired
        ) | (
            State::Activating,
            State::Succeeded | State::Blocked | State::RepairRequired
        )
    )
}

fn require_sorted_unique<'a>(
    values: impl Iterator<Item = &'a str>,
    subject: &str,
) -> Result<(), ModuleManagementError> {
    let values = values.collect::<Vec<_>>();
    if values.windows(2).all(|pair| pair[0] < pair[1]) {
        Ok(())
    } else {
        invalid(format!("{subject} must be sorted and unique"))
    }
}

fn valid_module_id(value: &str) -> bool {
    value.split_once('/').is_some_and(|(namespace, name)| {
        valid_identifier(namespace) && valid_identifier(name) && !name.contains('/')
    })
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && value != ".env"
        && !value.ends_with("/.env")
}

fn raw_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    format!("sha256:{hex}")
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ModuleManagementError> {
    Err(ModuleManagementError::InvalidContract(message.into()))
}

#[allow(clippy::needless_pass_by_value)]
fn json_error(error: serde_json::Error) -> ModuleManagementError {
    ModuleManagementError::InvalidContract(error.to_string())
}
