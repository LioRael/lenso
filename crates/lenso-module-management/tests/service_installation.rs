use chrono::{TimeZone as _, Utc};
use lenso_contracts::ServiceResponsibilityProfile;
use lenso_module_management::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "lenso-service-install-{name}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn installed_service() -> ServiceInstallation {
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
        exports: vec![InstalledServiceExport {
            export_key: "support".to_owned(),
            module_id: "acme/support".to_owned(),
            module_version: "1.0.0".to_owned(),
            module_release_digest: digest('7'),
            manifest_digest: digest('8'),
            contract_digests: vec![digest('6')],
        }],
        config_bindings: vec![ServiceConfigBinding {
            owner_id: "support".to_owned(),
            config_contract_digest: digest('1'),
            config_revision_id: "config-1".to_owned(),
            config_revision_digest: digest('2'),
            activation: ConfigActivationIntent::Activate,
            secret_references: vec!["secret://support/database".to_owned()],
        }],
        endpoint_binding: EndpointBinding {
            binding_id: "support-primary".to_owned(),
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
            deployment_reference: "deployment://support/prod".to_owned(),
            observation_adapter_id: "kubernetes".to_owned(),
            operation_adapter_id: Some("kubernetes".to_owned()),
        },
    }
}

fn authority() -> BTreeSet<String> {
    BTreeSet::from(["service.manage".to_owned()])
}

#[test]
fn apply_persists_desired_state_and_idempotent_needs_attention_receipt() {
    let root = temp_root("apply");
    let manager = WorkspaceServiceInstallationManager::new(&root, "acme/system", "local");
    let now = Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap();
    let plan = manager
        .preview(
            ServiceInstallationChange::Install {
                installation: installed_service(),
            },
            now,
        )
        .unwrap();
    let receipt = manager
        .apply("operation-1", &plan, "user:1", &authority(), now)
        .unwrap();
    let retried = manager
        .apply("operation-1", &plan, "user:1", &authority(), now)
        .unwrap();

    assert_eq!(receipt, retried);
    assert_eq!(
        receipt.outcome,
        ServiceInstallationOutcome::AppliedNeedsAttention
    );
    assert_eq!(manager.snapshot().unwrap(), plan.target);
    assert!(
        root.join(".lenso/environments/local/service-installations.json")
            .is_file()
    );
    assert!(
        root.join(".lenso/environments/local/service-install-receipts/operation-1.json")
            .is_file()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_recovers_receipt_after_state_was_committed_first() {
    let root = temp_root("recover");
    let manager = WorkspaceServiceInstallationManager::new(&root, "acme/system", "local");
    let now = Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap();
    let plan = manager
        .preview(
            ServiceInstallationChange::Install {
                installation: installed_service(),
            },
            now,
        )
        .unwrap();
    let state_path = root.join(".lenso/environments/local/service-installations.json");
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(
        &state_path,
        serde_json::to_vec_pretty(&plan.target).unwrap(),
    )
    .unwrap();

    let receipt = manager
        .apply("operation-crash", &plan, "system", &authority(), now)
        .unwrap();
    assert_eq!(receipt.target_state_digest, plan.target_state_digest);
    assert!(
        root.join(".lenso/environments/local/service-install-receipts/operation-crash.json")
            .is_file()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_plan_cannot_overwrite_a_newer_installation_set() {
    let root = temp_root("stale");
    let manager = WorkspaceServiceInstallationManager::new(&root, "acme/system", "local");
    let now = Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap();
    let stale = manager
        .preview(
            ServiceInstallationChange::Install {
                installation: installed_service(),
            },
            now,
        )
        .unwrap();
    manager
        .apply("operation-new", &stale, "system", &authority(), now)
        .unwrap();
    let mut updated = installed_service();
    updated.desired_mode = ServiceDesiredMode::Inactive;
    let newer = manager
        .preview(
            ServiceInstallationChange::Install {
                installation: updated,
            },
            now,
        )
        .unwrap();
    manager
        .apply("operation-newer", &newer, "system", &authority(), now)
        .unwrap();

    assert!(matches!(
        manager.apply("operation-stale", &stale, "system", &authority(), now),
        Err(ServiceInstallationError::StaleState)
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn profile_replacement_and_secret_shaped_public_adapter_config_are_rejected() {
    let mut state = ServiceInstallationSet::empty("acme/system", "local");
    state.services.push(installed_service());
    let mut replacement = installed_service();
    replacement.profile = ServiceResponsibilityProfile::Autonomous;
    assert!(matches!(
        plan_service_installation(
            &state,
            ServiceInstallationChange::Install {
                installation: replacement
            },
            Utc::now()
        ),
        Err(ServiceInstallationError::InvalidContract(_))
    ));

    let mut invalid = installed_service();
    invalid.endpoint_binding.resolver_source = EndpointResolverSource::Adapter {
        adapter_id: "consul".to_owned(),
        public_config: BTreeMap::from([("access_token".to_owned(), "plain".to_owned())]),
        secret_references: Vec::new(),
    };
    assert!(matches!(
        validate_service_installation(&invalid),
        Err(ServiceInstallationError::InvalidContract(_))
    ));
}

#[test]
fn explicit_service_uninstall_removes_only_desired_installation_state() {
    let root = temp_root("uninstall");
    let manager = WorkspaceServiceInstallationManager::new(&root, "acme/system", "local");
    let now = Utc.with_ymd_and_hms(2026, 7, 30, 9, 0, 0).unwrap();
    let install = manager
        .preview(
            ServiceInstallationChange::Install {
                installation: installed_service(),
            },
            now,
        )
        .unwrap();
    manager
        .apply("install", &install, "system", &authority(), now)
        .unwrap();
    let uninstall = manager
        .preview(
            ServiceInstallationChange::Uninstall {
                service_ref: installed_service().service_ref,
            },
            now,
        )
        .unwrap();
    let receipt = manager
        .apply("uninstall", &uninstall, "system", &authority(), now)
        .unwrap();

    assert_eq!(receipt.outcome, ServiceInstallationOutcome::Removed);
    assert!(manager.snapshot().unwrap().services.is_empty());
    assert!(
        root.join(".lenso/environments/local/service-install-receipts/install.json")
            .is_file()
    );
    fs::remove_dir_all(root).unwrap();
}
