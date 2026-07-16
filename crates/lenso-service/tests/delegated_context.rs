use lenso_service::{
    DelegatedActorCredentialRequest, DelegatedContextErrorCode, DelegatedContextProvider,
    ServiceContextPolicy, ServiceTenancyMode, SystemSandboxDelegatedContextProvider,
    TenantCredentialRequest,
};

fn provider() -> SystemSandboxDelegatedContextProvider {
    SystemSandboxDelegatedContextProvider::new("local", "delegated-context-secret").unwrap()
}

fn actor(provider: &SystemSandboxDelegatedContextProvider) -> lenso_service::DelegatedActorContext {
    provider
        .issue_actor(DelegatedActorCredentialRequest::new(
            "user_01",
            "service:support",
            "support.ticket.update",
            ["support.tickets.read", "support.tickets.update"],
            1_000,
            30_000,
        ))
        .unwrap()
}

fn tenant(provider: &SystemSandboxDelegatedContextProvider) -> lenso_service::TenantContext {
    provider
        .issue_tenant(TenantCredentialRequest::new(
            "tenant_01",
            "user_01",
            "delegation_1",
            "service:support",
            1_000,
            30_000,
        ))
        .unwrap()
}

fn policy(mode: ServiceTenancyMode) -> ServiceContextPolicy {
    ServiceContextPolicy::new(
        "service:support",
        "support.ticket.update",
        ["support.tickets.update"],
        ["support.tickets.read", "support.tickets.update"],
        mode,
    )
}

#[test]
fn valid_delegated_actor_and_required_tenant_are_verified() {
    let provider = provider();
    let actor = actor(&provider);
    let tenant = tenant(&provider);

    let admitted = policy(ServiceTenancyMode::Required)
        .verify(&provider, Some(&actor), Some(&tenant), 2_000)
        .unwrap();

    assert_eq!(admitted.actor.subject, "user_01");
    assert_eq!(admitted.actor.intent, "support.ticket.update");
    assert_eq!(admitted.tenant.unwrap().tenant_id, "tenant_01");
    assert_eq!(admitted.evidence.outcome, "identity_context_accepted");
    assert_eq!(
        admitted.evidence.delegation_id.as_deref(),
        Some("delegation_1")
    );
    assert_eq!(
        admitted.evidence.tenant_claim_id.as_deref(),
        Some("tenant_claim_1")
    );
}

#[test]
fn delegation_rejects_wrong_audience_overbroad_permission_and_wrong_intent() {
    let provider = provider();
    let tenant = tenant(&provider);

    let wrong_audience = provider
        .issue_actor(DelegatedActorCredentialRequest::new(
            "user_01",
            "service:billing",
            "support.ticket.update",
            ["support.tickets.update"],
            1_000,
            30_000,
        ))
        .unwrap();
    let overbroad = provider
        .issue_actor(DelegatedActorCredentialRequest::new(
            "user_01",
            "service:support",
            "support.ticket.update",
            ["support.tickets.update", "support.admin"],
            1_000,
            30_000,
        ))
        .unwrap();
    let wrong_intent = provider
        .issue_actor(DelegatedActorCredentialRequest::new(
            "user_01",
            "service:support",
            "support.ticket.delete",
            ["support.tickets.update"],
            1_000,
            30_000,
        ))
        .unwrap();

    for (actor, code) in [
        (wrong_audience, DelegatedContextErrorCode::AudienceMismatch),
        (overbroad, DelegatedContextErrorCode::OverbroadPermissions),
        (wrong_intent, DelegatedContextErrorCode::IntentMismatch),
    ] {
        assert_eq!(
            policy(ServiceTenancyMode::Required)
                .verify(&provider, Some(&actor), Some(&tenant), 2_000)
                .unwrap_err()
                .code,
            code
        );
    }
}

#[test]
fn tenancy_modes_require_preserve_or_disallow_explicit_scope() {
    let provider = provider();
    let actor = actor(&provider);
    let tenant = tenant(&provider);

    assert_eq!(
        policy(ServiceTenancyMode::Required)
            .verify(&provider, Some(&actor), None, 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::TenantRequired
    );
    assert_eq!(
        policy(ServiceTenancyMode::Optional)
            .verify(&provider, Some(&actor), Some(&tenant), 2_000)
            .unwrap()
            .tenant
            .unwrap()
            .tenant_id,
        "tenant_01"
    );
    assert!(
        policy(ServiceTenancyMode::Optional)
            .verify(&provider, Some(&actor), None, 2_000)
            .unwrap()
            .tenant
            .is_none()
    );
    assert_eq!(
        policy(ServiceTenancyMode::None)
            .verify(&provider, Some(&actor), Some(&tenant), 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::TenantIncompatible
    );
    assert!(
        policy(ServiceTenancyMode::None)
            .verify(&provider, Some(&actor), None, 2_000)
            .unwrap()
            .tenant
            .is_none()
    );
}

#[test]
fn tenant_context_cannot_be_spliced_across_actor_delegations() {
    let provider = provider();
    let actor = actor(&provider);
    let tenant = provider
        .issue_tenant(TenantCredentialRequest::new(
            "tenant_01",
            "user_02",
            "delegation_1",
            "service:support",
            1_000,
            30_000,
        ))
        .unwrap();

    assert_eq!(
        policy(ServiceTenancyMode::Required)
            .verify(&provider, Some(&actor), Some(&tenant), 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::TenantIncompatible
    );
}

#[test]
fn invalid_proof_expiry_and_missing_delegation_are_rejected_without_secrets_in_evidence() {
    let provider = provider();
    let tenant = tenant(&provider);
    let mut invalid = actor(&provider);
    invalid.proof.signature.push_str("tampered");

    for (actor, now, code) in [
        (None, 2_000, DelegatedContextErrorCode::DelegationRequired),
        (
            Some(&invalid),
            2_000,
            DelegatedContextErrorCode::InvalidProof,
        ),
        (
            Some(&actor(&provider)),
            40_000,
            DelegatedContextErrorCode::CredentialExpired,
        ),
    ] {
        let error = policy(ServiceTenancyMode::Required)
            .verify(&provider, actor, Some(&tenant), now)
            .unwrap_err();
        assert_eq!(error.code, code);
        let evidence = serde_json::to_string(&error.evidence).unwrap();
        assert!(!evidence.contains("eyJ"));
        assert!(!evidence.contains("delegated-context-secret"));
    }
}
