use axum::{Extension, body::Body};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_service::{
    AuthenticatedTransportBinding, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider,
    system_plane::{
        CapabilityAdvertisement, Ed25519EnrollmentSigner, Ed25519EnrollmentTrustStore,
        EnrollmentCapabilityGrant, EnrollmentOffer, EnrollmentPolicyGrant, EnrollmentSignature,
        EnrollmentSignatureAlgorithm, sign_enrollment_offer, verify_enrollment_receipt,
    },
};
use platform_system_plane::{
    AuthorizedSystemPlaneCaller, CapabilityNegotiationIssueCode, CapabilityRequirement,
    EnrollmentAcceptance, EnrollmentAuthorization, EnrollmentAuthorizer, EnrollmentCeremony,
    EnrollmentErrorCode, EnrollmentGrant, PostgresEnrollmentStore, SYSTEM_PLANE_MIGRATIONS,
    SystemPlaneAccess, SystemPlaneRegistryBuilder, SystemPlaneRuntime,
    SystemSandboxEnrollmentAuthorizer, router,
};
use platform_testing::TestDatabase;
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

fn enrollment(principal: &str) -> Arc<SystemSandboxEnrollmentAuthorizer> {
    Arc::new(
        SystemSandboxEnrollmentAuthorizer::new(
            "test",
            EnrollmentGrant::system_sandbox("support", principal, 0, 4_000_000_000_000),
        )
        .unwrap(),
    )
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

#[test]
fn capability_authorization_requires_exact_enrollment_contract_digest_and_features() {
    let identity = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "capability-grant-secret").unwrap(),
    );
    let runtime = Arc::new(SystemPlaneRuntime::new(
        registry(),
        SystemPlaneAccess::new(identity, "service:support", enrollment("service:console")),
    ));
    let caller = AuthorizedSystemPlaneCaller {
        runtime,
        service_principal: "service:console".to_owned(),
        enrollment: EnrollmentAuthorization {
            system_id: "customer-support".to_owned(),
            managed_service_id: "support".to_owned(),
            managed_service_principal: "service:support".to_owned(),
            managed_service_revision: "release:sha256:0123456789abcdef".to_owned(),
            console_service_principal: "service:console".to_owned(),
            receipt_digest: digest('9'),
            grant_revision: 1,
            authorization_epoch: 4,
            expires_at_unix_ms: 20_000,
            capabilities: vec![EnrollmentCapabilityGrant {
                contract_id: "lenso.system-plane.runtime-observability.v1".to_owned(),
                schema_digest: digest('a'),
                feature_ids: BTreeSet::from(["queue-summary".to_owned()]),
            }],
            policy: EnrollmentPolicyGrant {
                policy_id: "support-system-plane".to_owned(),
                policy_revision: "revision:1".to_owned(),
                policy_digest: digest('b'),
            },
        },
    };

    assert!(
        caller
            .require_capability(
                "lenso.system-plane.runtime-observability.v1",
                &digest('a'),
                ["queue-summary"],
            )
            .is_ok()
    );
    assert_eq!(
        caller
            .require_capability(
                "lenso.system-plane.runtime-observability.v1",
                &digest('c'),
                ["queue-summary"],
            )
            .unwrap_err()
            .code(),
        "system_plane_capability_not_granted"
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
        SystemPlaneAccess::new(
            provider.clone(),
            "service:support",
            enrollment("service:console"),
        ),
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
        SystemPlaneAccess::new(
            provider.clone(),
            "service:support",
            enrollment("service:console"),
        ),
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

#[tokio::test]
async fn signed_ceremony_atomically_persists_bilateral_receipt_grant_and_audit() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    platform_core::apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, SYSTEM_PLANE_MIGRATIONS)
        .await
        .unwrap();
    let console_signer = Arc::new(Ed25519EnrollmentSigner::new("console-key-1", [7; 32]).unwrap());
    let next_console_signer =
        Arc::new(Ed25519EnrollmentSigner::new("console-key-2", [8; 32]).unwrap());
    let service_signer = Arc::new(Ed25519EnrollmentSigner::new("service-key-1", [9; 32]).unwrap());
    let console_trust = Arc::new(
        Ed25519EnrollmentTrustStore::new([
            ("console-key-1", console_signer.verifying_key_bytes()),
            ("console-key-2", next_console_signer.verifying_key_bytes()),
        ])
        .unwrap(),
    );
    let service_trust =
        Ed25519EnrollmentTrustStore::new([("service-key-1", service_signer.verifying_key_bytes())])
            .unwrap();
    let requested_capability = EnrollmentCapabilityGrant {
        contract_id: "lenso.system-plane.runtime-observability.v1".to_owned(),
        schema_digest: digest('a'),
        feature_ids: BTreeSet::from(["queue-summary".to_owned()]),
    };
    let policy = EnrollmentPolicyGrant {
        policy_id: "support-system-plane".to_owned(),
        policy_revision: "revision:1".to_owned(),
        policy_digest: digest('b'),
    };
    let offer = sign_enrollment_offer(
        EnrollmentOffer {
            protocol: String::new(),
            system_id: "customer-support".to_owned(),
            console_service_principal: "service:console".to_owned(),
            nonce: "nonce-0123456789abcdef".to_owned(),
            issued_at_unix_ms: 1_000,
            expires_at_unix_ms: 20_000,
            requested_capabilities: vec![requested_capability.clone()],
            requested_policy: policy.clone(),
            signature: EnrollmentSignature {
                algorithm: EnrollmentSignatureAlgorithm::Ed25519,
                key_id: String::new(),
                subject_digest: String::new(),
                value: String::new(),
            },
        },
        console_signer.as_ref(),
    )
    .unwrap();
    let ceremony = EnrollmentCeremony::new(
        PostgresEnrollmentStore::new(db.pool.clone()),
        console_trust,
        service_signer.clone(),
    );
    let acceptance = EnrollmentAcceptance {
        managed_service_id: "support".to_owned(),
        managed_service_principal: "service:support".to_owned(),
        managed_service_revision: "release:sha256:0123456789abcdef".to_owned(),
        grant_revision: 1,
        authorization_epoch: 4,
        expires_at_unix_ms: 18_000,
        capabilities: vec![requested_capability],
        policy,
    };

    let receipt = ceremony.accept(&offer, &acceptance, 2_000).await.unwrap();
    let replay = ceremony.accept(&offer, &acceptance, 2_000).await.unwrap();
    assert_eq!(receipt, replay);
    assert!(verify_enrollment_receipt(&receipt, &offer, &service_trust, 3_000).is_ok());
    let authorization = PostgresEnrollmentStore::new(db.pool.clone())
        .authorize("support", "service:console", 3_000)
        .await
        .unwrap();
    assert_eq!(authorization.system_id, "customer-support");
    assert_eq!(authorization.managed_service_principal, "service:support");
    assert_eq!(authorization.authorization_epoch, 4);
    assert_eq!(authorization.capabilities.len(), 1);

    let store = PostgresEnrollmentStore::new(db.pool.clone());
    let revoked = store.revoke("support", 4, 4_000).await.unwrap();
    assert_eq!(revoked.grant.authorization_epoch, 5);
    assert_eq!(
        store
            .authorize("support", "service:console", 5_000)
            .await
            .unwrap_err()
            .code,
        EnrollmentErrorCode::Revoked
    );
    let next_offer = sign_enrollment_offer(
        EnrollmentOffer {
            console_service_principal: "service:next-console".to_owned(),
            nonce: "nonce-fedcba9876543210".to_owned(),
            ..offer.clone()
        },
        next_console_signer.as_ref(),
    )
    .unwrap();
    let next_acceptance = EnrollmentAcceptance {
        grant_revision: 2,
        authorization_epoch: 6,
        ..acceptance.clone()
    };
    let next_receipt = ceremony
        .accept(&next_offer, &next_acceptance, 5_000)
        .await
        .unwrap();
    assert!(verify_enrollment_receipt(&next_receipt, &next_offer, &service_trust, 6_000).is_ok());
    assert_eq!(
        store
            .authorize("support", "service:console", 6_000)
            .await
            .unwrap_err()
            .code,
        EnrollmentErrorCode::PrincipalMismatch
    );
    assert_eq!(
        store
            .authorize("support", "service:next-console", 6_000)
            .await
            .unwrap()
            .authorization_epoch,
        6
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from platform.system_plane_enrollment_audit where managed_service_id = 'support'",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        3
    );

    db.cleanup().await;
}
