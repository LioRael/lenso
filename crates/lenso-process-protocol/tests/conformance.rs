use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac as _};
use lenso_process_protocol::{
    HandshakeIdentity, HandshakeParams, child_proof_message, decode_strict,
    handshake_params_digest, host_proof_message,
};
use serde::Deserialize;
use sha2::Sha256;
use std::fmt::Write as _;

#[derive(Deserialize)]
struct Fixture {
    secret_hex: String,
    identity: HandshakeIdentity,
    host_nonce: String,
    session: String,
    canonical_payload: String,
    handshake_digest_hex: String,
    host_message_hex: String,
    child_message_hex: String,
    host_proof: String,
    child_proof: String,
}

#[derive(Deserialize)]
struct StrictJsonVector {
    name: String,
    wire: String,
    accepted: bool,
    forbidden_error_fragment: Option<String>,
}

#[test]
fn rust_matches_process_protocol_v1_proof_vectors() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/process-protocol/conformance.json"
    ))
    .unwrap();
    fixture.identity.validate().unwrap();
    let params = HandshakeParams {
        identity: fixture.identity,
        host_nonce: fixture.host_nonce,
        host_proof: URL_SAFE_NO_PAD.encode([0_u8; 32]),
    };
    let payload = lenso_process_protocol::canonicalize_proof_value(&serde_json::json!({
        "identity": &params.identity,
        "host_nonce": &params.host_nonce,
    }))
    .unwrap();
    let digest = handshake_params_digest(&params).unwrap();
    let host_message = host_proof_message(&digest);
    let child_message = child_proof_message(&digest, &fixture.session).unwrap();
    let secret = decode_hex(&fixture.secret_hex);
    let proof = |message: &[u8]| {
        let mut mac = Hmac::<Sha256>::new_from_slice(&secret).unwrap();
        mac.update(message);
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    };

    assert_eq!(
        String::from_utf8(payload).unwrap(),
        fixture.canonical_payload
    );
    assert_eq!(encode_hex(&digest), fixture.handshake_digest_hex);
    assert_eq!(encode_hex(&host_message), fixture.host_message_hex);
    assert_eq!(encode_hex(&child_message), fixture.child_message_hex);
    assert_eq!(proof(&host_message), fixture.host_proof);
    assert_eq!(proof(&child_message), fixture.child_proof);
}

#[test]
fn rust_matches_the_shared_strict_json_conformance_corpus() {
    let vectors: Vec<StrictJsonVector> = serde_json::from_str(include_str!(
        "../../../fixtures/process-protocol/strict-json-conformance.json"
    ))
    .expect("the shared strict JSON corpus should be valid JSON");

    for vector in vectors {
        let result = decode_strict::<serde_json::Value>(vector.wire.as_bytes());
        assert_eq!(result.is_ok(), vector.accepted, "{}", vector.name);
        if let Err(error) = result {
            let message = error.to_string();
            assert_eq!(message, "invalid strict JSON", "{}", vector.name);
            if let Some(fragment) = vector.forbidden_error_fragment {
                assert!(!message.contains(&fragment), "{}", vector.name);
            }
        }
    }
}

fn decode_hex(value: &str) -> Vec<u8> {
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty());
    pairs
        .iter()
        .map(|pair| {
            let text = std::str::from_utf8(pair).unwrap();
            u8::from_str_radix(text, 16).unwrap()
        })
        .collect()
}

fn encode_hex(value: &[u8]) -> String {
    value.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").unwrap();
        output
    })
}
