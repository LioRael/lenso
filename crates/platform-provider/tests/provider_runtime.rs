use axum::http::{HeaderMap, StatusCode};
use axum::{Json, Router, routing::get};
use lenso_module_management::{
    EndpointBinding, EndpointCachePolicy, EndpointResolverSource, EndpointSelectionPolicy,
    InstalledServiceRelease, PROVIDER_RUNTIME_PLAN_PROTOCOL, ProviderRuntimeModule,
    ProviderRuntimePlan, ProviderRuntimeService, ServiceIdentityPolicy, ServiceReference,
    ServiceTransportBinding, StaticEndpointDeclaration,
};
use platform_module::ModuleManifest;
use platform_provider::{
    EnvironmentBearerCredentialResolver, FixedBearerCredentialResolver,
    FixedProviderEndpointResolver, ProviderCredentialResolver, ProviderRuntimeAdapter,
    ProviderRuntimeAdapters,
};
use serde_json::json;
use std::sync::Arc;
use tokio::net::TcpListener;

async fn spawn_descriptor(manifest: ModuleManifest, sibling: Option<ModuleManifest>) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/",
        get(move || {
            let manifest = manifest.clone();
            let sibling = sibling.clone();
            async move {
                let mut exports = vec![descriptor_export(
                    manifest,
                    "support",
                    digest('3'),
                    digest('4'),
                )];
                if let Some(sibling) = sibling {
                    exports.push(descriptor_export(
                        sibling,
                        "sibling",
                        digest('6'),
                        digest('7'),
                    ));
                }
                Json(descriptor(exports))
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

async fn spawn_authenticated_descriptor(manifest: ModuleManifest) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().route(
        "/",
        get(move |headers: HeaderMap| {
            let manifest = manifest.clone();
            async move {
                if headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    != Some("Bearer provider-secret")
                {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({ "error": "unauthorized" })),
                    );
                }
                (
                    StatusCode::OK,
                    Json(descriptor(vec![descriptor_export(
                        manifest,
                        "support",
                        digest('3'),
                        digest('4'),
                    )])),
                )
            }
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

fn descriptor(exports: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "protocol": "lenso.provider.v1",
        "protocolContractDigest": digest('9'),
        "serviceId": "acme/support-service",
        "serviceReleaseVersion": "1.0.0",
        "serviceReleaseDigest": digest('2'),
        "runtimeInstanceId": "test-provider-1",
        "features": ["durable_invocations"],
        "transports": ["http_json"],
        "exports": exports,
    })
}

fn descriptor_export(
    manifest: ModuleManifest,
    export_key: &str,
    module_release_digest: String,
    manifest_digest: String,
) -> serde_json::Value {
    let module_id = manifest.module_id.clone();
    json!({
        "exportKey": export_key,
        "moduleId": module_id,
        "moduleVersion": "1.0.0",
        "moduleReleaseDigest": module_release_digest,
        "manifestDigest": manifest_digest,
        "manifest": manifest,
        "contractDigests": { "operation": digest('5') },
        "ready": true,
        "readinessReasons": [],
    })
}

fn plan(endpoint: String, manifest: ModuleManifest) -> ProviderRuntimePlan {
    ProviderRuntimePlan {
        protocol: PROVIDER_RUNTIME_PLAN_PROTOCOL.to_owned(),
        system_id: "acme/system".to_owned(),
        application_id: "acme/app".to_owned(),
        environment_id: "test".to_owned(),
        application_lock_digest: digest('1'),
        service_installation_revision: 3,
        providers: vec![ProviderRuntimeService {
            service_ref: ServiceReference {
                system_id: "acme/system".to_owned(),
                service_id: "acme/support-service".to_owned(),
            },
            service_release: InstalledServiceRelease {
                version: "1.0.0".to_owned(),
                digest: digest('2'),
                immutable_locator: "oci://example/support@sha256:2222".to_owned(),
            },
            endpoint_binding: EndpointBinding {
                binding_id: "support-provider".to_owned(),
                service_ref: ServiceReference {
                    system_id: "acme/system".to_owned(),
                    service_id: "acme/support-service".to_owned(),
                },
                resolver_source: EndpointResolverSource::Static {
                    endpoints: vec![StaticEndpointDeclaration {
                        address: endpoint,
                        binding: ServiceTransportBinding::ProviderHttpJson,
                        region: Some("test".to_owned()),
                        failure_domain: Some("test-1".to_owned()),
                        priority: 0,
                        weight: 1,
                    }],
                },
                allowed_bindings: vec![ServiceTransportBinding::ProviderHttpJson],
                identity_policy: ServiceIdentityPolicy {
                    principal: "spiffe://acme/support".to_owned(),
                    audience: "lenso-host".to_owned(),
                    trust_profile: "test".to_owned(),
                    credential_references: Vec::new(),
                },
                selection_policy: EndpointSelectionPolicy {
                    preferred_regions: vec!["test".to_owned()],
                    require_distinct_failure_domains: false,
                },
                cache_policy: EndpointCachePolicy {
                    maximum_age_seconds: 30,
                    stale_if_source_unavailable_seconds: None,
                },
            },
            modules: vec![ProviderRuntimeModule {
                export_key: "support".to_owned(),
                module_id: "acme/support".to_owned(),
                module_version: "1.0.0".to_owned(),
                module_release_digest: digest('3'),
                manifest_digest: digest('4'),
                contract_digests: vec![digest('5')],
                manifest,
            }],
        }],
    }
}

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

#[tokio::test]
async fn loads_only_locked_module_after_live_descriptor_verification() {
    let locked = ModuleManifest::builder("acme/support")
        .capabilities(vec!["support.read".to_owned()])
        .build();
    let sibling = ModuleManifest::builder("acme/notifications")
        .capabilities(vec!["notifications.send".to_owned()])
        .build();
    let endpoint = spawn_descriptor(locked.clone(), Some(sibling)).await;

    let loaded = ProviderRuntimeAdapter::new(plan(endpoint, locked))
        .unwrap()
        .load_verified()
        .await
        .unwrap();
    let modules = loaded.into_modules();

    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].manifest.module_id, "acme/support");
}

#[tokio::test]
async fn rejects_live_descriptor_that_differs_from_locked_manifest() {
    let locked = ModuleManifest::builder("acme/support")
        .capabilities(vec!["support.read".to_owned()])
        .build();
    let changed = ModuleManifest::builder("acme/support")
        .capabilities(vec!["support.write".to_owned()])
        .build();
    let endpoint = spawn_descriptor(changed, None).await;

    let error = ProviderRuntimeAdapter::new(plan(endpoint, locked))
        .unwrap()
        .load_verified()
        .await
        .unwrap_err();

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert!(error.public_message.contains("locked Module Release"));
}

#[tokio::test]
async fn rejects_unresolved_non_static_endpoint_sources() {
    let manifest = ModuleManifest::builder("acme/support").build();
    let mut runtime_plan = plan("http://127.0.0.1:1".to_owned(), manifest);
    runtime_plan.providers[0].endpoint_binding.resolver_source = EndpointResolverSource::Adapter {
        adapter_id: "consul".to_owned(),
        public_config: Default::default(),
        secret_references: Vec::new(),
    };

    let error = ProviderRuntimeAdapter::new(runtime_plan)
        .unwrap()
        .load_verified()
        .await
        .unwrap_err();

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
    assert!(error.public_message.contains("resolver adapter"));
}

#[tokio::test]
async fn injected_endpoint_resolver_activates_non_static_provider_source() {
    let manifest = ModuleManifest::builder("acme/support").build();
    let endpoint = spawn_descriptor(manifest.clone(), None).await;
    let mut runtime_plan = plan("http://127.0.0.1:1".to_owned(), manifest);
    runtime_plan.providers[0].endpoint_binding.resolver_source =
        EndpointResolverSource::LocalProcess {
            source_id: "local-supervisor".to_owned(),
        };
    let service_ref = runtime_plan.providers[0].service_ref.clone();
    let resolved = StaticEndpointDeclaration {
        address: endpoint,
        binding: ServiceTransportBinding::ProviderHttpJson,
        region: Some("test".to_owned()),
        failure_domain: Some("test-1".to_owned()),
        priority: 0,
        weight: 1,
    };
    let adapters = ProviderRuntimeAdapters::default().with_endpoint_resolver(
        "local-supervisor",
        Arc::new(FixedProviderEndpointResolver::new([(
            service_ref,
            vec![resolved],
        )])),
    );

    let loaded = ProviderRuntimeAdapter::with_adapters(runtime_plan, adapters)
        .unwrap()
        .load_verified()
        .await
        .unwrap();

    assert_eq!(loaded.into_modules().len(), 1);
}

#[tokio::test]
async fn rejects_unresolved_provider_credentials_instead_of_ignoring_them() {
    let manifest = ModuleManifest::builder("acme/support").build();
    let mut runtime_plan = plan("http://127.0.0.1:1".to_owned(), manifest);
    runtime_plan.providers[0]
        .endpoint_binding
        .identity_policy
        .credential_references = vec!["secret://support/client-identity".to_owned()];

    let error = ProviderRuntimeAdapter::new(runtime_plan)
        .unwrap()
        .load_verified()
        .await
        .unwrap_err();

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
    assert!(error.public_message.contains("credential adapter"));
}

#[tokio::test]
async fn bearer_env_rejects_non_environment_credential_references() {
    let manifest = ModuleManifest::builder("acme/support").build();
    let mut runtime_plan = plan("http://127.0.0.1:1".to_owned(), manifest);
    let policy = &mut runtime_plan.providers[0].endpoint_binding.identity_policy;
    policy.credential_references = vec!["secret://support/client-identity".to_owned()];

    let error = EnvironmentBearerCredentialResolver
        .resolve_bearer(policy)
        .await
        .unwrap_err();

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
    assert!(error.public_message.contains("only opaque env://"));
}

#[tokio::test]
async fn injected_credential_resolver_authenticates_descriptor_request() {
    let manifest = ModuleManifest::builder("acme/support").build();
    let endpoint = spawn_authenticated_descriptor(manifest.clone()).await;
    let mut runtime_plan = plan(endpoint, manifest);
    runtime_plan.providers[0]
        .endpoint_binding
        .identity_policy
        .trust_profile = "fixture-bearer".to_owned();
    runtime_plan.providers[0]
        .endpoint_binding
        .identity_policy
        .credential_references = vec!["secret://support/client-identity".to_owned()];
    let adapters = ProviderRuntimeAdapters::default().with_credential_resolver(
        "fixture-bearer",
        Arc::new(FixedBearerCredentialResolver::new("provider-secret")),
    );

    let loaded = ProviderRuntimeAdapter::with_adapters(runtime_plan, adapters)
        .unwrap()
        .load_verified()
        .await
        .unwrap();

    assert!(!format!("{loaded:?}").contains("provider-secret"));
    assert!(!format!("{:?}", loaded.proxy_registry()).contains("provider-secret"));
    assert_eq!(loaded.into_modules().len(), 1);
}
