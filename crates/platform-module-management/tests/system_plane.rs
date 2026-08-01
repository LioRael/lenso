use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{TimeZone as _, Utc};
use lenso_contracts::ServiceResponsibilityProfile;
use lenso_module_management::{
    EndpointBinding, EndpointCachePolicy, EndpointResolverSource, EndpointSelectionPolicy,
    InstalledServiceExport, InstalledServiceRelease, ManagementActor, ServiceDesiredMode,
    ServiceIdentityPolicy, ServiceInstallation, ServiceInstallationChange, ServiceLifecycleBinding,
    ServiceReference, ServiceTransportBinding, StaticEndpointDeclaration,
};
use lenso_service::{
    AuthenticatedTransportBinding, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider,
};
use platform_module_management::{
    SERVICE_INSTALLATIONS_FEATURE_APPLY, SERVICE_INSTALLATIONS_FEATURE_PLAN,
    SERVICE_INSTALLATIONS_FEATURE_SNAPSHOT, SERVICE_INSTALLATIONS_PATH,
    SERVICE_INSTALLATIONS_PROTOCOL, ServiceInstallationsProvider,
    service_installations_schema_digest,
};
use platform_system_plane::{
    EnrollmentGrant, SystemPlaneAccess, SystemPlaneRegistryBuilder, SystemPlaneRuntime,
    SystemSandboxEnrollmentAuthorizer,
};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt as _;

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn temp_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "lenso-system-plane-installations-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn installation() -> ServiceInstallation {
    let service_ref = ServiceReference {
        system_id: "acme/system".to_owned(),
        service_id: "acme/support-service".to_owned(),
    };
    ServiceInstallation {
        service_ref: service_ref.clone(),
        profile: ServiceResponsibilityProfile::Provider,
        desired_mode: ServiceDesiredMode::Active,
        service_release: InstalledServiceRelease {
            version: "1.0.0".to_owned(),
            digest: digest('1'),
            immutable_locator: format!("oci://registry.example/support@{}", digest('1')),
        },
        exports: vec![InstalledServiceExport {
            export_key: "support".to_owned(),
            module_id: "acme/support".to_owned(),
            module_version: "1.0.0".to_owned(),
            module_release_digest: digest('2'),
            manifest_digest: digest('3'),
            contract_digests: vec![digest('4')],
        }],
        config_bindings: Vec::new(),
        endpoint_binding: EndpointBinding {
            binding_id: "support-primary".to_owned(),
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
                principal: "spiffe://acme.test/support".to_owned(),
                audience: "lenso-host".to_owned(),
                trust_profile: "production".to_owned(),
                credential_references: Vec::new(),
            },
            selection_policy: EndpointSelectionPolicy::default(),
            cache_policy: EndpointCachePolicy {
                maximum_age_seconds: 30,
                stale_if_source_unavailable_seconds: None,
            },
        },
        lifecycle_binding: ServiceLifecycleBinding::External {
            deployment_reference: "deployment://support/prod".to_owned(),
            observation_adapter_id: "kubernetes".to_owned(),
            operation_adapter_id: None,
        },
    }
}

#[test]
fn advertises_one_canonical_capability_contract() {
    let advertisement = ServiceInstallationsProvider::advertisement();

    assert_eq!(advertisement.contract_id, SERVICE_INSTALLATIONS_PROTOCOL);
    assert_eq!(advertisement.endpoint, SERVICE_INSTALLATIONS_PATH);
    assert_eq!(
        advertisement.schema_digest,
        service_installations_schema_digest()
    );
    assert_eq!(
        advertisement.feature_ids,
        BTreeSet::from([
            SERVICE_INSTALLATIONS_FEATURE_APPLY.to_owned(),
            SERVICE_INSTALLATIONS_FEATURE_PLAN.to_owned(),
            SERVICE_INSTALLATIONS_FEATURE_SNAPSHOT.to_owned(),
        ])
    );
}

#[test]
fn provider_owns_preview_apply_and_idempotent_receipt_flow() {
    let root = temp_root();
    let provider = ServiceInstallationsProvider::new(&root);
    let now = Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap();
    let plan = provider
        .preview(
            "acme/system",
            "production",
            ServiceInstallationChange::Install {
                installation: installation(),
            },
            now,
        )
        .unwrap();
    let actor = ManagementActor {
        actor_id: "service:console".to_owned(),
        verified_authorities: BTreeSet::from(["service.manage".to_owned()]),
    };

    let receipt = provider.apply("operation-1", &plan, &actor, now).unwrap();
    let retried = provider.apply("operation-1", &plan, &actor, now).unwrap();

    assert_eq!(receipt, retried);
    assert_eq!(
        provider.snapshot("acme/system", "production").unwrap(),
        plan.target
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn capability_is_served_only_from_the_authenticated_system_plane_path() {
    let root = temp_root();
    let provider = Arc::new(ServiceInstallationsProvider::new(&root));
    let identity = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "service-installations-secret").unwrap(),
    );
    let registry =
        SystemPlaneRegistryBuilder::new("host", "service:host", "release:sha256:0123456789abcdef")
            .register(ServiceInstallationsProvider::advertisement())
            .build()
            .unwrap();
    let enrollment = Arc::new(
        SystemSandboxEnrollmentAuthorizer::new(
            "test",
            EnrollmentGrant::system_sandbox("host", "service:console", 0, 4_000_000_000_000),
        )
        .unwrap(),
    );
    let runtime = Arc::new(SystemPlaneRuntime::new(
        registry,
        SystemPlaneAccess::new(identity.clone(), "service:host", enrollment),
    ));
    let app = platform_system_plane::router::<()>(Some(runtime.clone()))
        .merge(platform_module_management::system_plane_router(Some(
            provider,
        )))
        .layer(Extension(Some(runtime)))
        .layer(Extension(AuthenticatedTransportBinding::new(
            "tls:test-peer",
        )))
        .split_for_parts()
        .0;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let credential = identity
        .issue(WorkloadCredentialRequest::new(
            "service:console",
            "service:host",
            "tls:test-peer",
            now,
            Duration::from_secs(30).as_millis() as u64,
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::get(
                "/system-plane/v1/service-installations/production?system_id=acme%2Fsystem",
            )
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", credential.token),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let legacy = app
        .oneshot(
            Request::get("/admin/services/installations/production?system_id=acme%2Fsystem")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", credential.token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(legacy.status(), StatusCode::NOT_FOUND);
    fs::remove_dir_all(root).unwrap();
}
