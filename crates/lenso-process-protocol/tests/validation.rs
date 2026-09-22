use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac as _};
use lenso_process_protocol::*;
use serde_json::Value;
use sha2::Sha256;

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn identity() -> HandshakeIdentity {
    HandshakeIdentity {
        protocol_profile: PROCESS_PROFILE.to_owned(),
        value_profile: VALUE_PROFILE.to_owned(),
        plugin_instance: "tool-provider".to_owned(),
        plugin_generation: "7".to_owned(),
        generation_spec_digest: digest('a'),
        artifact_digest: digest('b'),
        effective_host_grant_set_digest: digest('c'),
        interaction_profiles: vec![PROVIDE_REQUEST_PROFILE.to_owned()],
        provided_capabilities: vec![CapabilityDescriptor {
            capability_id: "lenso.agent.tool-provider@1".to_owned(),
            descriptor_version: "1.0.0".to_owned(),
            descriptor_digest: digest('d'),
            operations: vec![OperationDescriptor {
                operation: "catalog".to_owned(),
                interaction: InteractionKind::Request,
            }],
        }],
        outbound_bindings: Vec::new(),
        peer_limits: PeerLimits::v1_defaults(),
    }
}

#[test]
fn strict_decode_rejects_unknown_and_duplicate_fields() {
    assert!(decode_strict::<ShutdownParams>(br#"{"session":"a","extra":true}"#).is_err());
    assert!(decode_strict::<Value>(br#"{"payload":{"key":1,"key":2}}"#).is_err());
}

#[test]
fn readiness_requires_exact_profile_and_distinct_ports() {
    let mut record = ReadinessRecord {
        protocol: PROCESS_PROFILE.to_owned(),
        data_port: 31_001,
        control_port: 31_002,
    };
    record.validate().unwrap();
    record.control_port = record.data_port;
    assert!(record.validate().is_err());
}

#[test]
fn identity_requires_canonical_order_and_exact_profiles() {
    let mut duplicate_profiles = identity();
    duplicate_profiles.validate().unwrap();
    duplicate_profiles.interaction_profiles = vec![
        PROVIDE_REQUEST_PROFILE.to_owned(),
        PROVIDE_REQUEST_PROFILE.to_owned(),
    ];
    assert!(duplicate_profiles.validate().is_err());

    let mut unsupported_profile = identity();
    unsupported_profile
        .interaction_profiles
        .push("stream-v1".to_owned());
    assert!(unsupported_profile.validate().is_err());

    let mut outbound_without_profile = identity();
    outbound_without_profile
        .outbound_bindings
        .push(OutboundBindingDescriptor {
            binding_id: "outbound".to_owned(),
            capability_id: "example.outbound@1".to_owned(),
            descriptor_version: "1.0.0".to_owned(),
            descriptor_digest: digest('e'),
            provider_instance: "provider".to_owned(),
        });
    assert!(outbound_without_profile.validate().is_err());
}

#[test]
fn json_rpc_error_surface_is_closed_and_bounded() {
    let valid = JsonRpcError {
        jsonrpc: "2.0".to_owned(),
        id: Some("7".to_owned()),
        error: JsonRpcErrorObject {
            code: -32602,
            message: "Invalid params".to_owned(),
        },
    };
    valid.validate().unwrap();
    let mut invalid = valid;
    invalid.error.code = -32_000;
    assert!(invalid.validate().is_err());
}

#[test]
fn proof_canonicalization_uses_utf16_key_order_and_rejects_floats() {
    let value = serde_json::json!({"\u{e000}": 2, "\u{1f600}": 1});
    let canonical = canonicalize_proof_value(&value).unwrap();
    assert_eq!(String::from_utf8(canonical).unwrap(), "{\"😀\":1,\"\":2}");
    assert!(canonicalize_proof_value(&serde_json::json!(1.5)).is_err());
}

#[test]
fn proof_messages_are_byte_framed_and_hmac_stable() {
    let params = HandshakeParams {
        identity: identity(),
        host_nonce: URL_SAFE_NO_PAD.encode([2_u8; 32]),
        host_proof: URL_SAFE_NO_PAD.encode([0_u8; 32]),
    };
    let digest = handshake_params_digest(&params).unwrap();
    let host_message = host_proof_message(&digest);
    let child_message = child_proof_message(&digest, &URL_SAFE_NO_PAD.encode([3_u8; 32])).unwrap();
    assert_eq!(host_message.len(), "lenso-process-host-v1".len() + 1 + 32);
    assert_eq!(child_message.len(), "lenso-process-child-v1".len() + 1 + 64);

    let mut host_mac = Hmac::<Sha256>::new_from_slice(&[1_u8; 32]).unwrap();
    host_mac.update(&host_message);
    let mut child_mac = Hmac::<Sha256>::new_from_slice(&[1_u8; 32]).unwrap();
    child_mac.update(&child_message);
    assert_ne!(
        host_mac.finalize().into_bytes(),
        child_mac.finalize().into_bytes()
    );
}

#[test]
fn child_failure_surface_excludes_host_authoritative_failures() {
    let wire = br#"{"session":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","correlation_id":"1","outcome":{"kind":"runtime","failure":{"kind":"deadline_exceeded"}}}"#;
    assert!(decode_strict::<RequestResult>(wire).is_err());
}
