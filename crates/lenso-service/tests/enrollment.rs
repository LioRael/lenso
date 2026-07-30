use lenso_service::system_plane::{
    ENROLLMENT_OFFER_PROTOCOL, ENROLLMENT_RECEIPT_PROTOCOL, Ed25519EnrollmentSigner,
    Ed25519EnrollmentTrustStore, EnrollmentCapabilityGrant, EnrollmentContractIssueCode,
    EnrollmentOffer, EnrollmentPolicyGrant, EnrollmentReceipt, EnrollmentSignature,
    EnrollmentSignatureAlgorithm, enrollment_offer_digest, enrollment_offer_schema,
    enrollment_receipt_schema, sign_enrollment_offer, sign_enrollment_receipt,
    verify_enrollment_exchange, verify_enrollment_offer, verify_enrollment_receipt,
};
use std::collections::BTreeSet;

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn placeholder_signature() -> EnrollmentSignature {
    EnrollmentSignature {
        algorithm: EnrollmentSignatureAlgorithm::Ed25519,
        key_id: "pending".to_owned(),
        subject_digest: digest('0'),
        value: "pending".to_owned(),
    }
}

fn capability() -> EnrollmentCapabilityGrant {
    EnrollmentCapabilityGrant {
        contract_id: "lenso.system-plane.runtime-observability.v1".to_owned(),
        schema_digest: digest('a'),
        feature_ids: BTreeSet::from(["queue-summary".to_owned()]),
    }
}

fn policy() -> EnrollmentPolicyGrant {
    EnrollmentPolicyGrant {
        policy_id: "support-system-plane".to_owned(),
        policy_revision: "revision:1".to_owned(),
        policy_digest: digest('b'),
    }
}

fn signed_offer(signer: &Ed25519EnrollmentSigner) -> EnrollmentOffer {
    sign_enrollment_offer(
        EnrollmentOffer {
            protocol: String::new(),
            system_id: "customer-support".to_owned(),
            console_service_principal: "service:console".to_owned(),
            nonce: "nonce-0123456789abcdef".to_owned(),
            issued_at_unix_ms: 1_000,
            expires_at_unix_ms: 20_000,
            requested_capabilities: vec![capability()],
            requested_policy: policy(),
            signature: placeholder_signature(),
        },
        signer,
    )
    .unwrap()
}

#[test]
fn signed_offer_and_receipt_bind_both_service_principals_and_nonce() {
    let console_signer = Ed25519EnrollmentSigner::new("console-key-1", [7; 32]).unwrap();
    let service_signer = Ed25519EnrollmentSigner::new("service-key-1", [9; 32]).unwrap();
    let console_trust =
        Ed25519EnrollmentTrustStore::new([("console-key-1", console_signer.verifying_key_bytes())])
            .unwrap();
    let service_trust =
        Ed25519EnrollmentTrustStore::new([("service-key-1", service_signer.verifying_key_bytes())])
            .unwrap();
    let offer = signed_offer(&console_signer);
    assert_eq!(
        enrollment_offer_digest(&offer),
        "sha256:edb298d9438f8f4cb8b45f0dc6e6f8acb38b290163c84ed1a2dc8899f516a7f6"
    );
    let offer_digest = verify_enrollment_offer(&offer, &console_trust, 2_000).unwrap();
    assert_eq!(offer.protocol, ENROLLMENT_OFFER_PROTOCOL);
    assert_eq!(offer.signature.subject_digest, offer_digest);

    let receipt = sign_enrollment_receipt(
        EnrollmentReceipt {
            protocol: String::new(),
            offer_digest,
            system_id: offer.system_id.clone(),
            managed_service_id: "support".to_owned(),
            managed_service_principal: "service:support".to_owned(),
            managed_service_revision: "release:sha256:0123456789abcdef".to_owned(),
            console_service_principal: offer.console_service_principal.clone(),
            nonce: offer.nonce.clone(),
            issued_at_unix_ms: 2_000,
            expires_at_unix_ms: 18_000,
            grant_revision: 1,
            authorization_epoch: 0,
            granted_capabilities: vec![capability()],
            granted_policy: policy(),
            signature: placeholder_signature(),
        },
        &service_signer,
    )
    .unwrap();
    assert_eq!(receipt.protocol, ENROLLMENT_RECEIPT_PROTOCOL);
    assert!(verify_enrollment_receipt(&receipt, &offer, &service_trust, 3_000).is_ok());
    let exchange =
        verify_enrollment_exchange(&offer, &receipt, &console_trust, &service_trust, 3_000)
            .unwrap();
    assert_eq!(exchange.offer_digest, offer.signature.subject_digest);
    assert_eq!(exchange.receipt_digest, receipt.signature.subject_digest);
    assert_eq!(exchange.managed_service_id, "support");
    assert_eq!(exchange.managed_service_principal, "service:support");
    assert_eq!(exchange.grant_revision, 1);
    assert_eq!(exchange.authorization_epoch, 0);
    assert!(
        jsonschema::validator_for(&enrollment_offer_schema())
            .unwrap()
            .is_valid(&serde_json::to_value(&offer).unwrap())
    );
    assert!(
        jsonschema::validator_for(&enrollment_receipt_schema())
            .unwrap()
            .is_valid(&serde_json::to_value(&receipt).unwrap())
    );
    let mut widened = receipt.clone();
    widened.granted_capabilities[0]
        .feature_ids
        .insert("execution-payloads".to_owned());
    let widened = sign_enrollment_receipt(widened, &service_signer).unwrap();
    assert!(
        verify_enrollment_receipt(&widened, &offer, &service_trust, 3_000)
            .unwrap_err()
            .iter()
            .any(|issue| issue.code == EnrollmentContractIssueCode::InvalidCapability)
    );

    let mut rebound = receipt;
    rebound.console_service_principal = "service:attacker".to_owned();
    assert!(verify_enrollment_receipt(&rebound, &offer, &service_trust, 3_000).is_err());
}

#[test]
fn exchange_verification_rejects_an_offer_from_an_untrusted_console() {
    let trusted_console = Ed25519EnrollmentSigner::new("console-key-1", [7; 32]).unwrap();
    let attacker = Ed25519EnrollmentSigner::new("attacker-key-1", [8; 32]).unwrap();
    let service_signer = Ed25519EnrollmentSigner::new("service-key-1", [9; 32]).unwrap();
    let console_trust = Ed25519EnrollmentTrustStore::new([(
        "console-key-1",
        trusted_console.verifying_key_bytes(),
    )])
    .unwrap();
    let service_trust =
        Ed25519EnrollmentTrustStore::new([("service-key-1", service_signer.verifying_key_bytes())])
            .unwrap();
    let offer = signed_offer(&attacker);
    let receipt = sign_enrollment_receipt(
        EnrollmentReceipt {
            protocol: String::new(),
            offer_digest: enrollment_offer_digest(&offer),
            system_id: offer.system_id.clone(),
            managed_service_id: "support".to_owned(),
            managed_service_principal: "service:support".to_owned(),
            managed_service_revision: "release:sha256:0123456789abcdef".to_owned(),
            console_service_principal: offer.console_service_principal.clone(),
            nonce: offer.nonce.clone(),
            issued_at_unix_ms: 2_000,
            expires_at_unix_ms: 18_000,
            grant_revision: 1,
            authorization_epoch: 0,
            granted_capabilities: vec![capability()],
            granted_policy: policy(),
            signature: placeholder_signature(),
        },
        &service_signer,
    )
    .unwrap();

    assert!(verify_enrollment_receipt(&receipt, &offer, &service_trust, 3_000).is_ok());
    let issues =
        verify_enrollment_exchange(&offer, &receipt, &console_trust, &service_trust, 3_000)
            .unwrap_err();
    assert!(
        issues
            .iter()
            .any(|issue| issue.code == EnrollmentContractIssueCode::SignerUntrusted)
    );
}

#[test]
fn tampering_expiry_and_untrusted_keys_fail_closed() {
    let signer = Ed25519EnrollmentSigner::new("console-key-1", [7; 32]).unwrap();
    let offer = signed_offer(&signer);
    let empty_trust = Ed25519EnrollmentTrustStore::default();
    assert_eq!(
        verify_enrollment_offer(&offer, &empty_trust, 2_000).unwrap_err()[0].code,
        EnrollmentContractIssueCode::SignerUntrusted
    );
    let trust = Ed25519EnrollmentTrustStore::new([("console-key-1", signer.verifying_key_bytes())])
        .unwrap();
    assert!(
        verify_enrollment_offer(&offer, &trust, 21_000)
            .unwrap_err()
            .iter()
            .any(|issue| issue.code == EnrollmentContractIssueCode::InvalidLifetime)
    );
    let mut tampered = offer;
    tampered.expires_at_unix_ms += 1;
    assert_ne!(
        tampered.signature.subject_digest,
        enrollment_offer_digest(&tampered)
    );
    assert!(verify_enrollment_offer(&tampered, &trust, 2_000).is_err());
}

#[test]
fn enrollment_schemas_are_strict_and_pin_protocols() {
    let offer_schema = enrollment_offer_schema();
    let receipt_schema = enrollment_receipt_schema();
    assert_eq!(
        offer_schema["properties"]["protocol"]["const"],
        ENROLLMENT_OFFER_PROTOCOL
    );
    assert_eq!(
        receipt_schema["properties"]["protocol"]["const"],
        ENROLLMENT_RECEIPT_PROTOCOL
    );
    assert_eq!(offer_schema["additionalProperties"], false);
    assert_eq!(receipt_schema["additionalProperties"], false);
}
