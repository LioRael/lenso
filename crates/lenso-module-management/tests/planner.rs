use chrono::{TimeZone as _, Utc};
use lenso_contracts::{
    ArtifactReference, CatalogAction, DeclaredCompatibilityState, LinkedModuleDelivery,
    ModuleConsoleArtifact, ModuleDelivery, ModuleEligibility, ModuleEligibilityState,
    ModuleLifecycleState, ModuleManifest, ModuleMigrationActivation, ModuleMigrationDeclaration,
    ModuleRelease, ModuleRequirement, ModuleVerificationCell, ServiceModuleDelivery,
    ServiceResponsibilityProfile, VerificationEvaluation, VerificationOperation, VerificationState,
    digest_json,
};
use lenso_module_management::*;
use std::fs;
use std::path::{Path, PathBuf};

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "lenso-plan-{name}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(root.join(".lenso")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    root
}

fn scaffold(root: &Path, current_lock: &str) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"host\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nlenso-linked-composition = { path = \"generated/lenso-linked\" }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/main.rs"),
        "fn host() { let _ = HostBuilder::new().linked_modules(lenso_linked_composition::linked_modules()); }\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), current_lock).unwrap();
    fs::write(
        root.join(".lenso/linked-composition-seam.json"),
        serde_json::to_vec_pretty(&LinkedCompositionSeam {
            protocol: LINKED_COMPOSITION_SEAM_PROTOCOL.to_owned(),
            host_manifest_path: "Cargo.toml".to_owned(),
            host_source_path: "src/main.rs".to_owned(),
            generated_crate_path: "generated/lenso-linked".to_owned(),
            dependency_name: "lenso-linked-composition".to_owned(),
            lenso_version: "0.3.33".to_owned(),
        })
        .unwrap(),
    )
    .unwrap();
}

fn cargo_lock(include_module: bool) -> String {
    let dependency = if include_module {
        " \"acme-module\",\n"
    } else {
        ""
    };
    let module = if include_module {
        format!(
            "\n[[package]]\nname = \"acme-module\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{}\"\n",
            "a".repeat(64)
        )
    } else {
        String::new()
    };
    format!(
        "version = 4\n\n[[package]]\nname = \"host\"\nversion = \"0.1.0\"\ndependencies = [\n \"lenso-linked-composition\",\n]\n\n[[package]]\nname = \"lenso-linked-composition\"\nversion = \"0.0.0\"\ndependencies = [\n{dependency}]\n{module}"
    )
}

#[derive(Debug, Clone)]
struct FixtureCargo {
    lock: String,
}

impl CargoLockGenerator for FixtureCargo {
    fn generate(
        &self,
        sandbox: &Path,
        _manifest_path: &Path,
        _offline: bool,
    ) -> Result<Vec<String>, CargoLockResolutionError> {
        fs::write(sandbox.join("Cargo.lock"), &self.lock)?;
        Ok(vec![
            "cargo".to_owned(),
            "generate-lockfile".to_owned(),
            "--offline".to_owned(),
        ])
    }
}

fn desired() -> DesiredModuleComposition {
    DesiredModuleComposition {
        protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
        application_id: "app-1".to_owned(),
        revision: 0,
        selected: Vec::new(),
        local_overrides: Vec::new(),
    }
}

fn empty_service_installations() -> ServiceInstallationSet {
    ServiceInstallationSet::empty("acme/system", "local")
}

fn service_installation() -> ServiceInstallation {
    let service_ref = ServiceReference {
        system_id: "acme/system".to_owned(),
        service_id: "acme/support-service".to_owned(),
    };
    ServiceInstallation {
        service_ref: service_ref.clone(),
        profile: ServiceResponsibilityProfile::Provider,
        desired_mode: ServiceDesiredMode::Active,
        service_release: InstalledServiceRelease {
            version: "4.0.0".to_owned(),
            digest: digest('9'),
            immutable_locator: "oci://registry.example/acme/support@sha256:9999".to_owned(),
        },
        exports: vec![
            InstalledServiceExport {
                export_key: "acme_notifications".to_owned(),
                module_id: "acme/notifications".to_owned(),
                module_version: "1.0.0".to_owned(),
                module_release_digest: digest('5'),
                manifest_digest: digest('6'),
                contract_digests: vec![digest('8')],
            },
            InstalledServiceExport {
                export_key: "acme_support".to_owned(),
                module_id: "acme/support".to_owned(),
                module_version: "1.0.0".to_owned(),
                module_release_digest: digest('7'),
                manifest_digest: digest('8'),
                contract_digests: vec![digest('8')],
            },
        ],
        config_bindings: Vec::new(),
        endpoint_binding: EndpointBinding {
            binding_id: "support".to_owned(),
            service_ref,
            resolver_source: EndpointResolverSource::Static {
                endpoints: vec![StaticEndpointDeclaration {
                    address: "https://support.internal".to_owned(),
                    binding: ServiceTransportBinding::ProviderHttpJson,
                    region: None,
                    failure_domain: None,
                    priority: 0,
                    weight: 1,
                }],
            },
            allowed_bindings: vec![ServiceTransportBinding::ProviderHttpJson],
            identity_policy: ServiceIdentityPolicy {
                principal: "spiffe://acme/support".to_owned(),
                audience: "lenso-host".to_owned(),
                trust_profile: "local".to_owned(),
                credential_references: Vec::new(),
            },
            selection_policy: EndpointSelectionPolicy::default(),
            cache_policy: EndpointCachePolicy {
                maximum_age_seconds: 30,
                stale_if_source_unavailable_seconds: None,
            },
        },
        lifecycle_binding: ServiceLifecycleBinding::External {
            deployment_reference: "deployment://support".to_owned(),
            observation_adapter_id: "fixture".to_owned(),
            operation_adapter_id: Some("fixture".to_owned()),
        },
    }
}

fn candidate() -> ModuleResolutionCandidate {
    let manifest = ModuleManifest::builder("acme/module")
        .migrations(vec![ModuleMigrationDeclaration {
            migration_id: "create-records".to_owned(),
            order: 1,
            store: "host".to_owned(),
            destructive: true,
            reversible: true,
            activation: ModuleMigrationActivation::BeforeActivation,
        }])
        .build();
    let release = ModuleRelease::new(
        "acme/module",
        "1.0.0",
        manifest,
        ModuleDelivery::Linked(LinkedModuleDelivery {
            package: "acme-module".to_owned(),
            crate_version: "1.0.0".to_owned(),
            archive_checksum: digest('a'),
            default_features: false,
            features: vec!["runtime".to_owned()],
            binding: "module::linked_module".to_owned(),
            attestations: Vec::new(),
            migrations: vec![ArtifactReference {
                locator: "migrations/0001_create_records.sql".to_owned(),
                digest: digest('b'),
            }],
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
                receipt_digests: vec![digest('d')],
            },
            lifecycle: ModuleLifecycleState::default(),
            snapshot_age_seconds: 0,
            snapshot_fresh: true,
            reason_codes: Vec::new(),
        },
    }
}

fn candidate_with_console() -> ModuleResolutionCandidate {
    let mut candidate = candidate();
    candidate.release.console_artifact = Some(ModuleConsoleArtifact {
        package: "@acme/module-console".to_owned(),
        version: "1.0.0".to_owned(),
        integrity: digest('4'),
        exports: vec!["acmeConsoleModule".to_owned()],
        host_api_requirement: "^1".to_owned(),
        provenance: vec![ArtifactReference {
            locator: "https://modules.example/acme-console.js".to_owned(),
            digest: digest('4'),
        }],
    });
    candidate.release_digest = digest_json(&candidate.release).unwrap();
    candidate.verification_cell = verification_cell(&candidate.release_digest);
    candidate
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

fn service_candidate(
    module_id: &str,
    requirements: Vec<ModuleRequirement>,
) -> ModuleResolutionCandidate {
    let release = ModuleRelease::new(
        module_id,
        "1.0.0",
        ModuleManifest::builder(module_id)
            .requires(requirements)
            .build(),
        ModuleDelivery::Service(ServiceModuleDelivery {
            service_id: "acme/support-service".to_owned(),
            service_release_version: "4.0.0".to_owned(),
            service_release_digest: digest('9'),
            export: module_id.replace('/', "_"),
            responsibility_profile: ServiceResponsibilityProfile::Provider,
            contract_digests: vec![digest('8')],
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
                receipt_digests: vec![digest('d')],
            },
            lifecycle: ModuleLifecycleState::default(),
            snapshot_age_seconds: 0,
            snapshot_fresh: true,
            reason_codes: Vec::new(),
        },
    }
}

#[test]
fn one_plan_call_resolves_graph_cargo_workspace_migration_and_activation() {
    let root = temp_root("complete");
    scaffold(&root, &cargo_lock(false));
    let planner = ModuleChangePlanner::with_cargo_generator(
        &root,
        FixtureCargo {
            lock: cargo_lock(true),
        },
    );
    let plan = planner
        .plan(&ModuleChangePlanRequest {
            current_desired: desired(),
            current_lock: None,
            change: ModuleRootChange::Install {
                selection: DesiredModuleSelection {
                    module_id: "acme/module".to_owned(),
                    version_requirement: "^1".to_owned(),
                    optional_requirements: Vec::new(),
                    exact_release_digest: None,
                    delivery_preference: None,
                },
            },
            catalog_snapshot_digest: digest('c'),
            trust_policy_digest: digest('e'),
            compatibility_evidence_digest: digest('f'),
            resolver_version: "resolver-1".to_owned(),
            environment_id: "local".to_owned(),
            expected_target_revision: 0,
            candidates: vec![candidate()],
            current_service_installations: empty_service_installations(),
            service_deployments: Vec::new(),
            cargo_offline: true,
            created_at: Utc.with_ymd_and_hms(2026, 7, 30, 8, 0, 0).unwrap(),
        })
        .unwrap();

    validate_change_plan(&plan).unwrap();
    assert!(plan.cargo_lock_candidate.is_some());
    assert!(plan.effects.iter().any(|effect| matches!(
        effect,
        ModulePlanEffect::Migration {
            risk_class: ModuleRiskClass::DestructiveMigration,
            ..
        }
    )));
    assert_eq!(plan.approval_boundaries.len(), 1);
    assert_eq!(plan.approval_boundaries[0].effect_ids.len(), 1);
    assert!(matches!(
        plan.effects.last(),
        Some(ModulePlanEffect::Restart { .. })
    ));
    assert!(
        plan.effects
            .windows(2)
            .all(|pair| pair[0].effect_id() < pair[1].effect_id())
    );
    assert!(!root.join("generated/lenso-linked/Cargo.toml").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn console_artifact_change_produces_console_composition_effect() {
    let root = temp_root("console-composition");
    scaffold(&root, &cargo_lock(false));
    let plan = ModuleChangePlanner::with_cargo_generator(
        &root,
        FixtureCargo {
            lock: cargo_lock(true),
        },
    )
    .plan(&ModuleChangePlanRequest {
        current_desired: desired(),
        current_lock: None,
        change: ModuleRootChange::Install {
            selection: DesiredModuleSelection {
                module_id: "acme/module".to_owned(),
                version_requirement: "^1".to_owned(),
                optional_requirements: Vec::new(),
                exact_release_digest: None,
                delivery_preference: None,
            },
        },
        catalog_snapshot_digest: digest('c'),
        trust_policy_digest: digest('e'),
        compatibility_evidence_digest: digest('f'),
        resolver_version: "resolver-1".to_owned(),
        environment_id: "local".to_owned(),
        expected_target_revision: 0,
        candidates: vec![candidate_with_console()],
        current_service_installations: empty_service_installations(),
        service_deployments: Vec::new(),
        cargo_offline: true,
        created_at: Utc.with_ymd_and_hms(2026, 7, 30, 8, 0, 0).unwrap(),
    })
    .unwrap();

    let artifacts = plan
        .effects
        .iter()
        .find_map(|effect| match effect {
            ModulePlanEffect::ConsoleComposition { artifacts, .. } => Some(artifacts),
            _ => None,
        })
        .expect("Console artifact change should produce a composition effect");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].module_id, "acme/module");
    assert_eq!(
        artifacts[0].artifact_locator,
        "https://modules.example/acme-console.js"
    );
    assert_eq!(artifacts[0].exports, ["acmeConsoleModule"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_override_keeps_feature_evidence_without_requiring_registry_checksum() {
    let root = temp_root("override");
    scaffold(&root, &cargo_lock(false));
    let mut current_desired = desired();
    current_desired.local_overrides.push(LocalModuleOverride {
        module_id: "acme/module".to_owned(),
        path: "../module".to_owned(),
        content_digest: digest('7'),
        acknowledged_unverified: true,
    });
    let mut candidate_lock = cargo_lock(true);
    candidate_lock = candidate_lock.replace(
        &format!("source = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{}\"\n", "a".repeat(64)),
        "",
    );
    let plan = ModuleChangePlanner::with_cargo_generator(
        &root,
        FixtureCargo {
            lock: candidate_lock,
        },
    )
    .plan(&ModuleChangePlanRequest {
        current_desired,
        current_lock: None,
        change: ModuleRootChange::Install {
            selection: DesiredModuleSelection {
                module_id: "acme/module".to_owned(),
                version_requirement: "*".to_owned(),
                optional_requirements: Vec::new(),
                exact_release_digest: None,
                delivery_preference: Some(ManagedDeliveryKind::Linked),
            },
        },
        catalog_snapshot_digest: digest('c'),
        trust_policy_digest: digest('e'),
        compatibility_evidence_digest: digest('f'),
        resolver_version: "resolver-1".to_owned(),
        environment_id: "local".to_owned(),
        expected_target_revision: 0,
        candidates: vec![candidate()],
        current_service_installations: empty_service_installations(),
        service_deployments: Vec::new(),
        cargo_offline: true,
        created_at: Utc.with_ymd_and_hms(2026, 7, 30, 8, 0, 0).unwrap(),
    })
    .unwrap();

    assert_eq!(
        plan.cargo_lock_candidate.unwrap().changed_packages[0].candidate_features,
        vec!["runtime"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn modules_from_one_service_release_form_one_install_and_restart_cohort() {
    let root = temp_root("service-cohort");
    scaffold(&root, &cargo_lock(false));
    let root_module = service_candidate(
        "acme/support",
        vec![ModuleRequirement {
            module_id: "acme/notifications".to_owned(),
            version_requirement: "^1".to_owned(),
            capabilities: Vec::new(),
            optional: false,
        }],
    );
    let dependency = service_candidate("acme/notifications", Vec::new());
    let planner = ModuleChangePlanner::with_cargo_generator(
        &root,
        FixtureCargo {
            lock: cargo_lock(false),
        },
    );
    let candidates = vec![root_module, dependency];
    let deployment = ServiceDeploymentBinding {
        service_id: "acme/support-service".to_owned(),
        service_release_digest: digest('9'),
        adapter: ServiceDeploymentAdapterKind::Local,
        installation: Some(service_installation()),
        install: Some(ServiceDeploymentAction::Command {
            program: "deployctl".to_owned(),
            args: vec!["install".to_owned()],
            working_directory: None,
        }),
        remove: Some(ServiceDeploymentAction::Command {
            program: "deployctl".to_owned(),
            args: vec!["remove".to_owned()],
            working_directory: None,
        }),
        restart: Some(ServiceDeploymentAction::Command {
            program: "deployctl".to_owned(),
            args: vec!["restart".to_owned()],
            working_directory: None,
        }),
    };
    let plan = planner
        .plan(&ModuleChangePlanRequest {
            current_desired: desired(),
            current_lock: None,
            change: ModuleRootChange::Install {
                selection: DesiredModuleSelection {
                    module_id: "acme/support".to_owned(),
                    version_requirement: "*".to_owned(),
                    optional_requirements: Vec::new(),
                    exact_release_digest: None,
                    delivery_preference: Some(ManagedDeliveryKind::Provider),
                },
            },
            catalog_snapshot_digest: digest('c'),
            trust_policy_digest: digest('e'),
            compatibility_evidence_digest: digest('f'),
            resolver_version: "resolver-1".to_owned(),
            environment_id: "local".to_owned(),
            expected_target_revision: 0,
            candidates: candidates.clone(),
            current_service_installations: empty_service_installations(),
            service_deployments: vec![deployment.clone()],
            cargo_offline: true,
            created_at: Utc.with_ymd_and_hms(2026, 7, 30, 8, 0, 0).unwrap(),
        })
        .unwrap();

    assert_eq!(
        plan.effects
            .iter()
            .filter(|effect| matches!(effect, ModulePlanEffect::ServiceInstallation { .. }))
            .count(),
        1
    );
    assert_eq!(
        plan.effects
            .iter()
            .filter(|effect| matches!(effect, ModulePlanEffect::ServiceRestart { service_id, .. } if service_id == "acme/support-service"))
            .count(),
        1
    );

    let installed_services = plan
        .effects
        .iter()
        .find_map(|effect| match effect {
            ModulePlanEffect::ServiceInstallation {
                installation_plan: Some(plan),
                ..
            } => Some(plan.target.clone()),
            _ => None,
        })
        .unwrap();
    let removal = planner
        .plan(&ModuleChangePlanRequest {
            current_desired: plan.target_desired.clone(),
            current_lock: Some(plan.target_lock.clone()),
            change: ModuleRootChange::Uninstall {
                module_id: "acme/support".to_owned(),
            },
            catalog_snapshot_digest: digest('c'),
            trust_policy_digest: digest('e'),
            compatibility_evidence_digest: digest('f'),
            resolver_version: "resolver-1".to_owned(),
            environment_id: "local".to_owned(),
            expected_target_revision: 1,
            candidates,
            current_service_installations: installed_services,
            service_deployments: vec![deployment],
            cargo_offline: true,
            created_at: Utc.with_ymd_and_hms(2026, 7, 30, 8, 1, 0).unwrap(),
        })
        .unwrap();
    assert!(
        !removal
            .effects
            .iter()
            .any(|effect| matches!(effect, ModulePlanEffect::ServiceRemoval { .. }))
    );
    assert!(
        !removal
            .effects
            .iter()
            .any(|effect| matches!(effect, ModulePlanEffect::ServiceInstallation { .. }))
    );
    fs::remove_dir_all(root).unwrap();
}
