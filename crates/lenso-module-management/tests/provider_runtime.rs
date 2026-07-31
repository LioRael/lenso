use lenso_contracts::{
    CatalogAction, DeclaredCompatibilityState, ModuleDelivery, ModuleEligibility,
    ModuleEligibilityState, ModuleLifecycleState, ModuleManifest, ModuleRelease,
    ModuleVerificationCell, ServiceModuleDelivery, ServiceResponsibilityProfile,
    VerificationEvaluation, VerificationOperation, VerificationState, digest_json,
};
use lenso_module_management::*;
use std::fs;

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
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
        features: Vec::new(),
        target: "aarch64-apple-darwin".to_owned(),
        os: "macos".to_owned(),
        architecture: "aarch64".to_owned(),
        runner_image_digest: digest('3'),
        rust_version: "1.94.0".to_owned(),
        cargo_version: "1.94.0".to_owned(),
        store_engine: "postgres".to_owned(),
        store_version: "18".to_owned(),
        protocol_digests: vec![digest('8')],
        console_artifact_digest: None,
        console_host_api_version: None,
        node_version: None,
        package_manager_version: None,
        console_lock_digest: None,
    }
}

fn candidate(
    module_id: &str,
    export_key: &str,
    profile: ServiceResponsibilityProfile,
) -> ModuleResolutionCandidate {
    let manifest = ModuleManifest::builder(module_id)
        .capabilities(vec![format!("{}.read", module_id.replace('/', "."))])
        .build();
    let release = ModuleRelease::new(
        module_id,
        "1.0.0",
        manifest,
        ModuleDelivery::Service(ServiceModuleDelivery {
            service_id: "acme/support-service".to_owned(),
            service_release_version: "4.0.0".to_owned(),
            service_release_digest: digest('9'),
            export: export_key.to_owned(),
            responsibility_profile: profile,
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
                receipt_digests: vec![digest('e')],
            },
            lifecycle: ModuleLifecycleState::default(),
            snapshot_age_seconds: 0,
            snapshot_fresh: true,
            reason_codes: Vec::new(),
        },
    }
}

fn resolve(candidate: &ModuleResolutionCandidate) -> ApplicationModuleLock {
    ModuleGraphResolver
        .resolve(&ModuleResolutionRequest {
            current_desired: DesiredModuleComposition {
                protocol: DESIRED_MODULE_COMPOSITION_PROTOCOL.to_owned(),
                application_id: "acme/app".to_owned(),
                revision: 0,
                selected: Vec::new(),
                local_overrides: Vec::new(),
            },
            current_lock: None,
            change: ModuleRootChange::Install {
                selection: DesiredModuleSelection {
                    module_id: candidate.release.module_id.clone(),
                    version_requirement: "=1.0.0".to_owned(),
                    optional_requirements: Vec::new(),
                    exact_release_digest: Some(candidate.release_digest.clone()),
                    delivery_preference: Some(match &candidate.release.delivery {
                        ModuleDelivery::Service(delivery)
                            if delivery.responsibility_profile
                                == ServiceResponsibilityProfile::Provider =>
                        {
                            ManagedDeliveryKind::Provider
                        }
                        ModuleDelivery::Service(_) => ManagedDeliveryKind::Autonomous,
                        ModuleDelivery::Linked(_) => unreachable!(),
                    }),
                },
            },
            catalog_snapshot_digest: digest('c'),
            trust_policy_digest: digest('d'),
            resolver_version: "resolver-1".to_owned(),
            candidates: vec![candidate.clone()],
        })
        .unwrap()
        .target_lock
}

fn context(candidates: Vec<ModuleResolutionCandidate>) -> ModulePlanningContext {
    ModulePlanningContext {
        protocol: MODULE_PLANNING_CONTEXT_PROTOCOL.to_owned(),
        system_id: "acme/system".to_owned(),
        application_id: "acme/app".to_owned(),
        environment_id: "local".to_owned(),
        expected_target_revision: 0,
        catalog_snapshot_digest: digest('c'),
        trust_policy_digest: digest('d'),
        compatibility_evidence_digest: digest('e'),
        resolver_version: "resolver-1".to_owned(),
        candidates,
        service_deployments: Vec::new(),
        cargo_offline: true,
    }
}

fn installation(candidates: &[ModuleResolutionCandidate]) -> ServiceInstallationSet {
    let service_ref = ServiceReference {
        system_id: "acme/system".to_owned(),
        service_id: "acme/support-service".to_owned(),
    };
    ServiceInstallationSet {
        protocol: SERVICE_INSTALLATION_SET_PROTOCOL.to_owned(),
        system_id: "acme/system".to_owned(),
        environment_id: "local".to_owned(),
        revision: 7,
        previous_state_digest: Some(digest('7')),
        services: vec![ServiceInstallation {
            service_ref: service_ref.clone(),
            profile: ServiceResponsibilityProfile::Provider,
            desired_mode: ServiceDesiredMode::Active,
            service_release: InstalledServiceRelease {
                version: "4.0.0".to_owned(),
                digest: digest('9'),
                immutable_locator: "oci://registry.example/acme/support@sha256:9999".to_owned(),
            },
            exports: candidates
                .iter()
                .map(|candidate| {
                    let ModuleDelivery::Service(delivery) = &candidate.release.delivery else {
                        unreachable!()
                    };
                    InstalledServiceExport {
                        export_key: delivery.export.clone(),
                        module_id: candidate.release.module_id.clone(),
                        module_version: candidate.release.version.clone(),
                        module_release_digest: candidate.release_digest.clone(),
                        manifest_digest: candidate.release.manifest_digest.clone(),
                        contract_digests: delivery.contract_digests.clone(),
                    }
                })
                .collect(),
            config_bindings: Vec::new(),
            endpoint_binding: EndpointBinding {
                binding_id: "support-provider".to_owned(),
                service_ref,
                resolver_source: EndpointResolverSource::Static {
                    endpoints: vec![StaticEndpointDeclaration {
                        address: "https://support.internal".to_owned(),
                        binding: ServiceTransportBinding::ProviderHttpJson,
                        region: Some("local".to_owned()),
                        failure_domain: Some("local-1".to_owned()),
                        priority: 0,
                        weight: 1,
                    }],
                },
                allowed_bindings: vec![ServiceTransportBinding::ProviderHttpJson],
                identity_policy: ServiceIdentityPolicy {
                    principal: "spiffe://acme/support".to_owned(),
                    audience: "lenso-host".to_owned(),
                    trust_profile: "local-mtls".to_owned(),
                    credential_references: vec!["secret://support/identity".to_owned()],
                },
                selection_policy: EndpointSelectionPolicy::default(),
                cache_policy: EndpointCachePolicy {
                    maximum_age_seconds: 30,
                    stale_if_source_unavailable_seconds: Some(60),
                },
            },
            lifecycle_binding: ServiceLifecycleBinding::External {
                deployment_reference: "deployment://support/local".to_owned(),
                observation_adapter_id: "fixture".to_owned(),
                operation_adapter_id: None,
            },
        }],
    }
}

#[test]
fn compiles_exact_locked_provider_module_without_enabling_sibling_exports() {
    let selected = candidate(
        "acme/support",
        "support",
        ServiceResponsibilityProfile::Provider,
    );
    let sibling = candidate(
        "acme/notifications",
        "notifications",
        ServiceResponsibilityProfile::Provider,
    );
    let module_lock = resolve(&selected);
    let planning_context = context(vec![sibling.clone(), selected.clone()]);
    let installations = installation(&[selected.clone(), sibling]);

    let plan =
        compile_provider_runtime_plan(&module_lock, &planning_context, &installations).unwrap();

    assert_eq!(plan.protocol, PROVIDER_RUNTIME_PLAN_PROTOCOL);
    assert_eq!(plan.service_installation_revision, 7);
    assert_eq!(plan.providers.len(), 1);
    assert_eq!(plan.providers[0].modules.len(), 1);
    assert_eq!(plan.providers[0].modules[0].module_id, "acme/support");
    assert_eq!(plan.providers[0].modules[0].export_key, "support");
    assert_eq!(
        plan.providers[0].modules[0].manifest,
        selected.release.manifest
    );
    let json = serde_json::to_string(&plan).unwrap();
    assert!(!json.contains("notifications"));
    assert!(!json.contains("secret value"));
}

#[test]
fn rejects_missing_installation_with_stable_actionable_evidence() {
    let selected = candidate(
        "acme/support",
        "support",
        ServiceResponsibilityProfile::Provider,
    );
    let error = compile_provider_runtime_plan(
        &resolve(&selected),
        &context(vec![selected]),
        &ServiceInstallationSet::empty("acme/system", "local"),
    )
    .unwrap_err();

    assert_eq!(error.issues.len(), 1);
    assert_eq!(
        error.issues[0].code,
        ProviderRuntimePlanIssueCode::InstallationMissing
    );
    assert_eq!(
        error.issues[0].next_action,
        "apply the exact Service Installation plan before activating this Module"
    );
}

#[test]
fn rejects_export_digest_and_static_transport_mismatches() {
    let selected = candidate(
        "acme/support",
        "support",
        ServiceResponsibilityProfile::Provider,
    );
    let module_lock = resolve(&selected);
    let planning_context = context(vec![selected.clone()]);
    let mut installations = installation(&[selected]);
    installations.services[0].exports[0].manifest_digest = digest('0');
    installations.services[0].endpoint_binding.allowed_bindings =
        vec![ServiceTransportBinding::DirectHttp];

    let error =
        compile_provider_runtime_plan(&module_lock, &planning_context, &installations).unwrap_err();
    let codes = error
        .issues
        .iter()
        .map(|issue| issue.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&ProviderRuntimePlanIssueCode::ExportMismatch));
    assert!(codes.contains(&ProviderRuntimePlanIssueCode::ProviderTransportUnavailable));
}

#[test]
fn autonomous_service_modules_never_enter_the_provider_runtime_plan() {
    let selected = candidate(
        "acme/search",
        "search",
        ServiceResponsibilityProfile::Autonomous,
    );
    let plan = compile_provider_runtime_plan(
        &resolve(&selected),
        &context(vec![selected]),
        &ServiceInstallationSet::empty("acme/system", "local"),
    )
    .unwrap();

    assert!(plan.providers.is_empty());
}

#[test]
fn rejects_inputs_from_different_environment_identities() {
    let selected = candidate(
        "acme/support",
        "support",
        ServiceResponsibilityProfile::Provider,
    );
    let mut installations = installation(std::slice::from_ref(&selected));
    installations.environment_id = "production".to_owned();

    let error = compile_provider_runtime_plan(
        &resolve(&selected),
        &context(vec![selected]),
        &installations,
    )
    .unwrap_err();
    assert!(
        error
            .issues
            .iter()
            .any(|issue| issue.code == ProviderRuntimePlanIssueCode::IdentityMismatch)
    );
}

#[test]
fn workspace_entry_loads_only_locked_management_artifacts() {
    let selected = candidate(
        "acme/support",
        "support",
        ServiceResponsibilityProfile::Provider,
    );
    let module_lock = resolve(&selected);
    let planning_context = context(vec![selected.clone()]);
    let installations = installation(&[selected]);
    let root = std::env::temp_dir().join(format!(
        "lenso-provider-runtime-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let environment_root = root.join(".lenso/environments/local");
    fs::create_dir_all(&environment_root).unwrap();
    fs::write(
        root.join("lenso.modules.lock.json"),
        serde_json::to_vec_pretty(&module_lock).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".lenso/module-planning-context.json"),
        serde_json::to_vec_pretty(&planning_context).unwrap(),
    )
    .unwrap();
    fs::write(
        environment_root.join("service-installations.json"),
        serde_json::to_vec_pretty(&installations).unwrap(),
    )
    .unwrap();

    let plan = WorkspaceModuleManagement::new(&root)
        .provider_runtime_plan()
        .unwrap();

    assert_eq!(plan.providers.len(), 1);
    assert_eq!(plan.providers[0].modules[0].module_id, "acme/support");
    fs::remove_dir_all(root).unwrap();
}
