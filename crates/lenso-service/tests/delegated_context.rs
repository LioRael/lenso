use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer as _, SigningKey};
use lenso_service::{
    ContextClaimProof, DelegatedActorContext, DelegatedActorCredentialRequest,
    DelegatedContextErrorCode, DelegatedContextIssuer, DelegatedContextVerifier,
    Ed25519DelegatedContextVerifier, ServiceContextPolicy, ServiceTenancyMode,
    SystemSandboxDelegatedContextProvider, TenantContext, TenantCredentialRequest,
    delegated_actor_signing_bytes, tenant_context_signing_bytes,
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

fn public_key(key: &SigningKey) -> String {
    URL_SAFE_NO_PAD.encode(key.verifying_key().to_bytes())
}

fn ed25519_actor(key: &SigningKey, method: &str, delegation_id: &str) -> DelegatedActorContext {
    let mut context = DelegatedActorContext {
        issuer: "console:acme".to_owned(),
        subject: "operator:alice".to_owned(),
        audiences: vec!["service:support".to_owned()],
        intent: "support.ticket.update".to_owned(),
        permissions: vec!["support.tickets.update".to_owned()],
        expires_at_unix_ms: 31_000,
        delegation_id: delegation_id.to_owned(),
        proof: ContextClaimProof {
            verification_method: method.to_owned(),
            algorithm: "Ed25519".to_owned(),
            signature: String::new(),
        },
    };
    context.proof.signature = URL_SAFE_NO_PAD.encode(
        key.sign(&delegated_actor_signing_bytes(&context).unwrap())
            .to_bytes(),
    );
    context
}

fn ed25519_tenant(key: &SigningKey, method: &str, delegation_id: &str) -> TenantContext {
    let mut context = TenantContext {
        issuer: "console:acme".to_owned(),
        tenant_id: "tenant_01".to_owned(),
        actor_subject: "operator:alice".to_owned(),
        delegation_id: delegation_id.to_owned(),
        audiences: vec!["service:support".to_owned()],
        expires_at_unix_ms: 30_000,
        claim_id: "tenant-claim-1".to_owned(),
        proof: ContextClaimProof {
            verification_method: method.to_owned(),
            algorithm: "Ed25519".to_owned(),
            signature: String::new(),
        },
    };
    context.proof.signature = URL_SAFE_NO_PAD.encode(
        key.sign(&tenant_context_signing_bytes(&context).unwrap())
            .to_bytes(),
    );
    context
}

#[test]
fn production_verifier_accepts_overlapping_keys_then_rejects_the_retired_key() {
    let old = SigningKey::from_bytes(&[7; 32]);
    let active = SigningKey::from_bytes(&[9; 32]);
    let rotating = Ed25519DelegatedContextVerifier::from_base64_public_keys([
        ("console:acme", "console-key-old", public_key(&old)),
        ("console:acme", "console-key-active", public_key(&active)),
    ])
    .unwrap();
    let old_actor = ed25519_actor(&old, "console-key-old", "delegation-old");
    let active_actor = ed25519_actor(&active, "console-key-active", "delegation-active");
    rotating
        .verify_actor(&old_actor, "service:support", 2_000)
        .unwrap();
    rotating
        .verify_actor(&active_actor, "service:support", 2_000)
        .unwrap();

    let after_rotation = Ed25519DelegatedContextVerifier::from_base64_public_keys([(
        "console:acme",
        "console-key-active",
        public_key(&active),
    )])
    .unwrap();
    assert_eq!(
        after_rotation
            .verify_actor(&old_actor, "service:support", 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::InvalidProof
    );
    after_rotation
        .verify_actor(&active_actor, "service:support", 2_000)
        .unwrap();
}

#[test]
fn production_verifier_binds_every_actor_and_tenant_field_and_domain() {
    let key = SigningKey::from_bytes(&[11; 32]);
    let verifier = Ed25519DelegatedContextVerifier::from_base64_public_keys([(
        "console:acme",
        "console-key-1",
        public_key(&key),
    )])
    .unwrap();
    let actor = ed25519_actor(&key, "console-key-1", "delegation-1");
    let tenant = ed25519_tenant(&key, "console-key-1", "delegation-1");
    let admitted = policy(ServiceTenancyMode::Required)
        .verify(&verifier, Some(&actor), Some(&tenant), 2_000)
        .unwrap();
    assert_eq!(admitted.actor.subject, "operator:alice");
    assert_eq!(admitted.tenant.unwrap().tenant_id, "tenant_01");

    let mut altered = actor.clone();
    altered.permissions.push("support.admin".to_owned());
    assert_eq!(
        verifier
            .verify_actor(&altered, "service:support", 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::InvalidProof
    );

    let mut cross_domain = tenant;
    cross_domain.proof.signature = actor.proof.signature;
    assert_eq!(
        verifier
            .verify_tenant(&cross_domain, "service:support", 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::InvalidProof
    );
}

#[test]
fn production_signing_payloads_are_stable_and_domain_separated() {
    let key = SigningKey::from_bytes(&[13; 32]);
    let actor = ed25519_actor(&key, "console-key-1", "delegation-1");
    let tenant = ed25519_tenant(&key, "console-key-1", "delegation-1");

    assert_eq!(
        String::from_utf8(delegated_actor_signing_bytes(&actor).unwrap()).unwrap(),
        r#"{"audiences":["service:support"],"delegationId":"delegation-1","expiresAtUnixMs":31000,"intent":"support.ticket.update","issuer":"console:acme","permissions":["support.tickets.update"],"protocol":"lenso.delegated-actor-context.ed25519.v1","subject":"operator:alice"}"#
    );
    assert_eq!(
        String::from_utf8(tenant_context_signing_bytes(&tenant).unwrap()).unwrap(),
        r#"{"actorSubject":"operator:alice","audiences":["service:support"],"claimId":"tenant-claim-1","delegationId":"delegation-1","expiresAtUnixMs":30000,"issuer":"console:acme","protocol":"lenso.tenant-context.ed25519.v1","tenantId":"tenant_01"}"#
    );
}

#[test]
fn production_verifier_checks_audience_and_expiry_after_valid_signature() {
    let key = SigningKey::from_bytes(&[17; 32]);
    let verifier = Ed25519DelegatedContextVerifier::from_base64_public_keys([(
        "console:acme",
        "console-key-1",
        public_key(&key),
    )])
    .unwrap();
    let actor = ed25519_actor(&key, "console-key-1", "delegation-1");

    assert_eq!(
        verifier
            .verify_actor(&actor, "service:billing", 2_000)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::AudienceMismatch
    );
    assert_eq!(
        verifier
            .verify_actor(&actor, "service:support", 31_001)
            .unwrap_err()
            .code,
        DelegatedContextErrorCode::CredentialExpired
    );
}
