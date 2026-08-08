use axum::{Extension, body::Body};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    EnrollmentGrant, ModuleOperationsProvider, RuntimeObservabilityProvider,
    RuntimeOperationsProvider, ServiceRuntimeConfig, ServiceRuntimeState, SystemPlaneAccess,
    SystemPlaneRegistryBuilder, SystemPlaneRuntime, SystemSandboxEnrollmentAuthorizer,
    SystemSandboxManagementAuthorityVerifier, openapi_document, prepare_runtime, service_router,
};
use lenso_service::system_plane::{
    MODULE_OPERATIONS_FEATURE_INVENTORY_READ, ManagedServiceContext, ModuleInventoryRequest,
};
use lenso_service::{
    AuthenticatedTransportBinding, AutonomousServiceContract, AutonomousServiceStore,
    AutonomousServiceWorkload, ServiceTenancyMode, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider, WorkloadRole,
};
use platform_testing::TestDatabase;
use std::sync::Arc;
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

fn runtime(provider: Arc<SystemSandboxWorkloadIdentityProvider>) -> SystemPlaneRuntime {
    let registry = SystemPlaneRegistryBuilder::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
    )
    .register(RuntimeObservabilityProvider::advertisement())
    .build()
    .unwrap();
    SystemPlaneRuntime::new(
        registry,
        SystemPlaneAccess::new(
            provider,
            "service:support",
            Arc::new(
                SystemSandboxEnrollmentAuthorizer::new(
                    "test",
                    EnrollmentGrant::system_sandbox(
                        "support",
                        "service:console",
                        0,
                        4_000_000_000_000,
                    ),
                )
                .unwrap(),
            ),
        ),
    )
}

fn runtime_with_operations(
    provider: Arc<SystemSandboxWorkloadIdentityProvider>,
) -> SystemPlaneRuntime {
    let registry = SystemPlaneRegistryBuilder::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
    )
    .register(RuntimeObservabilityProvider::advertisement())
    .register(RuntimeOperationsProvider::advertisement())
    .build()
    .unwrap();
    SystemPlaneRuntime::new(
        registry,
        SystemPlaneAccess::new(
            provider,
            "service:support",
            Arc::new(
                SystemSandboxEnrollmentAuthorizer::new(
                    "test",
                    EnrollmentGrant::system_sandbox(
                        "support",
                        "service:console",
                        0,
                        4_000_000_000_000,
                    ),
                )
                .unwrap(),
            ),
        ),
    )
}

fn runtime_with_module_operations(
    provider: Arc<SystemSandboxWorkloadIdentityProvider>,
    modules: ModuleOperationsProvider,
) -> SystemPlaneRuntime {
    let registry = SystemPlaneRegistryBuilder::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
    )
    .register(ModuleOperationsProvider::advertisement())
    .build()
    .unwrap();
    SystemPlaneRuntime::new(
        registry,
        SystemPlaneAccess::new(
            provider,
            "service:support",
            Arc::new(
                SystemSandboxEnrollmentAuthorizer::new(
                    "test",
                    EnrollmentGrant::system_sandbox(
                        "support",
                        "service:console",
                        0,
                        4_000_000_000_000,
                    ),
                )
                .unwrap(),
            ),
        ),
    )
    .with_module_operations(modules)
}

fn service() -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        "support",
        vec![
            AutonomousServiceWorkload::new("support-api", "support", WorkloadRole::API),
            AutonomousServiceWorkload::new("support-migrate", "support", WorkloadRole::MIGRATION),
            AutonomousServiceWorkload::new("support-worker", "support", WorkloadRole::WORKER),
        ],
        ServiceTenancyMode::None,
        vec!["local".to_owned()],
    );
    service.stores = vec![AutonomousServiceStore::new("primary", "support")];
    service
}

#[tokio::test]
async fn autonomous_service_mounts_authenticated_core_discovery_without_story_writes() {
    let provider = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "autonomous-system-plane-secret")
            .unwrap(),
    );
    let state = ServiceRuntimeState::ready(
        "support",
        "support-api",
        "primary",
        "support-migrate",
        "support-worker",
    )
    .with_system_plane(runtime(provider.clone()));
    let app = service_router(OpenApiRouter::new(), state).layer(Extension(
        AuthenticatedTransportBinding::new("tls:console-peer"),
    ));
    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:console",
            "service:support",
            "tls:console-peer",
            now_unix_ms(),
            30_000,
        ))
        .unwrap();

    let response = app
        .oneshot(
            Request::get("/system-plane/v1")
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
    let document: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(document["serviceId"], "support");
    assert_eq!(document["servicePrincipal"], "service:support");
}

#[tokio::test]
async fn managed_service_module_inventory_request_is_typed_and_authoritative() {
    let identity = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "module-operations-secret").unwrap(),
    );
    let release: lenso_contracts::ModuleRelease = serde_json::from_value(
        lenso_contracts::console_contract_vectors()["positive"]["release"].clone(),
    )
    .unwrap();
    let modules = ModuleOperationsProvider::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
        [release],
    )
    .unwrap();
    let state = ServiceRuntimeState::ready(
        "support",
        "support-api",
        "primary",
        "support-migrate",
        "support-worker",
    )
    .with_system_plane(runtime_with_module_operations(identity.clone(), modules));
    let app = service_router(OpenApiRouter::new(), state).layer(Extension(
        AuthenticatedTransportBinding::new("tls:module-operations"),
    ));
    let credential = identity
        .issue(WorkloadCredentialRequest::new(
            "service:console",
            "service:support",
            "tls:module-operations",
            now_unix_ms(),
            30_000,
        ))
        .unwrap();
    let request = ModuleInventoryRequest {
        context: ManagedServiceContext::new(
            "system-sandbox",
            "support",
            "local",
            "service:support",
            "acme/support-console",
            "operator-1",
            format!("sha256:{}", "f".repeat(64)),
            [MODULE_OPERATIONS_FEATURE_INVENTORY_READ],
        ),
    };
    let response = app
        .oneshot(
            Request::post("/system-plane/v1/modules")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", credential.token),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let snapshot: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        snapshot["protocol"],
        "lenso.system-plane.module-operations.v1"
    );
    assert_eq!(snapshot["modules"][0]["moduleId"], "acme/support-console");
    assert_eq!(
        snapshot["modules"][0]["consoleUi"]["format"],
        "console_ui_esm"
    );
}

#[test]
fn autonomous_service_openapi_publishes_core_discovery_security() {
    let document = serde_json::to_value(openapi_document()).unwrap();

    assert!(document["paths"]["/system-plane/v1"]["get"].is_object());
    assert!(document["paths"]["/system-plane/v1/modules"]["post"].is_object());
    assert!(
        document["paths"]["/system-plane/v1/modules/action-contributions/resolve"]["post"]
            .is_object()
    );
    assert!(document["paths"]["/system-plane/v1/modules/config/read"]["post"].is_object());
    assert!(document["paths"]["/system-plane/v1/modules/config/write"]["post"].is_object());
    assert!(document["paths"]["/system-plane/v1/runtime-observability"]["get"].is_object());
    assert!(
        document["paths"]["/system-plane/v1/runtime-operations/function-runs/{id}"]["get"]
            .is_object()
    );
    assert!(
        document["paths"]["/system-plane/v1/runtime-operations/outbox-events/{id}"]["get"]
            .is_object()
    );
    assert!(document["paths"]["/system-plane/v1/runtime-operations/plans"]["post"].is_object());
    assert!(
        document["paths"]["/system-plane/v1/runtime-operations/operations"]["post"].is_object()
    );
    assert!(
        document["paths"]["/system-plane/v1/runtime-operations/operations/{id}"]["get"].is_object()
    );
    assert!(
        document["paths"]["/system-plane/v1/runtime-operations/operations/{id}/evidence"]["get"]
            .is_object()
    );
    assert!(
        document["paths"]
            ["/system-plane/v1/runtime-operations/operations/by-idempotency-key/{idempotency_key}"]
            ["get"]
            .is_object()
    );
    assert_eq!(
        document["paths"]["/system-plane/v1"]["get"]["security"][0]["bearer_auth"],
        serde_json::json!([])
    );
    assert_eq!(
        document["components"]["securitySchemes"]["bearer_auth"]["scheme"],
        "bearer"
    );
}

#[tokio::test]
async fn prepared_autonomous_service_mounts_advertised_runtime_capability_providers() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let identity = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "prepared-system-plane-secret").unwrap(),
    );
    let config = ServiceRuntimeConfig::new("support", "primary", "support")
        .with_system_plane(runtime_with_operations(identity.clone()))
        .with_runtime_observability(RuntimeObservabilityProvider::new(
            db.pool.clone(),
            "support",
            "release:sha256:0123456789abcdef",
        ))
        .with_runtime_operations(
            RuntimeOperationsProvider::new(
                db.pool.clone(),
                "support",
                "release:sha256:0123456789abcdef",
            )
            .with_authority_verifier(Arc::new(
                SystemSandboxManagementAuthorityVerifier::new("test").unwrap(),
            )),
        );
    let state = prepare_runtime(&service(), &config, db.pool.clone(), &[])
        .await
        .unwrap();
    let app = service_router(OpenApiRouter::new(), state).layer(Extension(
        AuthenticatedTransportBinding::new("tls:prepared-console"),
    ));
    let credential = identity
        .issue(WorkloadCredentialRequest::new(
            "service:console",
            "service:support",
            "tls:prepared-console",
            now_unix_ms(),
            30_000,
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::get("/system-plane/v1/runtime-observability")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", credential.token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected Runtime Observability response: {}",
        String::from_utf8_lossy(&body)
    );
    let response = app
        .oneshot(
            Request::get("/system-plane/v1/runtime-operations/function-runs/missing")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", credential.token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    drop(config);
    db.cleanup().await;
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
