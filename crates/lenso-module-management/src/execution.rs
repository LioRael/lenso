use crate::{
    AdvanceModuleOperation, JsonFileModuleOperationStore, LinkedWorkspaceError,
    LinkedWorkspaceTransaction, MODULE_APPROVAL_PROTOCOL, ManagementActor, ModuleApproval,
    ModuleChangePlan, ModuleEffectOutcome, ModuleEffectReceipt, ModuleEnvironmentPolicy,
    ModuleManagementEngine, ModuleManagementError, ModuleOperation, ModuleOperationError,
    ModuleOperationJournal, ModuleOperationKind, ModuleOperationState, ModuleOperationStore,
    ModuleOperationStoreError, ModulePlanEffect, ModuleRootChange, StartModuleOperation,
    WorkspaceModuleManagement, WorkspaceModuleManagementError,
};
use chrono::{DateTime, Duration, Utc};
use lenso_contracts::{ArtifactReference, digest_json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const POLICY_PATH: &str = ".lenso/module-environment-policy.json";
const MANAGEMENT_ROOT: &str = ".lenso/module-management";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleEffectExecution {
    pub outcome: ModuleEffectOutcome,
    pub evidence_references: Vec<ArtifactReference>,
}

pub trait ModuleEffectAdapter: std::fmt::Debug + Send + Sync {
    fn execute(
        &self,
        workspace_root: &Path,
        operation: &ModuleOperation,
        effect: &ModulePlanEffect,
    ) -> Result<ModuleEffectExecution, ModuleEffectAdapterError>;
}

#[derive(Debug, Error)]
pub enum ModuleEffectAdapterError {
    #[error("effect adapter does not support `{effect_id}`: {reason}")]
    Unsupported { effect_id: String, reason: String },
    #[error("effect `{effect_id}` failed: {reason}")]
    Failed { effect_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReviewedModulePlan {
    pub idempotency_key: String,
    pub plan: ModuleChangePlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveModuleOperation {
    pub expected_revision: u64,
    pub boundary_id: String,
    pub reason: String,
    pub nonce: String,
}

#[derive(Debug, Error)]
pub enum WorkspaceModuleOperatorError {
    #[error(transparent)]
    Workspace(#[from] WorkspaceModuleManagementError),
    #[error(transparent)]
    Management(#[from] ModuleManagementError),
    #[error(transparent)]
    Store(#[from] ModuleOperationStoreError),
    #[error(transparent)]
    Linked(#[from] LinkedWorkspaceError),
    #[error("Module operation artifact I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Module operation artifact JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("reviewed plan is stale or was not produced by this target")]
    StalePlan,
    #[error("Module environment policy is unavailable")]
    PolicyUnavailable,
    #[error("operation cannot be cancelled after target mutation began")]
    CancellationUnsafe,
}

#[derive(Debug)]
pub struct WorkspaceModuleOperator<A> {
    root: PathBuf,
    adapter: A,
}

impl<A: ModuleEffectAdapter> WorkspaceModuleOperator<A> {
    pub fn new(root: impl Into<PathBuf>, adapter: A) -> Self {
        Self {
            root: root.into(),
            adapter,
        }
    }

    pub fn start(
        &self,
        request: &StartReviewedModulePlan,
        actor: &ManagementActor,
        holder_id: &str,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        if request.idempotency_key.trim().is_empty() {
            return Err(WorkspaceModuleOperatorError::StalePlan);
        }
        let verified = WorkspaceModuleManagement::new(&self.root)
            .preview(request.plan.request.clone(), request.plan.created_at)?;
        if verified != request.plan {
            return Err(WorkspaceModuleOperatorError::StalePlan);
        }
        let policy = self.policy()?;
        self.persist_plan(&verified)?;
        let operation_id = content_id(
            "module-op",
            &(
                verified.application_id.as_str(),
                request.idempotency_key.as_str(),
            ),
        )?;
        Ok(self.engine().start(StartModuleOperation {
            operation_id: &operation_id,
            idempotency_key: &request.idempotency_key,
            operation_kind: operation_kind(&verified.request),
            plan: &verified,
            policy: &policy,
            actor,
            approvals: Vec::new(),
            holder_id,
            now,
        })?)
    }

    pub fn operation(
        &self,
        operation_id: &str,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        Ok(self.engine().store().load(operation_id)?)
    }

    pub fn journal(
        &self,
        operation_id: &str,
    ) -> Result<ModuleOperationJournal, WorkspaceModuleOperatorError> {
        Ok(self.engine().store().journal(operation_id)?)
    }

    pub fn approve(
        &self,
        operation_id: &str,
        request: ApproveModuleOperation,
        actor: &ManagementActor,
        holder_id: &str,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        let operation = self.operation(operation_id)?;
        let plan = self.load_plan(&operation.plan_digest)?;
        let policy = self.policy()?;
        let boundary = plan
            .approval_boundaries
            .iter()
            .find(|boundary| boundary.boundary_id == request.boundary_id)
            .ok_or(WorkspaceModuleOperatorError::StalePlan)?;
        let approval = ModuleApproval {
            protocol: MODULE_APPROVAL_PROTOCOL.to_owned(),
            approval_id: content_id(
                "module-approval",
                &(
                    operation_id,
                    request.boundary_id.as_str(),
                    request.nonce.as_str(),
                ),
            )?,
            plan_digest: plan.plan_digest.clone(),
            application_id: plan.application_id.clone(),
            environment_id: plan.environment_id.clone(),
            expected_target_revision: plan.expected_target_revision,
            boundary_id: boundary.boundary_id.clone(),
            risk_class: boundary.risk_class,
            actor_id: actor.actor_id.clone(),
            verified_authorities: actor.verified_authorities.iter().cloned().collect(),
            reason: request.reason,
            issued_at: now,
            expires_at: now
                + Duration::seconds(
                    i64::try_from(policy.maximum_approval_age_seconds).unwrap_or(i64::MAX),
                ),
            nonce: request.nonce,
        };
        Ok(self.engine().submit_approval(
            operation_id,
            request.expected_revision,
            &plan,
            &policy,
            actor,
            approval,
            holder_id,
            now,
        )?)
    }

    pub fn apply(
        &self,
        operation_id: &str,
        actor: &ManagementActor,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        let plan = self.load_bound_plan(operation_id)?;
        let engine = self.engine();
        let mut operation = engine.store().load(operation_id)?;
        if matches!(
            operation.state,
            ModuleOperationState::Ready | ModuleOperationState::ApplyingFiles
        ) {
            operation = LinkedWorkspaceTransaction::new(&self.root).apply(
                &engine,
                operation_id,
                &plan,
                operation.fencing_token,
                &actor.actor_id,
                now,
            )?;
        }
        operation = self.run_from_state(&engine, operation, &plan, actor, now)?;
        if matches!(
            operation.state,
            ModuleOperationState::Succeeded | ModuleOperationState::Blocked
        ) {
            let _ = engine.release_lease(MANAGEMENT_HOLDER, operation.fencing_token);
        }
        Ok(operation)
    }

    pub fn retry(
        &self,
        operation_id: &str,
        expected_revision: u64,
        actor: &ManagementActor,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        let plan = self.load_bound_plan(operation_id)?;
        let current = self.operation(operation_id)?;
        let next_state = next_incomplete_state(&current, &plan);
        self.engine().retry_blocked(
            operation_id,
            expected_revision,
            next_state,
            MANAGEMENT_HOLDER,
            self.policy()?.maximum_lease_seconds,
            &actor.actor_id,
            now,
        )?;
        self.apply(operation_id, actor, now)
    }

    pub fn resume(
        &self,
        operation_id: &str,
        expected_revision: u64,
        actor: &ManagementActor,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        let plan = self.load_bound_plan(operation_id)?;
        let current = self.operation(operation_id)?;
        if current.revision != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: operation_id.to_owned(),
                expected: expected_revision,
                observed: current.revision,
            }
            .into());
        }
        let evidence =
            LinkedWorkspaceTransaction::new(&self.root).resume_evidence(&current, &plan, now)?;
        let policy = self.policy()?;
        self.engine().resume_after_crash(
            operation_id,
            expected_revision,
            &evidence,
            MANAGEMENT_HOLDER,
            policy.maximum_lease_seconds,
            &actor.actor_id,
            now,
        )?;
        self.apply(operation_id, actor, now)
    }

    pub fn cancel(
        &self,
        operation_id: &str,
        expected_revision: u64,
        actor: &ManagementActor,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        let current = self.operation(operation_id)?;
        if !matches!(
            current.state,
            ModuleOperationState::AwaitingApproval | ModuleOperationState::Ready
        ) {
            return Err(WorkspaceModuleOperatorError::CancellationUnsafe);
        }
        let operation = self.engine().advance(AdvanceModuleOperation {
            operation_id: operation_id.to_owned(),
            expected_revision,
            fencing_token: current.fencing_token,
            next_state: ModuleOperationState::Cancelled,
            actor_id: actor.actor_id.clone(),
            outcome_code: "operation_cancelled_before_mutation".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: Vec::new(),
            now,
        })?;
        if current.fencing_token != 0 {
            let _ = self
                .engine()
                .release_lease(MANAGEMENT_HOLDER, current.fencing_token);
        }
        Ok(operation)
    }

    fn run_from_state(
        &self,
        engine: &ModuleManagementEngine<JsonFileModuleOperationStore>,
        mut operation: ModuleOperation,
        plan: &ModuleChangePlan,
        actor: &ManagementActor,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        if operation.state == ModuleOperationState::FilesApplied {
            operation = transition(
                engine,
                operation,
                ModuleOperationState::StagingConfiguration,
                &actor.actor_id,
                "configuration_stage_started",
                now,
            )?;
        }
        if operation.state == ModuleOperationState::StagingConfiguration {
            operation = self.execute_matching(engine, operation, plan, actor, now, |effect| {
                matches!(
                    effect,
                    ModulePlanEffect::Configuration { .. }
                        | ModulePlanEffect::Protected { .. }
                        | ModulePlanEffect::ConsoleComposition { .. }
                        | ModulePlanEffect::ServiceInstallation { .. }
                        | ModulePlanEffect::ServiceRemoval { .. }
                )
            })?;
            if operation.state == ModuleOperationState::Blocked {
                return Ok(operation);
            }
            operation = transition(
                engine,
                operation,
                ModuleOperationState::Migrating,
                &actor.actor_id,
                "migration_stage_started",
                now,
            )?;
        }
        if operation.state == ModuleOperationState::Migrating {
            operation = self.execute_matching(engine, operation, plan, actor, now, |effect| {
                matches!(effect, ModulePlanEffect::Migration { .. })
            })?;
            if operation.state == ModuleOperationState::Blocked {
                return Ok(operation);
            }
            operation = transition(
                engine,
                operation,
                ModuleOperationState::Verifying,
                &actor.actor_id,
                "verification_stage_started",
                now,
            )?;
        }
        if operation.state == ModuleOperationState::Verifying {
            operation = self.execute_matching(engine, operation, plan, actor, now, |effect| {
                matches!(effect, ModulePlanEffect::Validate { .. })
            })?;
            if operation.state == ModuleOperationState::Blocked {
                return Ok(operation);
            }
            operation = transition(
                engine,
                operation,
                ModuleOperationState::Activating,
                &actor.actor_id,
                "activation_stage_started",
                now,
            )?;
        }
        if operation.state == ModuleOperationState::Activating {
            operation = self.execute_matching(engine, operation, plan, actor, now, |effect| {
                matches!(
                    effect,
                    ModulePlanEffect::Restart { .. }
                        | ModulePlanEffect::ServiceRestart { .. }
                        | ModulePlanEffect::Activate { .. }
                )
            })?;
            if operation.state == ModuleOperationState::Blocked {
                return Ok(operation);
            }
            operation = transition(
                engine,
                operation,
                ModuleOperationState::Succeeded,
                &actor.actor_id,
                "reviewed_plan_succeeded",
                now,
            )?;
        }
        Ok(operation)
    }

    fn execute_matching(
        &self,
        engine: &ModuleManagementEngine<JsonFileModuleOperationStore>,
        mut operation: ModuleOperation,
        plan: &ModuleChangePlan,
        actor: &ManagementActor,
        now: DateTime<Utc>,
        matches: impl Fn(&ModulePlanEffect) -> bool,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        for effect in plan.effects.iter().filter(|effect| matches(effect)) {
            if operation
                .effect_receipts
                .iter()
                .any(|receipt| receipt.effect_id == effect.effect_id())
            {
                continue;
            }
            let execution = if matches!(effect, ModulePlanEffect::Protected { .. }) {
                Ok(ModuleEffectExecution {
                    outcome: ModuleEffectOutcome::Verified,
                    evidence_references: Vec::new(),
                })
            } else {
                self.adapter.execute(&self.root, &operation, effect)
            };
            let execution = match execution {
                Ok(execution) => execution,
                Err(error) => return Self::block(engine, operation, actor, effect, &error, now),
            };
            operation = engine.record_effect_receipt(
                &operation.operation_id,
                operation.revision,
                operation.fencing_token,
                &actor.actor_id,
                ModuleEffectReceipt {
                    receipt_id: format!("{}:{}", operation.operation_id, effect.effect_id()),
                    effect_id: effect.effect_id().to_owned(),
                    effect_digest: digest_json(effect)?,
                    operation_id: operation.operation_id.clone(),
                    attempt: operation.attempt,
                    fencing_token: operation.fencing_token,
                    outcome: execution.outcome,
                    evidence_references: execution.evidence_references,
                    committed_at: now,
                },
                now,
            )?;
        }
        Ok(operation)
    }

    fn block(
        engine: &ModuleManagementEngine<JsonFileModuleOperationStore>,
        operation: ModuleOperation,
        actor: &ManagementActor,
        effect: &ModulePlanEffect,
        error: &ModuleEffectAdapterError,
        now: DateTime<Utc>,
    ) -> Result<ModuleOperation, WorkspaceModuleOperatorError> {
        let code = match error {
            ModuleEffectAdapterError::Unsupported { .. } => "effect_adapter_unavailable",
            ModuleEffectAdapterError::Failed { .. } => "effect_execution_failed",
        };
        Ok(engine.advance(AdvanceModuleOperation {
            operation_id: operation.operation_id,
            expected_revision: operation.revision,
            fencing_token: operation.fencing_token,
            next_state: ModuleOperationState::Blocked,
            actor_id: actor.actor_id.clone(),
            outcome_code: code.to_owned(),
            evidence_references: Vec::new(),
            error: Some(ModuleOperationError {
                code: code.to_owned(),
                message: error.to_string(),
                evidence_references: Vec::new(),
                recorded_at: now,
            }),
            next_actions: vec![
                format!("configure_effect_adapter:{}", effect.effect_id()),
                "retry_operation".to_owned(),
            ],
            now,
        })?)
    }

    fn engine(&self) -> ModuleManagementEngine<JsonFileModuleOperationStore> {
        ModuleManagementEngine::new(JsonFileModuleOperationStore::new(
            self.root.join(MANAGEMENT_ROOT),
        ))
    }

    fn policy(&self) -> Result<ModuleEnvironmentPolicy, WorkspaceModuleOperatorError> {
        let bytes = fs::read(self.root.join(POLICY_PATH)).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                WorkspaceModuleOperatorError::PolicyUnavailable
            } else {
                error.into()
            }
        })?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn persist_plan(&self, plan: &ModuleChangePlan) -> Result<(), WorkspaceModuleOperatorError> {
        let root = self.root.join(MANAGEMENT_ROOT).join("plans");
        fs::create_dir_all(&root)?;
        let path = root.join(format!("{}.json", plan.plan_digest));
        let bytes = serde_json::to_vec_pretty(plan)?;
        if path.exists() {
            if fs::read(&path)? != bytes {
                return Err(WorkspaceModuleOperatorError::StalePlan);
            }
            return Ok(());
        }
        let temporary = root.join(format!("{}.next.json", plan.plan_digest));
        fs::write(&temporary, &bytes)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    fn load_plan(&self, digest: &str) -> Result<ModuleChangePlan, WorkspaceModuleOperatorError> {
        Ok(serde_json::from_slice(&fs::read(
            self.root
                .join(MANAGEMENT_ROOT)
                .join("plans")
                .join(format!("{digest}.json")),
        )?)?)
    }

    fn load_bound_plan(
        &self,
        operation_id: &str,
    ) -> Result<ModuleChangePlan, WorkspaceModuleOperatorError> {
        let operation = self.operation(operation_id)?;
        let plan = self.load_plan(&operation.plan_digest)?;
        if plan.plan_digest != operation.plan_digest {
            return Err(WorkspaceModuleOperatorError::StalePlan);
        }
        Ok(plan)
    }
}

pub const MANAGEMENT_HOLDER: &str = "module-management-api";

fn transition(
    engine: &ModuleManagementEngine<JsonFileModuleOperationStore>,
    operation: ModuleOperation,
    next_state: ModuleOperationState,
    actor_id: &str,
    outcome_code: &str,
    now: DateTime<Utc>,
) -> Result<ModuleOperation, ModuleManagementError> {
    engine.advance(AdvanceModuleOperation {
        operation_id: operation.operation_id,
        expected_revision: operation.revision,
        fencing_token: operation.fencing_token,
        next_state,
        actor_id: actor_id.to_owned(),
        outcome_code: outcome_code.to_owned(),
        evidence_references: Vec::new(),
        error: None,
        next_actions: Vec::new(),
        now,
    })
}

fn next_incomplete_state(
    operation: &ModuleOperation,
    plan: &ModuleChangePlan,
) -> ModuleOperationState {
    let completed = operation
        .effect_receipts
        .iter()
        .map(|receipt| receipt.effect_id.as_str())
        .collect::<BTreeSet<_>>();
    for effect in &plan.effects {
        if completed.contains(effect.effect_id()) {
            continue;
        }
        return match effect {
            ModulePlanEffect::Configuration { .. }
            | ModulePlanEffect::Protected { .. }
            | ModulePlanEffect::ConsoleComposition { .. }
            | ModulePlanEffect::ServiceInstallation { .. }
            | ModulePlanEffect::ServiceRemoval { .. }
            | ModulePlanEffect::WorkspaceFile { .. } => ModuleOperationState::StagingConfiguration,
            ModulePlanEffect::Migration { .. } => ModuleOperationState::Migrating,
            ModulePlanEffect::Validate { .. } => ModuleOperationState::Verifying,
            ModulePlanEffect::Restart { .. }
            | ModulePlanEffect::ServiceRestart { .. }
            | ModulePlanEffect::Activate { .. } => ModuleOperationState::Activating,
        };
    }
    ModuleOperationState::Activating
}

fn operation_kind(change: &ModuleRootChange) -> ModuleOperationKind {
    match change {
        ModuleRootChange::Install { .. } => ModuleOperationKind::Install,
        ModuleRootChange::Update { .. } | ModuleRootChange::SelectOptional { .. } => {
            ModuleOperationKind::Update
        }
        ModuleRootChange::Uninstall { .. } => ModuleOperationKind::Uninstall,
        ModuleRootChange::SwitchDelivery { .. } => ModuleOperationKind::DeliveryTransition,
        ModuleRootChange::Restore { .. } => ModuleOperationKind::Restore,
        ModuleRootChange::Repair { .. } => ModuleOperationKind::Repair,
    }
}

fn content_id<T: serde::Serialize>(prefix: &str, value: &T) -> Result<String, serde_json::Error> {
    let digest = digest_json(value)?;
    Ok(format!("{prefix}-{}", &digest[7..31]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        APPLICATION_MODULE_LOCK_PROTOCOL, ApplicationModuleLock,
        DESIRED_MODULE_COMPOSITION_PROTOCOL, DesiredModuleComposition, EnvironmentManagementMode,
        MODULE_CHANGE_PLAN_PROTOCOL, MODULE_ENVIRONMENT_POLICY_PROTOCOL, ModulePathPrecondition,
        application_module_lock_digest, desired_composition_digest, module_change_plan_digest,
    };
    use chrono::TimeZone as _;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

    #[derive(Debug)]
    struct FakeAdapter(AtomicBool);

    impl ModuleEffectAdapter for FakeAdapter {
        fn execute(
            &self,
            _root: &Path,
            _operation: &ModuleOperation,
            effect: &ModulePlanEffect,
        ) -> Result<ModuleEffectExecution, ModuleEffectAdapterError> {
            if matches!(effect, ModulePlanEffect::Validate { .. })
                && self.0.swap(false, Ordering::SeqCst)
            {
                return Err(ModuleEffectAdapterError::Failed {
                    effect_id: effect.effect_id().to_owned(),
                    reason: "injected failure".to_owned(),
                });
            }
            Ok(ModuleEffectExecution {
                outcome: match effect {
                    ModulePlanEffect::Validate { .. } => ModuleEffectOutcome::Verified,
                    ModulePlanEffect::Activate { .. } => ModuleEffectOutcome::Activated,
                    _ => ModuleEffectOutcome::Applied,
                },
                evidence_references: Vec::new(),
            })
        }
    }

    #[test]
    fn operator_runs_all_remaining_phases_through_one_interface() {
        let (operator, engine, operation, plan, actor, root) = fixture(false);
        let completed = operator
            .run_from_state(&engine, operation, &plan, &actor, now())
            .unwrap();
        assert_eq!(completed.state, ModuleOperationState::Succeeded);
        assert_eq!(completed.effect_receipts.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retry_resumes_the_first_incomplete_phase_without_repeating_receipts() {
        let (operator, engine, operation, plan, actor, root) = fixture(true);
        let blocked = operator
            .run_from_state(&engine, operation, &plan, &actor, now())
            .unwrap();
        assert_eq!(blocked.state, ModuleOperationState::Blocked);
        assert!(blocked.effect_receipts.is_empty());
        let completed = operator
            .retry(&blocked.operation_id, blocked.revision, &actor, now())
            .unwrap();
        assert_eq!(completed.state, ModuleOperationState::Succeeded);
        assert_eq!(completed.attempt, 2);
        assert_eq!(completed.effect_receipts.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }

    #[allow(clippy::type_complexity)]
    fn fixture(
        fail_once: bool,
    ) -> (
        WorkspaceModuleOperator<FakeAdapter>,
        ModuleManagementEngine<JsonFileModuleOperationStore>,
        ModuleOperation,
        ModuleChangePlan,
        ManagementActor,
        PathBuf,
    ) {
        let root = std::env::temp_dir().join(format!(
            "lenso-module-operator-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".lenso")).unwrap();
        fs::write(
            root.join(POLICY_PATH),
            serde_json::to_vec_pretty(&policy()).unwrap(),
        )
        .unwrap();
        let operator = WorkspaceModuleOperator::new(&root, FakeAdapter(AtomicBool::new(fail_once)));
        let plan = plan();
        operator.persist_plan(&plan).unwrap();
        let actor = actor();
        let engine = operator.engine();
        let started = engine
            .start(StartModuleOperation {
                operation_id: "operation-1",
                idempotency_key: "request-1",
                operation_kind: ModuleOperationKind::Update,
                plan: &plan,
                policy: &policy(),
                actor: &actor,
                approvals: Vec::new(),
                holder_id: MANAGEMENT_HOLDER,
                now: now(),
            })
            .unwrap();
        let applying = transition(
            &engine,
            started,
            ModuleOperationState::ApplyingFiles,
            &actor.actor_id,
            "test_apply",
            now(),
        )
        .unwrap();
        let applied = transition(
            &engine,
            applying,
            ModuleOperationState::FilesApplied,
            &actor.actor_id,
            "test_files",
            now(),
        )
        .unwrap();
        (operator, engine, applied, plan, actor, root)
    }

    fn plan() -> ModuleChangePlan {
        let desired = DesiredModuleComposition {
            protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
            application_id: "app-1".to_owned(),
            revision: 2,
            selected: Vec::new(),
            local_overrides: Vec::new(),
        };
        let desired_digest = desired_composition_digest(&desired).unwrap();
        let target_lock = ApplicationModuleLock {
            protocol: APPLICATION_MODULE_LOCK_PROTOCOL.to_owned(),
            application_id: "app-1".to_owned(),
            desired_composition_digest: desired_digest.clone(),
            catalog_snapshot_digest: digest('a'),
            trust_policy_digest: digest('b'),
            resolver_version: "resolver-1".to_owned(),
            modules: Vec::new(),
            capability_bindings: Vec::new(),
        };
        let target_lock_digest = application_module_lock_digest(&target_lock).unwrap();
        let mut plan = ModuleChangePlan {
            protocol: MODULE_CHANGE_PLAN_PROTOCOL.to_owned(),
            plan_id: "plan-1".to_owned(),
            plan_digest: String::new(),
            application_id: "app-1".to_owned(),
            environment_id: "local".to_owned(),
            expected_target_revision: 1,
            request: ModuleRootChange::Update {
                module_id: "acme/example".to_owned(),
                version_requirement: "^1".to_owned(),
            },
            current_desired_digest: digest('1'),
            target_desired: desired,
            target_desired_digest: desired_digest,
            current_lock_digest: Some(digest('2')),
            target_lock,
            target_lock_digest: target_lock_digest.clone(),
            catalog_snapshot_digest: digest('a'),
            resolver_version: "resolver-1".to_owned(),
            trust_policy_digest: digest('b'),
            compatibility_evidence_digest: digest('c'),
            cargo_lock_candidate: None,
            read_set: Vec::<ModulePathPrecondition>::new(),
            effects: vec![
                ModulePlanEffect::Validate {
                    effect_id: "80-validate:test".to_owned(),
                    command: "cargo check --locked".to_owned(),
                    expected_evidence: digest('d'),
                },
                ModulePlanEffect::Activate {
                    effect_id: "90-activate:lock".to_owned(),
                    target_lock_digest,
                },
                ModulePlanEffect::Restart {
                    effect_id: "99-restart:host".to_owned(),
                    target: "host".to_owned(),
                },
            ],
            approval_boundaries: Vec::new(),
            validation_commands: vec!["cargo check --locked".to_owned()],
            next_actions: vec!["review_plan".to_owned()],
            created_at: now(),
        };
        plan.plan_digest = module_change_plan_digest(&plan).unwrap();
        plan
    }

    fn policy() -> ModuleEnvironmentPolicy {
        ModuleEnvironmentPolicy {
            protocol: MODULE_ENVIRONMENT_POLICY_PROTOCOL.to_owned(),
            policy_id: "local".to_owned(),
            revision: "policy-1".to_owned(),
            mode: EnvironmentManagementMode::Full,
            require_distinct_approver: false,
            maximum_approval_age_seconds: 3_600,
            maximum_lease_seconds: 60,
            require_backup_for_non_local_destructive_effects: true,
        }
    }

    fn actor() -> ManagementActor {
        ManagementActor {
            actor_id: "user:operator".to_owned(),
            verified_authorities: BTreeSet::from(["module.manage".to_owned()]),
        }
    }
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 30, 12, 0, 0).unwrap()
    }
    fn digest(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }
}
