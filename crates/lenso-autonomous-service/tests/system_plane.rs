use axum::{Extension, body::Body};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    EnrollmentGrant, RuntimeObservabilityProvider, RuntimeOperationsProvider, ServiceRuntimeConfig,
    ServiceRuntimeState, SystemPlaneAccess, SystemPlaneRegistryBuilder, SystemPlaneRuntime,
    SystemSandboxEnrollmentAuthorizer, SystemSandboxManagementAuthorityVerifier, openapi_document,
    prepare_runtime, service_router,
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

#[test]
fn autonomous_service_openapi_publishes_core_discovery_security() {
    let document = serde_json::to_value(openapi_document()).unwrap();

    assert!(document["paths"]["/system-plane/v1"]["get"].is_object());
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

    assert_eq!(response.status(), StatusCode::OK);
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
