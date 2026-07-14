use lenso_service::{
    SystemSandboxWorkloadIdentityProvider, WorkloadCredentialRequest, WorkloadIdentityErrorCode,
    WorkloadIdentityProvider, WorkloadIdentityVerification,
};

const NOW_MS: u64 = 4_102_444_800_000;

fn provider() -> SystemSandboxWorkloadIdentityProvider {
    SystemSandboxWorkloadIdentityProvider::new("local", "sandbox-secret-v1").unwrap()
}

#[test]
fn system_sandbox_issues_and_verifies_a_short_lived_service_principal() {
    let provider = provider();
    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "service:billing",
            "sandbox-transport:billing-api",
            NOW_MS,
            30_000,
        ))
        .unwrap();

    assert_eq!(credential.service_principal, "service:support");
    assert_eq!(credential.expires_at_unix_ms, NOW_MS + 30_000);
    assert!(credential.issuer.contains("development-only"));

    let authenticated = provider
        .verify(
            &credential.token,
            &WorkloadIdentityVerification::new(
                "service:billing",
                "sandbox-transport:billing-api",
                NOW_MS + 1,
            ),
        )
        .unwrap();
    assert_eq!(authenticated.service_principal, "service:support");
    assert_eq!(authenticated.credential_id, credential.credential_id);
    assert_eq!(authenticated.evidence.outcome, "authenticated");
}

#[test]
fn system_sandbox_rejects_non_development_configuration() {
    let error =
        SystemSandboxWorkloadIdentityProvider::new("production", "sandbox-secret").unwrap_err();

    assert_eq!(
        error.code,
        WorkloadIdentityErrorCode::DevelopmentProviderForbidden
    );
    assert_eq!(error.evidence.outcome, "development_provider_forbidden");
}

#[test]
fn system_sandbox_rejects_credentials_that_are_not_short_lived() {
    let error = provider()
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "service:billing",
            "sandbox-transport:billing-api",
            NOW_MS,
            300_001,
        ))
        .unwrap_err();

    assert_eq!(error.code, WorkloadIdentityErrorCode::InvalidRequest);
    assert_eq!(error.evidence.outcome, "invalid_credential_request");
}

#[test]
fn verification_checks_audience_expiry_proof_and_transport_binding() {
    let provider = provider();
    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "service:billing",
            "sandbox-transport:billing-api",
            NOW_MS,
            30_000,
        ))
        .unwrap();

    for (verification, expected) in [
        (
            WorkloadIdentityVerification::new(
                "service:other",
                "sandbox-transport:billing-api",
                NOW_MS + 1,
            ),
            WorkloadIdentityErrorCode::AudienceMismatch,
        ),
        (
            WorkloadIdentityVerification::new(
                "service:billing",
                "sandbox-transport:other",
                NOW_MS + 1,
            ),
            WorkloadIdentityErrorCode::TransportBindingMismatch,
        ),
        (
            WorkloadIdentityVerification::new(
                "service:billing",
                "sandbox-transport:billing-api",
                NOW_MS + 30_000,
            ),
            WorkloadIdentityErrorCode::CredentialExpired,
        ),
    ] {
        assert_eq!(
            provider
                .verify(&credential.token, &verification)
                .unwrap_err()
                .code,
            expected
        );
    }

    let mut tampered = credential.token;
    tampered.push('x');
    assert_eq!(
        provider
            .verify(
                &tampered,
                &WorkloadIdentityVerification::new(
                    "service:billing",
                    "sandbox-transport:billing-api",
                    NOW_MS + 1,
                ),
            )
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::InvalidProof
    );
}

#[test]
fn rotation_preserves_service_identity_and_reports_stale_and_failed_rotation() {
    let provider = provider();
    let before = provider
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "service:billing",
            "sandbox-transport:billing-api",
            NOW_MS,
            30_000,
        ))
        .unwrap();

    let rotation = provider.rotate("sandbox-secret-v2").unwrap();
    assert_eq!(rotation.outcome, "rotated");
    assert_ne!(rotation.previous_key_id, rotation.active_key_id);
    let mut forged_stale = before.token.clone();
    forged_stale.push('x');
    assert_eq!(
        provider
            .verify(
                &forged_stale,
                &WorkloadIdentityVerification::new(
                    "service:billing",
                    "sandbox-transport:billing-api",
                    NOW_MS + 1,
                ),
            )
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::InvalidProof
    );
    assert_eq!(
        provider
            .verify(
                &before.token,
                &WorkloadIdentityVerification::new(
                    "service:billing",
                    "sandbox-transport:billing-api",
                    NOW_MS + 1,
                ),
            )
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::StaleCredential
    );

    let after = provider
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "service:billing",
            "sandbox-transport:billing-api",
            NOW_MS + 1,
            30_000,
        ))
        .unwrap();
    assert_eq!(after.service_principal, before.service_principal);
    assert_ne!(after.credential_id, before.credential_id);

    let failed = provider.rotate("").unwrap_err();
    assert_eq!(failed.code, WorkloadIdentityErrorCode::RotationFailed);
    assert_eq!(failed.evidence.outcome, "rotation_failed");
    assert_eq!(
        failed.evidence.key_id.as_deref(),
        Some(after.key_id.as_str())
    );
}

#[test]
fn provider_host_tokens_and_network_coordinates_are_not_service_identity() {
    let provider = provider();
    let verification = WorkloadIdentityVerification::new(
        "service:billing",
        "sandbox-transport:billing-api",
        NOW_MS,
    );
    assert_eq!(
        provider
            .verify("provider-host-token", &verification)
            .unwrap_err()
            .code,
        WorkloadIdentityErrorCode::InvalidProof
    );

    for coordinate in [
        "127.0.0.1",
        "support.local",
        "process-42",
        "replica-7",
        "region-a",
        "failure-domain-a",
    ] {
        assert_eq!(
            provider
                .issue(WorkloadCredentialRequest::new(
                    coordinate,
                    "service:billing",
                    "sandbox-transport:billing-api",
                    NOW_MS,
                    30_000,
                ))
                .unwrap_err()
                .code,
            WorkloadIdentityErrorCode::InvalidRequest
        );
    }

    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "service:billing",
            "sandbox-transport:billing-api",
            NOW_MS,
            30_000,
        ))
        .unwrap();
    let authenticated = provider.verify(&credential.token, &verification).unwrap();
    assert_ne!(authenticated.service_principal, "127.0.0.1");
    assert_ne!(authenticated.service_principal, "support.local");
    assert_ne!(authenticated.service_principal, "replica-7");
    assert_ne!(authenticated.service_principal, "region-a");
}
