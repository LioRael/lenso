use axum::{Extension, body::Body};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_service::{
    AuthenticatedTransportBinding, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider, system_plane::CapabilityAdvertisement,
};
use platform_system_plane::{
    CapabilityNegotiationIssueCode, CapabilityRequirement, SystemPlaneAccess,
    SystemPlaneRegistryBuilder, SystemPlaneRuntime, router,
};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tower::ServiceExt as _;

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn capability(major: u32) -> CapabilityAdvertisement {
    CapabilityAdvertisement {
        contract_id: format!("lenso.system-plane.runtime-observability.v{major}"),
        major_version: major,
        feature_ids: BTreeSet::from(["function-runs".to_owned(), "queue-summary".to_owned()]),
        schema_digest: digest(char::from_digit(major, 10).unwrap()),
        endpoint: format!("/system-plane/v1/runtime-observability-v{major}"),
    }
}

fn registry() -> platform_system_plane::SystemPlaneRegistry {
    SystemPlaneRegistryBuilder::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
    )
    .register(capability(2))
    .register(capability(1))
    .build()
    .unwrap()
}

#[test]
fn registry_publishes_capabilities_in_stable_contract_order() {
    let registry = registry();

    assert_eq!(registry.document().service_id, "support");
    assert_eq!(registry.document().capabilities[0].major_version, 1);
    assert_eq!(registry.document().capabilities[1].major_version, 2);
}

#[test]
fn negotiation_selects_highest_shared_major_and_checks_features_and_digest() {
    let requirement = CapabilityRequirement::new("runtime-observability", [1, 2])
        .requiring_features(["queue-summary"])
        .accepting_schema_digests([digest('2')]);

    let result = registry().negotiate(&[requirement]);

    assert!(result.is_compatible());
    assert_eq!(result.accepted[0].advertisement.major_version, 2);
}

#[test]
fn negotiation_isolates_requirement_failures() {
    let result = registry().negotiate(&[
        CapabilityRequirement::new("runtime-observability", [3]),
        CapabilityRequirement::new("runtime-observability", [2]),
        CapabilityRequirement::new("stories", [1]).requiring_features(["opaque-cursors"]),
    ]);

    assert!(!result.is_compatible());
    assert!(result.accepted.is_empty());
    assert_eq!(
        result
            .issues
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        vec![
            CapabilityNegotiationIssueCode::UnsupportedMajorVersion,
            CapabilityNegotiationIssueCode::DuplicateRequirement,
            CapabilityNegotiationIssueCode::MissingCapability,
        ]
    );
}

#[test]
fn negotiation_rejects_missing_features_and_unaccepted_schema_digests_independently() {
    let missing_feature =
        registry().negotiate(&[CapabilityRequirement::new("runtime-observability", [2])
            .requiring_features(["execution-payloads"])]);
    let wrong_digest =
        registry().negotiate(&[CapabilityRequirement::new("runtime-observability", [2])
            .accepting_schema_digests([digest('f')])]);

    assert_eq!(
        missing_feature.issues[0].code,
        CapabilityNegotiationIssueCode::MissingRequiredFeature
    );
    assert_eq!(
        wrong_digest.issues[0].code,
        CapabilityNegotiationIssueCode::SchemaDigestMismatch
    );
}

#[tokio::test]
async fn discovery_fails_closed_without_runtime_configuration() {
    let app = router::<()>(None).split_for_parts().0;

    let response = app
        .oneshot(
            Request::get("/system-plane/v1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn discovery_requires_enrolled_transport_bound_workload_identity() {
    let provider = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "system-plane-test-secret").unwrap(),
    );
    let runtime = Arc::new(SystemPlaneRuntime::new(
        registry(),
        SystemPlaneAccess::new(provider.clone(), "service:support", "service:console"),
    ));
    let app = router::<()>(Some(runtime))
        .split_for_parts()
        .0
        .layer(Extension(AuthenticatedTransportBinding::new(
            "tls:test-peer",
        )));
    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:console",
            "service:support",
            "tls:test-peer",
            now_unix_ms(),
            Duration::from_secs(30).as_millis() as u64,
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
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["serviceId"], "support");
    assert_eq!(body["capabilities"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn discovery_rejects_a_valid_but_unenrolled_service_principal() {
    let provider = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "system-plane-enrollment-secret")
            .unwrap(),
    );
    let runtime = Arc::new(SystemPlaneRuntime::new(
        registry(),
        SystemPlaneAccess::new(provider.clone(), "service:support", "service:console"),
    ));
    let app = router::<()>(Some(runtime))
        .split_for_parts()
        .0
        .layer(Extension(AuthenticatedTransportBinding::new(
            "tls:test-peer",
        )));
    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:other-console",
            "service:support",
            "tls:test-peer",
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

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers()["x-lenso-error-code"],
        "system_plane_console_not_enrolled"
    );
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
