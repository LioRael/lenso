use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use lenso_process_protocol::{
    AuthoringHandshakeProofInput, authoring::*, authoring_callback_proof_message,
    authoring_child_proof_message, authoring_handshake_proof_payload, authoring_host_proof_message,
    decode_strict,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

fn fixture() -> Value {
    decode_strict(include_bytes!(
        "../../../fixtures/process-protocol/authoring-v2-conformance.json"
    ))
    .expect("shared Authoring V2 fixture must be strict JSON")
}

fn field<T: DeserializeOwned>(fixture: &Value, name: &str) -> T {
    serde_json::from_value(fixture[name].clone()).expect("fixture value must decode")
}

#[test]
fn shared_authoring_v2_values_validate() {
    let values = fixture();
    let initialize: InitializeParams = field(&values, "initialize");
    initialize.validate().unwrap();
    initialize
        .validate_for_runtime_profile("lenso.process-stdio@2")
        .unwrap();
    initialize.validate_initialized(&initialize).unwrap();
    let identity = &initialize.identity;

    let construct: ConstructParams = field(&values, "construct");
    construct.validate_for(identity).unwrap();
    field::<ConstructedResult>(&values, "constructed")
        .validate_for(&construct)
        .unwrap();

    let invoke: InvokeParams = field(&values, "invoke");
    invoke.validate_for(identity).unwrap();
    invoke.validate_against(&initialize).unwrap();
    field::<InvocationResult>(&values, "result")
        .validate_for(&invoke)
        .unwrap();
    let outbound = field::<OutboundCallParams>(&values, "outbound_call");
    outbound
        .validate_against(&initialize, &invoke.scope, true)
        .unwrap();
    assert_eq!(
        field::<OutboundCallResult>(&values, "result")
            .validate_for_outbound(&outbound)
            .unwrap_err()
            .detail(),
        "response identity mismatch"
    );

    let cancel: CancelParams = field(&values, "cancel");
    cancel.validate_for(identity).unwrap();
    field::<CancelAck>(&values, "cancel_ack")
        .validate_for(&cancel)
        .unwrap();
    field::<Settlement>(&values, "settlement")
        .validate_for(identity)
        .unwrap();
    let stop: StopParams = field(&values, "stop");
    stop.validate_for(identity).unwrap();
    field::<StoppedResult>(&values, "stopped")
        .validate_for(&stop)
        .unwrap();
}

#[test]
fn authoring_v2_proof_bytes_bind_initialize_callback_and_session() {
    let mut initialize: InitializeParams = field(&fixture(), "initialize");
    initialize.identity.session = URL_SAFE_NO_PAD.encode([3_u8; 32]);
    let host_nonce = URL_SAFE_NO_PAD.encode([1_u8; 32]);
    let child_nonce = URL_SAFE_NO_PAD.encode([2_u8; 32]);
    let payload = authoring_handshake_proof_payload(AuthoringHandshakeProofInput {
        initialize: &initialize,
        callback_origin: "http://127.0.0.1:31001/",
        host_nonce: &host_nonce,
    })
    .unwrap();
    let digest: [u8; 32] = Sha256::digest(&payload).into();
    assert_eq!(
        digest,
        [
            0x9f, 0x33, 0xc7, 0xec, 0xaa, 0x83, 0xba, 0x6d, 0x2e, 0x91, 0x74, 0xd8, 0xdd, 0xa1,
            0x9f, 0x18, 0x1f, 0xe1, 0xb1, 0x0f, 0x68, 0x83, 0xb6, 0x98, 0x60, 0x62, 0x52, 0x26,
            0x4b, 0xd9, 0x74, 0xd7,
        ]
    );

    assert_eq!(
        authoring_host_proof_message(&digest),
        [b"lenso-authoring-host-v2\0".as_slice(), digest.as_slice()].concat()
    );
    assert_eq!(
        authoring_child_proof_message(&digest, &child_nonce).unwrap(),
        [
            b"lenso-authoring-child-v2\0".as_slice(),
            digest.as_slice(),
            &[2_u8; 32],
        ]
        .concat()
    );
    let callback = authoring_callback_proof_message(
        &initialize.identity.session,
        "lenso.call",
        &serde_json::json!({"route_id": "route-1", "correlation_id": "1"}),
    )
    .unwrap();
    assert_eq!(
        callback,
        [
            b"lenso-authoring-callback-v2\0".as_slice(),
            &[3_u8; 32],
            b"\0lenso.call\0{\"correlation_id\":\"1\",\"route_id\":\"route-1\"}",
        ]
        .concat()
    );

    assert!(
        authoring_handshake_proof_payload(AuthoringHandshakeProofInput {
            initialize: &initialize,
            callback_origin: "http://example.com:31001/",
            host_nonce: &host_nonce,
        })
        .is_err()
    );
    assert!(
        authoring_handshake_proof_payload(AuthoringHandshakeProofInput {
            initialize: &initialize,
            callback_origin: "http://127.0.0.1:0/",
            host_nonce: &host_nonce,
        })
        .is_err()
    );
    assert!(
        authoring_handshake_proof_payload(AuthoringHandshakeProofInput {
            initialize: &initialize,
            callback_origin: "http://127.0.0.1:80/",
            host_nonce: &host_nonce,
        })
        .is_ok()
    );
    assert!(
        authoring_callback_proof_message(
            &initialize.identity.session,
            "lenso.unknown",
            &Value::Null,
        )
        .is_err()
    );
}

#[test]
fn named_requirements_reject_foreign_and_wrong_descriptor_routes() {
    let values = fixture();
    let mut initialize: InitializeParams = field(&values, "initialize");
    initialize.routes[0].requirement_id = "missing".to_owned();
    assert_eq!(
        initialize.validate().unwrap_err().detail(),
        "route references an unknown requirement_id"
    );

    let mut initialize: InitializeParams = field(&values, "initialize");
    initialize.routes[0].descriptor_version = "2.0.0".to_owned();
    assert_eq!(
        initialize.validate().unwrap_err().detail(),
        "route descriptor does not match its named requirement"
    );
}

#[test]
fn scopes_preserve_parent_authority_and_nonincreasing_budget() {
    let values = fixture();
    let parent = field::<InvokeParams>(&values, "invoke").scope;
    let mut child = field::<OutboundCallParams>(&values, "outbound_call").scope;
    child.remaining_budget_nanos = "900001".to_owned();
    assert_eq!(
        child.validate_child_of(&parent).unwrap_err().detail(),
        "outbound remaining budget may not increase"
    );
    child.remaining_budget_nanos = "800000".to_owned();
    child.permissions.clear();
    assert_eq!(
        child.validate_child_of(&parent).unwrap_err().detail(),
        "outbound scope must preserve permissions and extensions"
    );
}

#[test]
fn closed_parent_and_cross_requirement_routes_reject_before_dispatch() {
    let values = fixture();
    let initialize: InitializeParams = field(&values, "initialize");
    let invoke: InvokeParams = field(&values, "invoke");
    let mut call: OutboundCallParams = field(&values, "outbound_call");
    assert_eq!(
        call.validate_against(&initialize, &invoke.scope, false)
            .unwrap_err()
            .detail(),
        "closed parent scope cannot start an outbound call"
    );
    call.requirement_id = "secondary_store".to_owned();
    assert_eq!(
        call.validate_against(&initialize, &invoke.scope, true)
            .unwrap_err()
            .detail(),
        "outbound route belongs to another requirement"
    );
}

#[test]
fn strict_decoder_rejects_duplicate_and_unknown_fields() {
    assert!(decode_strict::<InitializeParams>(br#"{"api_version":2,"api_version":2}"#).is_err());
    let values = fixture();
    let mut initialize = values["initialize"].clone();
    initialize
        .as_object_mut()
        .unwrap()
        .insert("unknown".to_owned(), Value::Bool(true));
    let wire = serde_json::to_vec(&initialize).unwrap();
    assert!(decode_strict::<InitializeParams>(&wire).is_err());
    assert_eq!(
        decode_frame::<Value>(&vec![b' '; 1_048_577], DEFAULT_MAX_FRAME_BYTES)
            .unwrap_err()
            .detail(),
        "Authoring frame exceeds max_frame_bytes"
    );
}

#[test]
fn runtime_failures_are_structured_and_strict() {
    let values = fixture();
    let invoke: InvokeParams = field(&values, "invoke");
    let mut result = field::<InvocationResult>(&values, "result");
    result.outcome = InvocationOutcome::Runtime {
        failure: RuntimeFailure::ResourceExhausted {
            capability: "example.store@1".to_owned(),
            operation: "get".to_owned(),
        },
    };
    result.validate_for(&invoke).unwrap();

    let malformed = br#"{
        "session":"session-1",
        "correlation_id":"1",
        "outcome":{
            "kind":"runtime",
            "failure":{"kind":"cancelled","request_id":"1","detail":"extra"}
        }
    }"#;
    assert!(decode_strict::<InvocationResult>(malformed).is_err());
}

#[test]
fn exact_initialized_echo_rejects_session_and_profile_changes() {
    let values = fixture();
    let initialize: InitializeParams = field(&values, "initialize");
    let mut altered = initialize.clone();
    altered.identity.session = "session-2".to_owned();
    assert_eq!(
        initialize
            .validate_initialized(&altered)
            .unwrap_err()
            .detail(),
        "initialized result does not exactly echo initialization"
    );
    altered = initialize.clone();
    altered.identity.runtime_profile = "lenso.bun-authoring@2".to_owned();
    assert_eq!(
        altered
            .validate_for_runtime_profile("lenso.process-stdio@2")
            .unwrap_err()
            .detail(),
        "runtime profile does not match the selected Adapter profile"
    );
    assert_eq!(
        initialize
            .validate_initialized(&altered)
            .unwrap_err()
            .detail(),
        "initialized result does not exactly echo initialization"
    );
}

#[test]
fn event_and_stream_values_preserve_endpoint_route_and_session_identity() {
    let values = fixture();
    let initialize: InitializeParams = field(&values, "initialize");
    let invoke: InvokeParams = field(&values, "invoke");
    let outbound: OutboundCallParams = field(&values, "outbound_call");

    let event = EventPublishParams {
        session: invoke.session.clone(),
        correlation_id: invoke.correlation_id.clone(),
        endpoint_id: invoke.endpoint_id.clone(),
        capability_id: invoke.capability_id.clone(),
        descriptor_version: invoke.descriptor_version.clone(),
        descriptor_digest: invoke.descriptor_digest.clone(),
        operation: "changed".to_owned(),
        scope: invoke.scope.clone(),
        event: serde_json::json!({"key": "a"}),
    };
    event.validate_against(&initialize).unwrap();
    EventPublishResult {
        session: event.session.clone(),
        correlation_id: event.correlation_id.clone(),
        outcome: EventPublishOutcome::Accepted,
    }
    .validate_for(&event)
    .unwrap();

    let outbound_event = OutboundEventPublishParams {
        session: outbound.session.clone(),
        correlation_id: outbound.correlation_id.clone(),
        requirement_id: outbound.requirement_id.clone(),
        route_id: outbound.route_id.clone(),
        operation: "changed".to_owned(),
        scope: outbound.scope.clone(),
        event: serde_json::json!({"key": "a"}),
    };
    outbound_event
        .validate_against(&initialize, &invoke.scope, true)
        .unwrap();

    let open = StreamOpenParams {
        session: invoke.session.clone(),
        correlation_id: "3".to_owned(),
        endpoint_id: invoke.endpoint_id.clone(),
        capability_id: invoke.capability_id.clone(),
        descriptor_version: invoke.descriptor_version.clone(),
        descriptor_digest: invoke.descriptor_digest.clone(),
        operation: "watch".to_owned(),
        scope: invoke.scope.clone(),
        request: serde_json::json!({"key": "a"}),
    };
    open.validate_against(&initialize).unwrap();
    StreamOpenResult {
        session: open.session.clone(),
        correlation_id: open.correlation_id.clone(),
        outcome: StreamOpenOutcome::Opened {
            stream_id: "1".to_owned(),
        },
    }
    .validate_for(&open)
    .unwrap();

    let send = StreamSendParams {
        session: invoke.session.clone(),
        correlation_id: "4".to_owned(),
        stream_id: "1".to_owned(),
        sequence: "0".to_owned(),
        message: serde_json::json!({"text": "hello"}),
    };
    send.validate_for(&initialize.identity).unwrap();
    StreamActionResult {
        session: send.session.clone(),
        correlation_id: send.correlation_id.clone(),
        stream_id: send.stream_id.clone(),
        outcome: StreamActionOutcome::Accepted,
    }
    .validate_for_send(&send)
    .unwrap();

    let receive = StreamReceiveParams {
        session: invoke.session,
        correlation_id: "5".to_owned(),
        stream_id: "1".to_owned(),
    };
    receive.validate_for(&initialize.identity).unwrap();
    StreamReceiveResult {
        session: receive.session.clone(),
        correlation_id: receive.correlation_id.clone(),
        stream_id: receive.stream_id.clone(),
        outcome: StreamReceiveOutcome::Terminal {
            outcome: StreamTerminalOutcome::Success,
        },
    }
    .validate_for(&receive)
    .unwrap();
}

#[test]
fn stream_results_reject_cross_session_and_cross_stream_confusion() {
    let initialize: InitializeParams = field(&fixture(), "initialize");
    let request = StreamReceiveParams {
        session: initialize.identity.session.clone(),
        correlation_id: "8".to_owned(),
        stream_id: "2".to_owned(),
    };
    let mut result = StreamReceiveResult {
        session: request.session.clone(),
        correlation_id: request.correlation_id.clone(),
        stream_id: "3".to_owned(),
        outcome: StreamReceiveOutcome::PeerHalfClosed,
    };
    assert_eq!(
        result.validate_for(&request).unwrap_err().detail(),
        "stream result identity mismatch"
    );
    result.stream_id = request.stream_id.clone();
    result.session = "session-2".to_owned();
    assert_eq!(
        result.validate_for(&request).unwrap_err().detail(),
        "response identity mismatch"
    );
}
