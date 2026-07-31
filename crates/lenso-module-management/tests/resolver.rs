use lenso_contracts::{
    CatalogAction, DeclaredCompatibilityState, LinkedModuleDelivery, ModuleDelivery,
    ModuleEligibility, ModuleEligibilityState, ModuleLifecycleState, ModuleManifest, ModuleRelease,
    ModuleRequirement, ModuleVerificationCell, ServiceModuleDelivery, ServiceResponsibilityProfile,
    VerificationEvaluation, VerificationOperation, VerificationState, digest_json,
};
use lenso_module_management::*;

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
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

fn requirement(module_id: &str, version_requirement: &str, optional: bool) -> ModuleRequirement {
    ModuleRequirement {
        module_id: module_id.to_owned(),
        version_requirement: version_requirement.to_owned(),
        capabilities: Vec::new(),
        optional,
    }
}

fn candidate(
    module_id: &str,
    version: &str,
    requires: Vec<ModuleRequirement>,
    capabilities: Vec<&str>,
) -> ModuleResolutionCandidate {
    let manifest = ModuleManifest::builder(module_id)
        .capabilities(capabilities.into_iter().map(str::to_owned).collect())
        .requires(requires)
        .build();
    let release = ModuleRelease::new(
        module_id,
        version,
        manifest,
        ModuleDelivery::Linked(LinkedModuleDelivery {
            package: module_id.replace('/', "-"),
            crate_version: version.to_owned(),
            archive_checksum: digest('a'),
            default_features: false,
            features: vec!["runtime".to_owned()],
            binding: "module::linked_module".to_owned(),
            attestations: Vec::new(),
            migrations: Vec::new(),
        }),
    )
    .unwrap();
    let release_digest = digest_json(&release).unwrap();
    ModuleResolutionCandidate {
        catalog_snapshot_digest: digest('c'),
        verification_cell: verification_cell(&release_digest),
        release_digest,
        release,
        eligibility: ModuleEligibility {
            action: CatalogAction::Install,
            state: ModuleEligibilityState::Eligible,
            declared_compatibility: DeclaredCompatibilityState::Compatible,
            verification: VerificationEvaluation {
                state: VerificationState::Verified,
                reason_code: "exact_cell_passed".to_owned(),
                receipt_digests: vec![digest('e')],
            },
            lifecycle: ModuleLifecycleState::default(),
            snapshot_age_seconds: 0,
            snapshot_fresh: true,
            reason_codes: Vec::new(),
        },
    }
}

fn verification_cell(release_digest: &str) -> ModuleVerificationCell {
    ModuleVerificationCell {
        module_release_digest: release_digest.to_owned(),
        operation: VerificationOperation::FreshInstall,
        source_release_digest: None,
        lenso_version: "0.3.33".to_owned(),
        host_version: "0.3.33".to_owned(),
        cli_version: "0.2.13".to_owned(),
        starter_digest: digest('1'),
        management_engine_version: "0.1.0".to_owned(),
        delivery_digest: digest('2'),
        features: vec!["runtime".to_owned()],
        target: "aarch64-apple-darwin".to_owned(),
        os: "macos".to_owned(),
        architecture: "aarch64".to_owned(),
        runner_image_digest: digest('3'),
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
    }
}

fn service_candidate(module_id: &str, version: &str) -> ModuleResolutionCandidate {
    let manifest = ModuleManifest::builder(module_id).build();
    let release = ModuleRelease::new(
        module_id,
        version,
        manifest,
        ModuleDelivery::Service(ServiceModuleDelivery {
            service_id: "acme/service".to_owned(),
            service_release_version: version.to_owned(),
            service_release_digest: digest('9'),
            export: module_id.replace('/', "_"),
            responsibility_profile: ServiceResponsibilityProfile::Provider,
            contract_digests: vec![digest('8')],
        }),
    )
    .unwrap();
    let release_digest = digest_json(&release).unwrap();
    let mut resolved = candidate(module_id, version, Vec::new(), Vec::new());
    resolved.verification_cell = verification_cell(&release_digest);
    resolved.release_digest = release_digest;
    resolved.release = release;
    resolved
}

fn install(module_id: &str, optional_requirements: Vec<&str>) -> ModuleRootChange {
    ModuleRootChange::Install {
        selection: DesiredModuleSelection {
            module_id: module_id.to_owned(),
            version_requirement: "*".to_owned(),
            optional_requirements: optional_requirements
                .into_iter()
                .map(str::to_owned)
                .collect(),
            exact_release_digest: None,
            delivery_preference: None,
        },
    }
}

fn request(
    current_desired: DesiredModuleComposition,
    current_lock: Option<ApplicationModuleLock>,
    change: ModuleRootChange,
    candidates: Vec<ModuleResolutionCandidate>,
) -> ModuleResolutionRequest {
    ModuleResolutionRequest {
        current_desired,
        current_lock,
        change,
        catalog_snapshot_digest: digest('c'),
        trust_policy_digest: digest('d'),
        resolver_version: "resolver-1".to_owned(),
        candidates,
    }
}

#[test]
fn install_resolves_complete_graph_and_excludes_unselected_optional_requirement() {
    let root = candidate(
        "acme/root",
        "1.0.0",
        vec![
            requirement("acme/core", "^2", false),
            requirement("acme/optional", "^1", true),
        ],
        Vec::new(),
    );
    let core_old = candidate("acme/core", "2.1.0", Vec::new(), vec!["storage"]);
    let core_new = candidate("acme/core", "2.3.0", Vec::new(), vec!["storage"]);
    let optional = candidate("acme/optional", "1.0.0", Vec::new(), Vec::new());

    let resolution = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", Vec::new()),
            vec![root, core_old, core_new, optional],
        ))
        .unwrap();

    assert_eq!(resolution.target_desired.revision, 2);
    assert_eq!(
        resolution
            .target_lock
            .modules
            .iter()
            .map(|module| (module.module_id.as_str(), module.version.as_str()))
            .collect::<Vec<_>>(),
        vec![("acme/core", "2.3.0"), ("acme/root", "1.0.0")]
    );
    assert_eq!(
        resolution.target_lock.modules[0].reason,
        LockedModuleReason::Transitive
    );
}

#[test]
fn resolver_retains_current_eligible_release_before_considering_newer_version() {
    let old = candidate("acme/root", "1.0.0", Vec::new(), Vec::new());
    let new = candidate("acme/root", "1.1.0", Vec::new(), Vec::new());
    let first = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", Vec::new()),
            vec![old.clone()],
        ))
        .unwrap();

    let second = ModuleGraphResolver
        .resolve(&request(
            first.target_desired.clone(),
            Some(first.target_lock),
            ModuleRootChange::Update {
                module_id: "acme/root".to_owned(),
                version_requirement: "*".to_owned(),
            },
            vec![old, new],
        ))
        .unwrap();

    assert_eq!(second.target_lock.modules[0].version, "1.0.0");
}

#[test]
fn optional_selection_adds_transitive_module_and_capability_binding() {
    let mut optional_requirement = requirement("acme/search", "^1", true);
    optional_requirement.capabilities = vec!["query".to_owned()];
    let root = candidate("acme/root", "1.0.0", vec![optional_requirement], Vec::new());
    let search = candidate("acme/search", "1.0.0", Vec::new(), vec!["query"]);

    let resolution = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", vec!["acme/search"]),
            vec![root, search],
        ))
        .unwrap();

    assert_eq!(resolution.target_lock.modules.len(), 2);
    assert_eq!(
        resolution.target_lock.capability_bindings,
        vec![LockedCapabilityBinding {
            capability: "query".to_owned(),
            provider_module_id: "acme/search".to_owned(),
            consumer_module_id: "acme/root".to_owned(),
        }]
    );
}

#[test]
fn uninstall_re_resolves_and_reports_orphan_removal() {
    let root = candidate(
        "acme/root",
        "1.0.0",
        vec![requirement("acme/core", "*", false)],
        Vec::new(),
    );
    let core = candidate("acme/core", "1.0.0", Vec::new(), Vec::new());
    let installed = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", Vec::new()),
            vec![root.clone(), core.clone()],
        ))
        .unwrap();

    let removed = ModuleGraphResolver
        .resolve(&request(
            installed.target_desired,
            Some(installed.target_lock),
            ModuleRootChange::Uninstall {
                module_id: "acme/root".to_owned(),
            },
            vec![root, core],
        ))
        .unwrap();

    assert!(removed.target_lock.modules.is_empty());
    assert_eq!(
        removed.removed_orphan_module_ids,
        vec!["acme/core".to_owned(), "acme/root".to_owned()]
    );
}

#[test]
fn conflict_explains_dependency_path_constraints_and_alternatives() {
    let root = candidate(
        "acme/root",
        "1.0.0",
        vec![requirement("acme/core", "^2", false)],
        Vec::new(),
    );
    let core = candidate("acme/core", "1.5.0", Vec::new(), Vec::new());

    let error = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", Vec::new()),
            vec![root, core],
        ))
        .unwrap_err();

    assert_eq!(error.conflict.code, "unsatisfiable_module");
    assert_eq!(error.conflict.module_id.as_deref(), Some("acme/core"));
    assert_eq!(
        error.conflict.dependency_paths,
        vec![vec!["acme/root".to_owned(), "acme/core".to_owned()]]
    );
    assert_eq!(error.conflict.constraints, vec!["^2".to_owned()]);
    assert_eq!(error.conflict.eligible_alternatives.len(), 1);
}

#[test]
fn blocked_exact_pin_does_not_override_policy() {
    let mut blocked = candidate("acme/root", "1.0.0", Vec::new(), Vec::new());
    blocked.eligibility.state = ModuleEligibilityState::Blocked;
    let pinned_digest = blocked.release_digest.clone();
    let change = ModuleRootChange::Install {
        selection: DesiredModuleSelection {
            module_id: "acme/root".to_owned(),
            version_requirement: "*".to_owned(),
            optional_requirements: Vec::new(),
            exact_release_digest: Some(pinned_digest),
            delivery_preference: None,
        },
    };

    let error = ModuleGraphResolver
        .resolve(&request(desired(), None, change, vec![blocked]))
        .unwrap_err();
    assert_eq!(error.conflict.code, "unsatisfiable_module");
}

#[test]
fn new_unconstrained_selection_prefers_linked_delivery() {
    let linked = candidate("acme/root", "1.0.0", Vec::new(), Vec::new());
    let service = service_candidate("acme/root", "1.0.0");

    let resolution = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", Vec::new()),
            vec![service, linked],
        ))
        .unwrap();
    assert!(matches!(
        resolution.target_lock.modules[0].delivery,
        ModuleDelivery::Linked(_)
    ));
}

#[test]
fn optional_requirement_must_be_declared_by_the_selecting_root() {
    let root = candidate("acme/root", "1.0.0", Vec::new(), Vec::new());
    let error = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/root", vec!["acme/not-declared"]),
            vec![root],
        ))
        .unwrap_err();
    assert_eq!(error.conflict.code, "optional_requirement_not_declared");
}

#[test]
fn dependency_cycle_fails_before_a_lock_is_produced() {
    let first = candidate(
        "acme/first",
        "1.0.0",
        vec![requirement("acme/second", "*", false)],
        Vec::new(),
    );
    let second = candidate(
        "acme/second",
        "1.0.0",
        vec![requirement("acme/first", "*", false)],
        Vec::new(),
    );
    let error = ModuleGraphResolver
        .resolve(&request(
            desired(),
            None,
            install("acme/first", Vec::new()),
            vec![first, second],
        ))
        .unwrap_err();
    assert_eq!(error.conflict.code, "dependency_cycle");
}
