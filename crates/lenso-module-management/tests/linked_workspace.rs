use chrono::{TimeZone as _, Utc};
use lenso_contracts::{
    LinkedModuleDelivery, ModuleDelivery, ModuleLifecycleState, ModuleVerificationCell,
    VerificationEvaluation, VerificationOperation, VerificationState,
};
use lenso_module_management::*;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 30, 1, 0, 0).unwrap()
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "lenso-linked-{name}-{}-{}",
        std::process::id(),
        now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn scaffold(root: &Path) {
    fs::create_dir_all(root.join(".lenso")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nlenso-linked-composition = { path = \"generated/lenso-linked\" }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn host() { let _ = HostBuilder::new().linked_modules(lenso_linked_composition::linked_modules()); }\n",
    )
    .unwrap();
    let seam = LinkedCompositionSeam {
        protocol: LINKED_COMPOSITION_SEAM_PROTOCOL.to_owned(),
        host_manifest_path: "Cargo.toml".to_owned(),
        host_source_path: "src/main.rs".to_owned(),
        generated_crate_path: "generated/lenso-linked".to_owned(),
        dependency_name: "lenso-linked-composition".to_owned(),
        lenso_version: "0.3.33".to_owned(),
    };
    fs::write(
        root.join(".lenso/linked-composition-seam.json"),
        format!("{}\n", serde_json::to_string_pretty(&seam).unwrap()),
    )
    .unwrap();
}

fn desired() -> DesiredModuleComposition {
    DesiredModuleComposition {
        protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
        application_id: "app-1".to_owned(),
        revision: 1,
        selected: Vec::new(),
        local_overrides: Vec::new(),
    }
}

fn module_lock(desired: &DesiredModuleComposition) -> ApplicationModuleLock {
    ApplicationModuleLock {
        protocol: APPLICATION_MODULE_LOCK_PROTOCOL.to_owned(),
        application_id: desired.application_id.clone(),
        desired_composition_digest: desired_composition_digest(desired).unwrap(),
        catalog_snapshot_digest: digest('a'),
        trust_policy_digest: digest('b'),
        resolver_version: "resolver-1".to_owned(),
        modules: Vec::new(),
        capability_bindings: Vec::new(),
    }
}

fn linked_desired_and_lock() -> (DesiredModuleComposition, ApplicationModuleLock) {
    let desired = DesiredModuleComposition {
        protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
        application_id: "app-1".to_owned(),
        revision: 1,
        selected: vec![DesiredModuleSelection {
            module_id: "acme/example".to_owned(),
            version_requirement: "^1.2".to_owned(),
            optional_requirements: Vec::new(),
            exact_release_digest: None,
            delivery_preference: Some(ManagedDeliveryKind::Linked),
        }],
        local_overrides: vec![LocalModuleOverride {
            module_id: "acme/example".to_owned(),
            path: "../example-module".to_owned(),
            content_digest: digest('d'),
            acknowledged_unverified: true,
        }],
    };
    let module_lock = ApplicationModuleLock {
        protocol: APPLICATION_MODULE_LOCK_PROTOCOL.to_owned(),
        application_id: desired.application_id.clone(),
        desired_composition_digest: desired_composition_digest(&desired).unwrap(),
        catalog_snapshot_digest: digest('a'),
        trust_policy_digest: digest('b'),
        resolver_version: "resolver-1".to_owned(),
        modules: vec![LockedModule {
            module_id: "acme/example".to_owned(),
            version: "1.2.3".to_owned(),
            release_digest: digest('e'),
            manifest_digest: digest('f'),
            delivery: ModuleDelivery::Linked(LinkedModuleDelivery {
                package: "lenso-example-module".to_owned(),
                crate_version: "1.2.3".to_owned(),
                archive_checksum: digest('7'),
                default_features: false,
                features: vec!["runtime".to_owned()],
                binding: "module::linked_module".to_owned(),
                attestations: Vec::new(),
                migrations: Vec::new(),
            }),
            reason: LockedModuleReason::Direct,
            dependency_module_ids: Vec::new(),
            crate_features: vec!["runtime".to_owned()],
            migration_artifacts: Vec::new(),
            console_ui_artifact: None,
            verification: VerificationEvaluation {
                state: VerificationState::Verified,
                reason_code: "exact_cell_passed".to_owned(),
                receipt_digests: vec![digest('8')],
            },
            verification_cell: ModuleVerificationCell {
                module_release_digest: digest('e'),
                operation: VerificationOperation::FreshInstall,
                source_release_digest: None,
                lenso_version: "0.3.33".to_owned(),
                host_version: "0.3.33".to_owned(),
                cli_version: "0.2.13".to_owned(),
                starter_digest: digest('9'),
                management_engine_version: "0.1.0".to_owned(),
                delivery_digest: digest('7'),
                features: vec!["runtime".to_owned()],
                target: "aarch64-apple-darwin".to_owned(),
                os: "macos".to_owned(),
                architecture: "aarch64".to_owned(),
                runner_image_digest: digest('0'),
                rust_version: "1.94.0".to_owned(),
                cargo_version: "1.94.0".to_owned(),
                store_engine: "postgres".to_owned(),
                store_version: "18".to_owned(),
                protocol_digests: Vec::new(),
                console_artifact_digest: None,
                console_host_api_version: None,
                node_version: None,
                package_manager_version: None,
                console_lock_digest: None,
            },
            lifecycle: ModuleLifecycleState::default(),
            local_override_digest: Some(digest('d')),
        }],
        capability_bindings: Vec::new(),
    };
    (desired, module_lock)
}

fn plan(root: &Path) -> ModuleChangePlan {
    let desired = desired();
    let module_lock = module_lock(&desired);
    let reviewed = format!("{}\n", serde_json::to_string_pretty(&desired).unwrap());
    let workspace = LinkedWorkspacePlanner::new(root)
        .plan(&desired, &module_lock, &reviewed, None)
        .unwrap();
    let desired_digest = desired_composition_digest(&desired).unwrap();
    let lock_digest = application_module_lock_digest(&module_lock).unwrap();
    let mut plan = ModuleChangePlan {
        protocol: MODULE_CHANGE_PLAN_PROTOCOL.to_owned(),
        plan_id: "linked-plan-1".to_owned(),
        plan_digest: String::new(),
        application_id: desired.application_id.clone(),
        environment_id: "local".to_owned(),
        expected_target_revision: 0,
        request: ModuleRootChange::Install {
            selection: DesiredModuleSelection {
                module_id: "acme/example".to_owned(),
                version_requirement: "^1.0".to_owned(),
                optional_requirements: Vec::new(),
                exact_release_digest: None,
                delivery_preference: Some(ManagedDeliveryKind::Linked),
            },
        },
        current_desired_digest: digest('1'),
        target_desired: desired,
        target_desired_digest: desired_digest,
        current_lock_digest: None,
        target_lock: module_lock,
        target_lock_digest: lock_digest,
        catalog_snapshot_digest: digest('a'),
        resolver_version: "resolver-1".to_owned(),
        trust_policy_digest: digest('b'),
        compatibility_evidence_digest: digest('c'),
        cargo_lock_candidate: None,
        read_set: workspace.read_set,
        effects: workspace.effects,
        approval_boundaries: Vec::new(),
        validation_commands: vec!["cargo metadata --locked".to_owned()],
        next_actions: vec!["review_plan".to_owned()],
        created_at: now(),
    };
    plan.plan_digest = module_change_plan_digest(&plan).unwrap();
    plan
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

fn start<'a>(
    plan: &'a ModuleChangePlan,
    policy: &'a ModuleEnvironmentPolicy,
    actor: &'a ManagementActor,
) -> StartModuleOperation<'a> {
    StartModuleOperation {
        operation_id: "linked-operation-1",
        idempotency_key: "linked-request-1",
        operation_kind: ModuleOperationKind::Install,
        plan,
        policy,
        actor,
        approvals: Vec::new(),
        holder_id: "worker-a",
        now: now(),
    }
}

#[test]
fn deterministic_plan_applies_only_managed_files_and_preserves_unrelated_dirt() {
    let root = temp_root("apply");
    scaffold(&root);
    fs::write(root.join("unrelated.txt"), "keep me\n").unwrap();
    let first = plan(&root);
    let second = plan(&root);
    assert_eq!(first.read_set, second.read_set);
    assert_eq!(first.effects, second.effects);
    assert_eq!(first.plan_digest, second.plan_digest);

    let policy = policy();
    let actor = actor();
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let operation = engine.start(start(&first, &policy, &actor)).unwrap();
    let applied = LinkedWorkspaceTransaction::new(&root)
        .apply(
            &engine,
            &operation.operation_id,
            &first,
            operation.fencing_token,
            &actor.actor_id,
            now(),
        )
        .unwrap();

    assert_eq!(applied.state, ModuleOperationState::FilesApplied);
    assert_eq!(applied.effect_receipts.len(), first.effects.len());
    assert!(!applied.workspace_backups.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "keep me\n"
    );
    assert!(
        fs::read_to_string(root.join("generated/lenso-linked/src/lib.rs"))
            .unwrap()
            .contains(&first.target_lock_digest)
    );
    assert!(
        engine
            .store()
            .journal(&operation.operation_id)
            .unwrap()
            .events[1]
            .outcome_code
            .contains("workspace_backups_recorded")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn generated_crate_uses_exact_alias_features_override_and_binding_export() {
    let root = temp_root("generated-crate");
    scaffold(&root);
    let (desired, module_lock) = linked_desired_and_lock();
    let reviewed = format!("{}\n", serde_json::to_string_pretty(&desired).unwrap());
    let workspace = LinkedWorkspacePlanner::new(&root)
        .plan(&desired, &module_lock, &reviewed, None)
        .unwrap();
    let contents = workspace
        .effects
        .iter()
        .filter_map(|effect| match effect {
            ModulePlanEffect::WorkspaceFile {
                path,
                after_content: Some(content),
                ..
            } => Some((path.as_str(), content.as_str())),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let cargo = contents["generated/lenso-linked/Cargo.toml"];
    assert!(cargo.contains("lenso_module_acme_example = { package = \"lenso-example-module\""));
    assert!(cargo.contains("path = \"../example-module\""));
    assert!(cargo.contains("features = [\"runtime\"]"));
    let source = contents["generated/lenso-linked/src/lib.rs"];
    assert!(source.contains("lenso_module_acme_example::module::linked_module()"));
    assert!(source.contains(&application_module_lock_digest(&module_lock).unwrap()));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_generated_path_is_rejected_during_side_effect_free_planning() {
    use std::os::unix::fs::symlink;

    let root = temp_root("symlink");
    scaffold(&root);
    let outside = temp_root("symlink-outside");
    fs::create_dir_all(root.join("generated")).unwrap();
    symlink(&outside, root.join("generated/lenso-linked")).unwrap();
    let desired = desired();
    let module_lock = module_lock(&desired);
    let reviewed = format!("{}\n", serde_json::to_string_pretty(&desired).unwrap());
    assert!(matches!(
        LinkedWorkspacePlanner::new(&root).plan(&desired, &module_lock, &reviewed, None),
        Err(LinkedWorkspaceError::UnsafePath(path)) if path.contains("generated/lenso-linked")
    ));
    assert!(fs::read_dir(&outside).unwrap().next().is_none());
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn one_byte_change_invalidates_plan_before_any_workspace_backup_or_write() {
    let root = temp_root("stale");
    scaffold(&root);
    let plan = plan(&root);
    fs::write(
        root.join("src/main.rs"),
        "fn host() { let _ = HostBuilder::new().linked_modules(lenso_linked_composition::linked_modules()); } // changed\n",
    )
    .unwrap();
    let policy = policy();
    let actor = actor();
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    assert!(matches!(
        LinkedWorkspaceTransaction::new(&root).apply(
            &engine,
            &operation.operation_id,
            &plan,
            operation.fencing_token,
            &actor.actor_id,
            now(),
        ),
        Err(LinkedWorkspaceError::Stale(path)) if path == "src/main.rs"
    ));
    let unchanged = engine.store().load(&operation.operation_id).unwrap();
    assert_eq!(unchanged.state, ModuleOperationState::Ready);
    assert!(unchanged.workspace_backups.is_empty());
    assert!(!root.join("lenso.modules.lock.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_later_write_restores_every_prior_file_exactly() {
    let root = temp_root("restore");
    scaffold(&root);
    fs::create_dir_all(root.join("generated/lenso-linked/src")).unwrap();
    make_read_only(&root.join("generated/lenso-linked/src"));
    let plan = plan(&root);
    let policy = policy();
    let actor = actor();
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    let result = LinkedWorkspaceTransaction::new(&root).apply(
        &engine,
        &operation.operation_id,
        &plan,
        operation.fencing_token,
        &actor.actor_id,
        now(),
    );
    make_writable(&root.join("generated/lenso-linked/src"));

    assert!(matches!(result, Err(LinkedWorkspaceError::Io(_))));
    assert!(!root.join("generated/lenso-linked/Cargo.toml").exists());
    assert!(!root.join("generated/lenso-linked/src/lib.rs").exists());
    assert_eq!(
        engine.store().load(&operation.operation_id).unwrap().state,
        ModuleOperationState::Restored
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn crash_resume_recognizes_reviewed_bytes_without_repeating_the_effect() {
    let root = temp_root("resume");
    scaffold(&root);
    let plan = plan(&root);
    let policy = policy();
    let actor = actor();
    let engine = ModuleManagementEngine::new(MemoryModuleOperationStore::default());
    let operation = engine.start(start(&plan, &policy, &actor)).unwrap();
    let backups = absent_backups_for_plan(&plan);
    let applying = engine
        .begin_workspace_application(
            &operation.operation_id,
            operation.revision,
            operation.fencing_token,
            &actor.actor_id,
            &plan,
            backups,
            Vec::new(),
            now(),
        )
        .unwrap();
    let first = plan
        .effects
        .iter()
        .find_map(|effect| match effect {
            ModulePlanEffect::WorkspaceFile {
                effect_id,
                path,
                after_content: Some(content),
                ..
            } => Some((effect_id, path, content)),
            _ => None,
        })
        .unwrap();
    let target = root.join(first.1);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, first.2).unwrap();
    let resume_at = now() + chrono::Duration::seconds(61);
    let evidence = LinkedWorkspaceTransaction::new(&root)
        .resume_evidence(&applying, &plan, resume_at)
        .unwrap();
    assert_eq!(evidence.next_effect_id, *first.0);
    let resumed = engine
        .resume_after_crash(
            &operation.operation_id,
            applying.revision,
            &evidence,
            "worker-b",
            60,
            "service:management-worker",
            resume_at,
        )
        .unwrap();
    let completed = LinkedWorkspaceTransaction::new(&root)
        .apply(
            &engine,
            &operation.operation_id,
            &plan,
            resumed.fencing_token,
            "service:management-worker",
            resume_at,
        )
        .unwrap();
    assert_eq!(completed.state, ModuleOperationState::FilesApplied);
    assert_eq!(completed.attempt, 2);
    assert!(completed.effect_receipts.iter().any(|receipt| {
        receipt.effect_id == *first.0 && receipt.outcome == ModuleEffectOutcome::AlreadyApplied
    }));
    fs::remove_dir_all(root).unwrap();
}

fn absent_backups_for_plan(plan: &ModuleChangePlan) -> Vec<ModuleWorkspaceBackup> {
    let mut paths = HashSet::new();
    for path in plan.effects.iter().filter_map(|effect| match effect {
        ModulePlanEffect::WorkspaceFile { path, .. } => Some(path.as_str()),
        _ => None,
    }) {
        let mut current = Path::new(path);
        loop {
            paths.insert(current.to_string_lossy().into_owned());
            let Some(parent) = current.parent() else {
                break;
            };
            if parent.as_os_str().is_empty() {
                break;
            }
            current = parent;
        }
    }
    let existing = BTreeSet::from(["Cargo.toml", "src", "src/main.rs", ".lenso"]);
    let mut backups = paths
        .into_iter()
        .map(|path| {
            assert!(
                !existing.contains(path.as_str()),
                "test backup helper unexpectedly includes scaffold path {path}"
            );
            ModuleWorkspaceBackup {
                path,
                existence: PathExistence::Absent,
                file_type: ManagedFileType::Absent,
                content_base64: None,
                content_digest: None,
                mode: None,
            }
        })
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| left.path.cmp(&right.path));
    backups
}

#[cfg(unix)]
fn make_read_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o555)).unwrap();
}

#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_read_only(_: &Path) {}

#[cfg(not(unix))]
fn make_writable(_: &Path) {}
