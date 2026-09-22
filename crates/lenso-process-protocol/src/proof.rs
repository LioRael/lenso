use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{
    HandshakeIdentity, HandshakeParams, ProtocolError, authoring::InitializeParams,
    decode_base64url_32,
};

const HOST_PROOF_DOMAIN: &[u8] = b"lenso-process-host-v1";
const CHILD_PROOF_DOMAIN: &[u8] = b"lenso-process-child-v1";
const AUTHORING_HOST_PROOF_DOMAIN: &[u8] = b"lenso-authoring-host-v2";
const AUTHORING_CHILD_PROOF_DOMAIN: &[u8] = b"lenso-authoring-child-v2";
const AUTHORING_CALLBACK_PROOF_DOMAIN: &[u8] = b"lenso-authoring-callback-v2";
const MAX_SAFE_JSON_INTEGER: u64 = 9_007_199_254_740_991;

/// Returns RFC 8785-compatible canonical bytes for the restricted proof profile.
pub fn canonicalize_proof_value(value: &Value) -> Result<Vec<u8>, ProtocolError> {
    let mut output = Vec::new();
    write_canonical(value, &mut output)?;
    Ok(output)
}

/// Computes the raw SHA-256 digest bound by host and child handshake proofs.
pub fn handshake_params_digest(params: &HandshakeParams) -> Result<[u8; 32], ProtocolError> {
    #[derive(Serialize)]
    struct ProofPayload<'a> {
        identity: &'a HandshakeIdentity,
        host_nonce: &'a str,
    }

    params.identity.validate()?;
    decode_base64url_32("host_nonce", &params.host_nonce)?;
    let value = serde_json::to_value(ProofPayload {
        identity: &params.identity,
        host_nonce: &params.host_nonce,
    })
    .map_err(|error| ProtocolError::new(format!("cannot encode handshake proof: {error}")))?;
    let canonical = canonicalize_proof_value(&value)?;
    Ok(Sha256::digest(canonical).into())
}

/// Returns the exact bytes authenticated by `host_proof`.
#[must_use]
pub fn host_proof_message(handshake_digest: &[u8; 32]) -> Vec<u8> {
    proof_message(HOST_PROOF_DOMAIN, handshake_digest, None)
}

/// Returns the exact bytes authenticated by `child_proof`.
pub fn child_proof_message(
    handshake_digest: &[u8; 32],
    session: &str,
) -> Result<Vec<u8>, ProtocolError> {
    let session = decode_base64url_32("session", session)?;
    Ok(proof_message(
        CHILD_PROOF_DOMAIN,
        handshake_digest,
        Some(&session),
    ))
}

/// Values bound by an Authoring V2 initialization handshake.
#[derive(Clone, Copy, Debug)]
pub struct AuthoringHandshakeProofInput<'a> {
    /// Exact initialization admitted by the child.
    pub initialize: &'a InitializeParams,
    /// Exact loopback HTTP origin exposed by the Host callback listener.
    pub callback_origin: &'a str,
    /// Canonical 32-byte base64url Host nonce.
    pub host_nonce: &'a str,
}

/// Returns the canonical bytes hashed before authenticating Authoring V2 initialization.
pub fn authoring_handshake_proof_payload(
    input: AuthoringHandshakeProofInput<'_>,
) -> Result<Vec<u8>, ProtocolError> {
    #[derive(Serialize)]
    struct ProofPayload<'a> {
        initialize: &'a InitializeParams,
        callback_origin: &'a str,
        host_nonce: &'a str,
    }

    input.initialize.validate()?;
    validate_loopback_http_origin(input.callback_origin)?;
    decode_base64url_32("host_nonce", input.host_nonce)?;
    let value = serde_json::to_value(ProofPayload {
        initialize: input.initialize,
        callback_origin: input.callback_origin,
        host_nonce: input.host_nonce,
    })
    .map_err(|error| ProtocolError::new(format!("cannot encode authoring proof: {error}")))?;
    canonicalize_proof_value(&value)
}

/// Returns the exact bytes authenticated by the Authoring V2 Host proof.
#[must_use]
pub fn authoring_host_proof_message(handshake_digest: &[u8; 32]) -> Vec<u8> {
    proof_message(AUTHORING_HOST_PROOF_DOMAIN, handshake_digest, None)
}

/// Returns the exact bytes authenticated by the Authoring V2 child proof.
pub fn authoring_child_proof_message(
    handshake_digest: &[u8; 32],
    child_nonce: &str,
) -> Result<Vec<u8>, ProtocolError> {
    let child_nonce = decode_base64url_32("child_nonce", child_nonce)?;
    Ok(proof_message(
        AUTHORING_CHILD_PROOF_DOMAIN,
        handshake_digest,
        Some(&child_nonce),
    ))
}

/// Returns the exact bytes authenticating one child-to-Host callback request.
pub fn authoring_callback_proof_message(
    session: &str,
    method: &str,
    params: &Value,
) -> Result<Vec<u8>, ProtocolError> {
    if !matches!(
        method,
        "lenso.call"
            | "lenso.event.publish"
            | "lenso.stream.open"
            | "lenso.stream.send"
            | "lenso.stream.receive"
            | "lenso.stream.close_send"
            | "lenso.stream.cancel"
            | "lenso.settled"
    ) {
        return Err(ProtocolError::new("unsupported Authoring callback method"));
    }
    let session = decode_base64url_32("session", session)?;
    let canonical_params = canonicalize_proof_value(params)?;
    let mut message = Vec::with_capacity(
        AUTHORING_CALLBACK_PROOF_DOMAIN.len()
            + 1
            + session.len()
            + 1
            + method.len()
            + 1
            + canonical_params.len(),
    );
    message.extend_from_slice(AUTHORING_CALLBACK_PROOF_DOMAIN);
    message.push(0);
    message.extend_from_slice(&session);
    message.push(0);
    message.extend_from_slice(method.as_bytes());
    message.push(0);
    message.extend_from_slice(&canonical_params);
    Ok(message)
}

fn validate_loopback_http_origin(value: &str) -> Result<(), ProtocolError> {
    let port = value
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| value.strip_prefix("http://[::1]:"))
        .and_then(|value| value.strip_suffix('/'))
        .ok_or_else(|| {
            ProtocolError::new("callback_origin must be an exact loopback HTTP origin")
        })?;
    if !port.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || port.starts_with('0')
        || port
            .parse::<u16>()
            .ok()
            .as_ref()
            .is_none_or(|port| *port == 0)
    {
        return Err(ProtocolError::new(
            "callback_origin must be an exact loopback HTTP origin",
        ));
    }
    Ok(())
}

fn proof_message(domain: &[u8], digest: &[u8; 32], suffix: Option<&[u8; 32]>) -> Vec<u8> {
    let mut message = Vec::with_capacity(domain.len() + 1 + 32 + suffix.map_or(0, |_| 32));
    message.extend_from_slice(domain);
    message.push(0);
    message.extend_from_slice(digest);
    if let Some(suffix) = suffix {
        message.extend_from_slice(suffix);
    }
    message
}

fn write_canonical(value: &Value, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(true) => output.extend_from_slice(b"true"),
        Value::Bool(false) => output.extend_from_slice(b"false"),
        Value::String(value) => output.extend_from_slice(
            serde_json::to_string(value)
                .map_err(|error| ProtocolError::new(error.to_string()))?
                .as_bytes(),
        ),
        Value::Number(number) => {
            let encoded = if let Some(value) = number.as_u64() {
                if value > MAX_SAFE_JSON_INTEGER {
                    return Err(ProtocolError::new(
                        "proof JSON integer exceeds the portable safe range",
                    ));
                }
                value.to_string()
            } else if let Some(value) = number.as_i64() {
                if value.unsigned_abs() > MAX_SAFE_JSON_INTEGER {
                    return Err(ProtocolError::new(
                        "proof JSON integer exceeds the portable safe range",
                    ));
                }
                value.to_string()
            } else {
                return Err(ProtocolError::new(
                    "floating-point values are forbidden in proof JSON",
                ));
            };
            output.extend_from_slice(encoded.as_bytes());
        }
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
            output.push(b'{');
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .map_err(|error| ProtocolError::new(error.to_string()))?
                        .as_bytes(),
                );
                output.push(b':');
                write_canonical(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}
