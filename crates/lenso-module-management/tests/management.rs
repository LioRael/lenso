use chrono::{TimeZone as _, Utc};
use lenso_module_management::*;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::sync::{Arc, Barrier};

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn content_digest(content: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(content.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    format!("sha256:{hex}")
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 30, 0, 0, 0).unwrap()
}

fn actor() -> ManagementActor {
    ManagementActor {
        actor_id: "user:operator".to_owned(),
        verified_authorities: BTreeSet::from(["module.manage".to_owned()]),
    }
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

fn plan(protected: bool) -> ModuleChangePlan {
    let desired = DesiredModuleComposition {
        protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
        application_id: "app-1".to_owned(),
        revision: 2,
        selected: Vec::new(),
        local_overrides: Vec::new(),
    };
    let desired_digest = desired_composition_digest(&desired).unwrap();
    let module_lock = ApplicationModuleLock {
        protocol: APPLICATION_MODULE_LOCK_PROTOCOL.to_owned(),
        application_id: "app-1".to_owned(),
        desired_composition_digest: desired_digest.clone(),
        catalog_snapshot_digest: digest('a'),
        trust_policy_digest: digest('b'),
        resolver_version: "resolver-1".to_owned(),
        modules: Vec::new(),
        capability_bindings: Vec::new(),
    };
    let lock_digest = application_module_lock_digest(&module_lock).unwrap();
    let effects = if protected {
        vec![ModulePlanEffect::Protected {
            effect_id: "delete-data".to_owned(),
            risk_class: ModuleRiskClass::DataDeletion,
            subject: "store:app-1".to_owned(),
            evidence_digest: digest('c'),
        }]
    } else {
        vec![ModulePlanEffect::WorkspaceFile {
            effect_id: "write-composition".to_owned(),
            path: "generated/lenso-linked/Cargo.toml".to_owned(),
            ownership: ModuleFileOwnership::Generated,
            change: ModuleFileChange::Modify,
            before_digest: Some(digest('d')),
            after_digest: Some(content_digest("reviewed candidate")),
            after_content: Some("reviewed candidate".to_owned()),
            after_mode: Some(0o644),
            patch: "@@ exact patch @@".to_owned(),
            reversible_before_migration: true,
        }]
    };
    let boundaries = if protected {
        vec![ModuleApprovalBoundary {
            boundary_id: "data-deletion".to_owned(),
            risk_class: ModuleRiskClass::DataDeletion,
            required_authority: "module.data.delete".to_owned(),
            effect_ids: vec!["delete-data".to_owned()],
            backup_evidence_digest: Some(digest('f')),
        }]
    } else {
        Vec::new()
    };
    let mut plan = ModuleChangePlan {
        protocol: MODULE_CHANGE_PLAN_PROTOCOL.to_owned(),
        plan_id: if protected {
            "plan-protected"
        } else {
            "plan-ordinary"
        }
        .to_owned(),
        plan_digest: String::new(),
        application_id: "app-1".to_owned(),
        environment_id: "local".to_owned(),
        expected_target_revision: 1,
        request: ModuleRootChange::Uninstall {
            module_id: "acme/support-ticket".to_owned(),
        },
        current_desired_digest: digest('1'),
        target_desired: desired,
        target_desired_digest: desired_digest,
        current_lock_digest: Some(digest('2')),
        target_lock: module_lock,
        target_lock_digest: lock_digest,
        catalog_snapshot_digest: digest('a'),
        resolver_version: "resolver-1".to_owned(),
        trust_policy_digest: digest('b'),
        compatibility_evidence_digest: digest('3'),
        cargo_lock_candidate: None,
        read_set: vec![ModulePathPrecondition {
            path: "generated/lenso-linked/Cargo.toml".to_owned(),
            existence: PathExistence::Present,
            content_digest: Some(digest('d')),
            file_type: ManagedFileType::Regular,
            mode: Some(0o644),
        }],
        effects,
        approval_boundaries: boundaries,
        validation_commands: vec!["cargo metadata --locked".to_owned()],
        next_actions: vec!["review_plan".to_owned()],
        created_at: now(),
    };
    plan.plan_digest = module_change_plan_digest(&plan).unwrap();
    plan
}

fn start<'a>(
    plan: &'a ModuleChangePlan,
    policy: &'a ModuleEnvironmentPolicy,
    actor: &'a ManagementActor,
) -> StartModuleOperation<'a> {
    StartModuleOperation {
        operation_id: "operation-1",
        idempotency_key: "request-1",
        operation_kind: ModuleOperationKind::Uninstall,
        plan,
        policy,
        actor,
        approvals: Vec::new(),
        holder_id: "worker-a",
        now: now(),
    }
}

fn start_named<'a>(
    operation_id: &'a str,
    idempotency_key: &'a str,
    operation_kind: ModuleOperationKind,
    plan: &'a ModuleChangePlan,
    policy: &'a ModuleEnvironmentPolicy,
    actor: &'a ManagementActor,
) -> StartModuleOperation<'a> {
    StartModuleOperation {
        operation_id,
        idempotency_key,
        operation_kind,
        plan,
        policy,
        actor,
        approvals: Vec::new(),
        holder_id: "worker-a",
        now: now(),
    }
}

#[test]
fn ordinary_operation_is_idempotent_fenced_and_revision_checked() {
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let plan = plan(false);
    let policy = policy();
    let actor = actor();
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    assert_eq!(operation.state, ModuleOperationState::Ready);
    assert_eq!(operation.fencing_token, 1);
    assert_eq!(
        engine.start(start(&plan, &policy, &actor)).unwrap(),
        operation
    );

    let applying = engine
        .advance(AdvanceModuleOperation {
            operation_id: operation.operation_id.clone(),
            expected_revision: 0,
            fencing_token: 1,
            next_state: ModuleOperationState::ApplyingFiles,
            actor_id: actor.actor_id.clone(),
            outcome_code: "workspace_apply_started".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: vec!["apply_next_guarded_file".to_owned()],
            now: now(),
        })
        .unwrap();
    assert_eq!(applying.revision, 1);
    assert!(matches!(
        engine.advance(AdvanceModuleOperation {
            operation_id: operation.operation_id,
            expected_revision: 0,
            fencing_token: 1,
            next_state: ModuleOperationState::Succeeded,
            actor_id: actor.actor_id,
            outcome_code: "invalid".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: Vec::new(),
            now: now(),
        }),
        Err(ModuleManagementError::Store(
            ModuleOperationStoreError::RevisionConflict { .. }
        ))
    ));
}

#[test]
fn protected_operation_waits_without_holding_lease_then_accepts_one_exact_approval() {
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let plan = plan(true);
    let policy = policy();
    let actor = actor();
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    assert_eq!(operation.state, ModuleOperationState::AwaitingApproval);
    assert_eq!(operation.fencing_token, 0);
    assert!(engine.store().load_lease().unwrap().is_none());

    let approval = ModuleApproval {
        protocol: MODULE_APPROVAL_PROTOCOL.to_owned(),
        approval_id: "approval-1".to_owned(),
        plan_digest: plan.plan_digest.clone(),
        application_id: plan.application_id.clone(),
        environment_id: plan.environment_id.clone(),
        expected_target_revision: plan.expected_target_revision,
        boundary_id: "data-deletion".to_owned(),
        risk_class: ModuleRiskClass::DataDeletion,
        actor_id: "user:approver".to_owned(),
        verified_authorities: vec!["module.data.delete".to_owned()],
        reason: "approved exact data scope".to_owned(),
        issued_at: now(),
        expires_at: now() + chrono::Duration::minutes(30),
        nonce: "nonce-1".to_owned(),
    };
    let ready = engine
        .submit_approval(
            &operation.operation_id,
            0,
            &plan,
            &policy,
            &actor,
            approval,
            "worker-a",
            now(),
        )
        .unwrap();
    assert_eq!(ready.state, ModuleOperationState::Ready);
    assert_eq!(ready.approvals.len(), 1);
    assert_eq!(ready.fencing_token, 1);
}

#[test]
fn lease_takeover_increments_fencing_token_only_after_expiry() {
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let first = engine
        .acquire_lease("app-1", "worker-a", 60, now())
        .unwrap();
    assert!(matches!(
        engine.acquire_lease("app-1", "worker-b", 60, now()),
        Err(ModuleManagementError::LeaseHeld(_))
    ));
    let second = engine
        .acquire_lease(
            "app-1",
            "worker-b",
            60,
            now() + chrono::Duration::seconds(61),
        )
        .unwrap();
    assert_eq!(second.fencing_token, first.fencing_token + 1);
}

#[test]
fn file_store_reconstructs_authoritative_state_from_digest_chained_journal() {
    let root = std::env::temp_dir().join(format!(
        "lenso-module-management-{}-{}",
        std::process::id(),
        now().timestamp_nanos_opt().unwrap()
    ));
    let plan = plan(false);
    let policy = policy();
    let actor = actor();
    let engine = ModuleManagementEngine::new(JsonFileModuleOperationStore::new(&root));
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    drop(engine);

    let reopened = JsonFileModuleOperationStore::new(&root);
    assert_eq!(reopened.load(&operation.operation_id).unwrap(), operation);
    assert_eq!(
        reopened
            .journal(&operation.operation_id)
            .unwrap()
            .events
            .len(),
        1
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_store_atomically_deduplicates_concurrent_idempotency_keys() {
    let root = std::env::temp_dir().join(format!(
        "lenso-module-management-idempotency-{}-{}",
        std::process::id(),
        now().timestamp_nanos_opt().unwrap()
    ));
    let barrier = Arc::new(Barrier::new(2));
    let handles = ["operation-a", "operation-b"].map(|operation_id| {
        let root = root.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let plan = plan(true);
            let policy = policy();
            let actor = actor();
            let engine = ModuleManagementEngine::new(JsonFileModuleOperationStore::new(root));
            barrier.wait();
            engine
                .start(start_named(
                    operation_id,
                    "same-request",
                    ModuleOperationKind::Uninstall,
                    &plan,
                    &policy,
                    &actor,
                ))
                .unwrap()
        })
    });
    let mut results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    results.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    assert_eq!(results[0], results[1], "threads must load one operation");
    assert!(matches!(
        results[0].operation_id.as_str(),
        "operation-a" | "operation-b"
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn crashed_worker_takeover_creates_a_new_attempt_without_repeating_receipts() {
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let plan = plan(false);
    let policy = policy();
    let actor = actor();
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    let applying = engine
        .advance(AdvanceModuleOperation {
            operation_id: operation.operation_id,
            expected_revision: 0,
            fencing_token: 1,
            next_state: ModuleOperationState::ApplyingFiles,
            actor_id: actor.actor_id,
            outcome_code: "workspace_apply_started".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: vec!["write-composition".to_owned()],
            now: now(),
        })
        .unwrap();
    let resumed = engine
        .resume_after_crash(
            &applying.operation_id,
            1,
            &ModuleResumeEvidence {
                plan_digest: plan.plan_digest,
                observed_target_digest: digest('8'),
                completed_effect_ids: Vec::new(),
                next_effect_id: "write-composition".to_owned(),
                next_effect_idempotent: true,
                observed_at: now() + chrono::Duration::seconds(61),
            },
            "worker-b",
            60,
            "service:management-worker",
            now() + chrono::Duration::seconds(61),
        )
        .unwrap();
    assert_eq!(resumed.attempt, 2);
    assert_eq!(resumed.fencing_token, 2);
    assert_eq!(resumed.state, ModuleOperationState::ApplyingFiles);
}

#[test]
fn repair_plan_cannot_repeat_a_completed_effect() {
    let mut repair = ModuleRepairPlan {
        protocol: MODULE_REPAIR_PLAN_PROTOCOL.to_owned(),
        repair_plan_id: "repair-1".to_owned(),
        repair_plan_digest: String::new(),
        original_operation_id: "operation-1".to_owned(),
        original_operation_revision: 4,
        application_id: "app-1".to_owned(),
        environment_id: "local".to_owned(),
        observed_state_digest: digest('9'),
        completed_effect_ids: vec!["migration-1".to_owned()],
        actions: vec![ModuleRepairAction::Resume {
            effect_ids: vec!["migration-1".to_owned()],
        }],
        approval_boundaries: Vec::new(),
        created_at: now(),
    };
    repair.repair_plan_digest = module_repair_plan_digest(&repair).unwrap();
    assert!(matches!(
        validate_repair_plan(&repair),
        Err(ModuleManagementError::InvalidContract(_))
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn reconciliation_requires_the_exact_succeeded_repair_operation() {
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let ordinary_plan = plan(false);
    let policy = policy();
    let actor = actor();
    let original = engine
        .start(start(&ordinary_plan, &policy, &actor))
        .unwrap();
    let applying = engine
        .advance(AdvanceModuleOperation {
            operation_id: original.operation_id.clone(),
            expected_revision: 0,
            fencing_token: original.fencing_token,
            next_state: ModuleOperationState::ApplyingFiles,
            actor_id: actor.actor_id.clone(),
            outcome_code: "workspace_apply_started".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: Vec::new(),
            now: now(),
        })
        .unwrap();
    let repair_required = engine
        .advance(AdvanceModuleOperation {
            operation_id: original.operation_id.clone(),
            expected_revision: applying.revision,
            fencing_token: applying.fencing_token,
            next_state: ModuleOperationState::RepairRequired,
            actor_id: actor.actor_id.clone(),
            outcome_code: "partial_effect_requires_repair".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: vec!["create_repair_plan".to_owned()],
            now: now(),
        })
        .unwrap();
    assert!(matches!(
        engine.advance(AdvanceModuleOperation {
            operation_id: original.operation_id.clone(),
            expected_revision: repair_required.revision,
            fencing_token: repair_required.fencing_token,
            next_state: ModuleOperationState::Reconciled,
            actor_id: actor.actor_id.clone(),
            outcome_code: "bypass".to_owned(),
            evidence_references: Vec::new(),
            error: None,
            next_actions: Vec::new(),
            now: now(),
        }),
        Err(ModuleManagementError::IllegalTransition { .. })
    ));

    let mut repair_plan = ModuleRepairPlan {
        protocol: MODULE_REPAIR_PLAN_PROTOCOL.to_owned(),
        repair_plan_id: "repair-1".to_owned(),
        repair_plan_digest: String::new(),
        original_operation_id: original.operation_id.clone(),
        original_operation_revision: repair_required.revision,
        application_id: original.application_id.clone(),
        environment_id: original.environment_id.clone(),
        observed_state_digest: digest('9'),
        completed_effect_ids: Vec::new(),
        actions: vec![ModuleRepairAction::Resume {
            effect_ids: vec!["write-composition".to_owned()],
        }],
        approval_boundaries: Vec::new(),
        created_at: now(),
    };
    repair_plan.repair_plan_digest = module_repair_plan_digest(&repair_plan).unwrap();
    let mut repair_change_plan = plan(false);
    repair_change_plan.plan_id = "repair-change-plan".to_owned();
    repair_change_plan.request = ModuleRootChange::Repair {
        repair_plan_digest: repair_plan.repair_plan_digest.clone(),
    };
    repair_change_plan.plan_digest = module_change_plan_digest(&repair_change_plan).unwrap();
    let repair_operation = engine
        .start(start_named(
            "operation-2",
            "repair-request-1",
            ModuleOperationKind::Repair,
            &repair_change_plan,
            &policy,
            &actor,
        ))
        .unwrap();
    let mut current = repair_operation;
    for next_state in [
        ModuleOperationState::ApplyingFiles,
        ModuleOperationState::FilesApplied,
        ModuleOperationState::Verifying,
        ModuleOperationState::Activating,
        ModuleOperationState::Succeeded,
    ] {
        current = engine
            .advance(AdvanceModuleOperation {
                operation_id: current.operation_id.clone(),
                expected_revision: current.revision,
                fencing_token: current.fencing_token,
                next_state,
                actor_id: actor.actor_id.clone(),
                outcome_code: "repair_advanced".to_owned(),
                evidence_references: Vec::new(),
                error: None,
                next_actions: Vec::new(),
                now: now(),
            })
            .unwrap();
    }
    let reconciled = engine
        .reconcile_with_succeeded_repair(
            &original.operation_id,
            repair_required.revision,
            &repair_plan,
            &repair_change_plan,
            &current.operation_id,
            &actor.actor_id,
            Vec::new(),
            now(),
        )
        .unwrap();
    assert_eq!(reconciled.state, ModuleOperationState::Reconciled);
    assert_eq!(
        reconciled.repair_operation_id.as_deref(),
        Some("operation-2")
    );
}
