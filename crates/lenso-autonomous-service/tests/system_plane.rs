use axum::{Extension, body::Body};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    ServiceRuntimeState, SystemPlaneAccess, SystemPlaneRegistryBuilder, SystemPlaneRuntime,
    openapi_document, service_router,
};
use lenso_service::{
    AuthenticatedTransportBinding, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider, system_plane::CapabilityAdvertisement,
};
use std::{collections::BTreeSet, sync::Arc};
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

fn runtime(provider: Arc<SystemSandboxWorkloadIdentityProvider>) -> SystemPlaneRuntime {
    let registry = SystemPlaneRegistryBuilder::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
    )
    .register(CapabilityAdvertisement {
        contract_id: "lenso.system-plane.runtime-observability.v1".to_owned(),
        major_version: 1,
        feature_ids: BTreeSet::from(["queue-summary".to_owned()]),
        schema_digest: format!("sha256:{}", "a".repeat(64)),
        endpoint: "/system-plane/v1/runtime-observability".to_owned(),
    })
    .build()
    .unwrap();
    SystemPlaneRuntime::new(
        registry,
        SystemPlaneAccess::new(provider, "service:support", "service:console"),
    )
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
    assert_eq!(
        document["paths"]["/system-plane/v1"]["get"]["security"][0]["bearer_auth"],
        serde_json::json!([])
    );
    assert_eq!(
        document["components"]["securitySchemes"]["bearer_auth"]["scheme"],
        "bearer"
    );
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
