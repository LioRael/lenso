use lenso_service::{
    CallPolicyEvent, CallPolicyFailure, CallPolicyRuntime, CallPolicyTerminalOutcome,
    DirectGrpcCallError, DirectGrpcClient, DirectGrpcServerPolicy, Endpoint, EndpointState,
    GrpcIdempotency, ManualCallPolicyClock, ServiceReference, StaticEndpointResolver,
    generate_direct_grpc_bindings,
    support_grpc_v1::{
        GetSlaRequest, ProbeSlaRequest, SlaResponse, UpdateSlaRequest,
        support_service_server::{SupportService, SupportServiceServer},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tonic::{Request, Response, Status};

fn bindings() -> lenso_service::DirectGrpcBindings {
    generate_direct_grpc_bindings(
        "support-grpc",
        "v1",
        lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
        lenso_service::DIRECT_GRPC_DESCRIPTOR_V1,
    )
    .unwrap()
}

#[test]
fn versioned_protobuf_generates_protocol_preserving_bindings() {
    let bindings = bindings();
    assert_eq!(bindings.contract_id, "support-grpc");
    assert_eq!(bindings.version, "v1");
    assert_eq!(
        bindings.service_name,
        "lenso.services.support.v1.SupportService"
    );
    assert_eq!(bindings.operations.len(), 3);
    assert_eq!(
        bindings.operation("GetSla").unwrap().idempotency,
        GrpcIdempotency::Idempotent
    );
    assert_eq!(
        bindings.operation("ProbeSla").unwrap().idempotency,
        GrpcIdempotency::Unknown
    );
    assert_eq!(
        bindings.operation("UpdateSla").unwrap().idempotency,
        GrpcIdempotency::RequiresKey
    );
    assert_eq!(
        bindings
            .operation("GetSla")
            .unwrap()
            .call_policy
            .concurrency
            .as_ref()
            .unwrap()
            .max_in_flight,
        2
    );
    assert_eq!(
        bindings
            .operation("GetSla")
            .unwrap()
            .call_policy
            .fallback
            .as_ref()
            .unwrap()
            .handler,
        "support.cached_sla"
    );
}

#[test]
fn invalid_grpc_call_policy_is_rejected_deterministically() {
    let source = r#"
        syntax = "proto3";
        package test;
        service Test {
          // lenso-call-policy: {"maxAttempts":2}
          rpc Probe(ProbeRequest) returns (ProbeResponse);
        }
        message ProbeRequest {}
        message ProbeResponse {}
    "#;
    assert_eq!(
        lenso_service::parse_protobuf_call_policies(source, [("Probe", GrpcIdempotency::Unknown)])
            .unwrap_err(),
        "rpc Probe lenso-call-policy.maxAttempts: unsafe_retry_policy"
    );
}

#[test]
fn generated_grpc_bindings_require_an_explicit_call_policy() {
    assert_eq!(
        lenso_service::parse_protobuf_call_policies(
            "rpc Probe(ProbeRequest) returns (ProbeResponse);",
            [("Probe", GrpcIdempotency::Idempotent)]
        )
        .unwrap_err(),
        "rpc Probe requires lenso-call-policy"
    );
}

#[test]
fn authoritative_descriptor_remains_compatible_with_contract_evaluation() {
    let canonical = lenso_service::canonicalize_protobuf_request_response(
        "v1",
        lenso_service::DIRECT_GRPC_DESCRIPTOR_V1,
    )
    .unwrap();
    assert_eq!(canonical["format"], "protobuf");
    for operation in ["GetSla", "ProbeSla", "UpdateSla"] {
        assert!(
            canonical["operations"]
                .get(format!(
                    "lenso.services.support.v1.SupportService.{operation}"
                ))
                .is_some()
        );
    }
}

#[derive(Clone)]
struct SupportFixture {
    attempts: Arc<AtomicUsize>,
    get_available: Arc<AtomicBool>,
    deadline_exceeded: Arc<AtomicBool>,
    admission: DirectGrpcServerPolicy,
}

#[tonic::async_trait]
impl SupportService for SupportFixture {
    async fn get_sla(
        &self,
        request: Request<GetSlaRequest>,
    ) -> Result<Response<SlaResponse>, Status> {
        let _admission = self
            .admission
            .admit("GetSla", &request)
            .map_err(|error| error.status())?;
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.deadline_exceeded.load(Ordering::SeqCst) {
            return Err(Status::deadline_exceeded("deadline elapsed in flight"));
        }
        if !self.get_available.load(Ordering::SeqCst) {
            return Err(Status::unavailable("sla unavailable"));
        }
        assert_eq!(
            request.metadata().get("x-lenso-deadline-unix-ms").unwrap(),
            "4102444800000"
        );
        let mut response = Response::new(SlaResponse {
            payload: request.into_inner().payload,
        });
        response
            .metadata_mut()
            .insert("x-support-version", "v1".parse().unwrap());
        Ok(response)
    }
    async fn update_sla(
        &self,
        request: Request<UpdateSlaRequest>,
    ) -> Result<Response<SlaResponse>, Status> {
        let _admission = self
            .admission
            .admit("UpdateSla", &request)
            .map_err(|error| error.status())?;
        assert_eq!(
            request.metadata().get("idempotency-key").unwrap(),
            "sla-42:update"
        );
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == 1 {
            return Err(Status::unavailable("try again"));
        }
        let mut status = Status::failed_precondition("stale revision");
        status
            .metadata_mut()
            .insert("x-support-revision", "41".parse().unwrap());
        Err(status)
    }
    async fn probe_sla(
        &self,
        _request: Request<ProbeSlaRequest>,
    ) -> Result<Response<SlaResponse>, Status> {
        let _admission = self
            .admission
            .admit("ProbeSla", &_request)
            .map_err(|error| error.status())?;
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Err(Status::unavailable("probe unavailable"))
    }
}

async fn client(attempts: Arc<AtomicUsize>) -> DirectGrpcClient<StaticEndpointResolver> {
    client_with_behavior(
        attempts,
        Arc::new(AtomicBool::new(true)),
        Arc::new(AtomicBool::new(false)),
    )
    .await
}

async fn client_with_availability(
    attempts: Arc<AtomicUsize>,
    get_available: Arc<AtomicBool>,
) -> DirectGrpcClient<StaticEndpointResolver> {
    client_with_behavior(attempts, get_available, Arc::new(AtomicBool::new(false))).await
}

async fn client_with_behavior(
    attempts: Arc<AtomicUsize>,
    get_available: Arc<AtomicBool>,
    deadline_exceeded: Arc<AtomicBool>,
) -> DirectGrpcClient<StaticEndpointResolver> {
    let generated_bindings = bindings();
    let admission = DirectGrpcServerPolicy::new(generated_bindings.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(SupportServiceServer::new(SupportFixture {
                attempts,
                get_available,
                deadline_exceeded,
                admission,
            }))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();
    DirectGrpcClient::new(resolver, generated_bindings)
}

#[test]
fn generated_grpc_server_policy_rejects_overload_before_business_handling() {
    let mut generated_bindings = bindings();
    generated_bindings
        .operations
        .iter_mut()
        .find(|operation| operation.operation_id == "GetSla")
        .unwrap()
        .call_policy
        .overload
        .as_mut()
        .unwrap()
        .max_in_flight = 1;
    let admission = DirectGrpcServerPolicy::new(generated_bindings);
    let request = grpc_request_with_context((), false);
    let _first = admission.admit("GetSla", &request).unwrap();
    let error = admission.admit("GetSla", &request).unwrap_err();
    assert_eq!(error.status().code(), tonic::Code::ResourceExhausted);
    let lenso_service::DirectGrpcAdmissionError::Overloaded { evidence } = error else {
        panic!("expected overload evidence")
    };
    assert_eq!(evidence.events, [CallPolicyEvent::OverloadRejected]);
    assert_eq!(
        evidence.terminal_outcome,
        CallPolicyTerminalOutcome::Rejected
    );
}

#[test]
fn generated_grpc_server_policy_rejects_deadline_and_missing_key() {
    let admission = DirectGrpcServerPolicy::new(bindings());
    let expired = grpc_request_with_context((), false);
    let mut expired = expired;
    expired
        .metadata_mut()
        .insert("x-lenso-deadline-unix-ms", "1".parse().unwrap());
    assert_eq!(
        admission
            .admit("GetSla", &expired)
            .unwrap_err()
            .status()
            .code(),
        tonic::Code::DeadlineExceeded
    );

    let missing_key = grpc_request_with_context((), false);
    assert_eq!(
        admission
            .admit("UpdateSla", &missing_key)
            .unwrap_err()
            .status()
            .code(),
        tonic::Code::InvalidArgument
    );
    let with_key = grpc_request_with_context((), true);
    assert!(admission.admit("UpdateSla", &with_key).is_ok());
}

fn grpc_request_with_context<T>(message: T, with_key: bool) -> Request<T> {
    let mut request = Request::new(message);
    request
        .metadata_mut()
        .insert("x-lenso-deadline-unix-ms", "4102444800000".parse().unwrap());
    if with_key {
        request
            .metadata_mut()
            .insert("idempotency-key", "test-key".parse().unwrap());
    }
    request
}

#[tokio::test]
async fn generated_client_applies_the_same_circuit_and_fallback_policy() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let available = Arc::new(AtomicBool::new(false));
    let clock = Arc::new(ManualCallPolicyClock::new(1_000));
    let client = client_with_availability(Arc::clone(&attempts), Arc::clone(&available))
        .await
        .with_policy_runtime(CallPolicyRuntime::new(clock.clone()))
        .with_fallback("support.cached_sla", |_| b"cached-sla".to_vec());

    for call_index in 0..2 {
        let error = client
            .get_sla(&ServiceReference::new("support"), vec![], 4_102_444_800_000)
            .await
            .unwrap_err();
        let DirectGrpcCallError::Status { evidence, .. } = error else {
            panic!("expected native gRPC status")
        };
        if call_index == 1 {
            assert_eq!(
                evidence.call_policy.events,
                [
                    CallPolicyEvent::RetryScheduled,
                    CallPolicyEvent::CallFailed,
                    CallPolicyEvent::CircuitOpened
                ]
            );
        }
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 4);

    let fallback = client
        .get_sla(&ServiceReference::new("support"), vec![], 4_102_444_800_000)
        .await
        .unwrap();
    assert_eq!(fallback.payload, b"cached-sla");
    assert_eq!(
        fallback.evidence.call_policy.terminal_outcome,
        CallPolicyTerminalOutcome::Fallback
    );
    assert_eq!(
        fallback.evidence.call_policy.events,
        [
            CallPolicyEvent::CircuitOpen,
            CallPolicyEvent::FallbackApplied
        ]
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 4);

    clock.advance_ms(1_000);
    available.store(true, Ordering::SeqCst);
    let recovered = client
        .get_sla(
            &ServiceReference::new("support"),
            b"live".to_vec(),
            4_102_444_800_000,
        )
        .await
        .unwrap();
    assert_eq!(recovered.payload, b"live");
    assert!(
        recovered
            .evidence
            .call_policy
            .events
            .contains(&CallPolicyEvent::CircuitRecovered)
    );
}

#[tokio::test]
async fn declared_grpc_deadline_and_transport_fallbacks_are_composition_owned() {
    let mut deadline_bindings = bindings();
    deadline_bindings
        .operation("GetSla")
        .expect("fixture operation");
    deadline_bindings.operations[0]
        .call_policy
        .fallback
        .as_mut()
        .unwrap()
        .on
        .push(CallPolicyFailure::DeadlineExpired);
    let empty_resolver = StaticEndpointResolver::new(Vec::<EndpointState>::new()).unwrap();
    let deadline = DirectGrpcClient::new(empty_resolver, deadline_bindings)
        .with_fallback("support.cached_sla", |_| b"deadline-fallback".to_vec())
        .get_sla(&ServiceReference::new("support"), vec![], 1)
        .await
        .unwrap();
    assert_eq!(deadline.payload, b"deadline-fallback");
    assert_eq!(
        deadline.evidence.call_policy.events,
        [
            CallPolicyEvent::DeadlineExpired,
            CallPolicyEvent::FallbackApplied
        ]
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut transport_bindings = bindings();
    transport_bindings.operations[0]
        .call_policy
        .fallback
        .as_mut()
        .unwrap()
        .on
        .push(CallPolicyFailure::TransportFailure);
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();
    let transport = DirectGrpcClient::new(resolver, transport_bindings)
        .with_fallback("support.cached_sla", |_| b"transport-fallback".to_vec())
        .get_sla(&ServiceReference::new("support"), vec![], 4_102_444_800_000)
        .await
        .unwrap();
    assert_eq!(transport.payload, b"transport-fallback");
    assert_eq!(transport.evidence.call_policy.attempts, 0);
    assert_eq!(
        transport.evidence.call_policy.terminal_outcome,
        CallPolicyTerminalOutcome::Fallback
    );
}

#[tokio::test]
async fn in_flight_grpc_deadline_uses_deadline_evidence_and_fallback() {
    let deadline_exceeded = Arc::new(AtomicBool::new(true));
    let client = client_with_behavior(
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicBool::new(true)),
        deadline_exceeded,
    )
    .await
    .with_fallback("support.cached_sla", |_| b"deadline-fallback".to_vec());

    let response = client
        .get_sla(&ServiceReference::new("support"), vec![], 4_102_444_800_000)
        .await
        .unwrap();
    assert_eq!(response.payload, b"deadline-fallback");
    assert_eq!(
        response.evidence.grpc_code,
        Some(tonic::Code::DeadlineExceeded)
    );
    assert!(
        response
            .evidence
            .call_policy
            .events
            .contains(&CallPolicyEvent::DeadlineExpired)
    );
    assert_eq!(
        response.evidence.call_policy.terminal_outcome,
        CallPolicyTerminalOutcome::Fallback
    );
}

#[tokio::test]
async fn generated_client_resolves_service_reference_before_transport() {
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new("http://127.0.0.1:1")],
    )])
    .unwrap();
    let error = DirectGrpcClient::new(resolver, bindings())
        .get_sla(&ServiceReference::new("missing"), vec![], 4_102_444_800_000)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("missing"));
}

#[tokio::test]
async fn generated_client_preserves_metadata_payload_and_absolute_deadline() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let response = client(Arc::clone(&attempts))
        .await
        .get_sla(
            &ServiceReference::new("support"),
            b"ticket-42".to_vec(),
            4_102_444_800_000,
        )
        .await
        .unwrap();
    assert_eq!(response.payload, b"ticket-42");
    assert_eq!(response.metadata.get("x-support-version").unwrap(), "v1");
    assert_eq!(response.evidence.attempts, 1);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn keyed_retry_keeps_native_status_and_proves_attempt_count() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let error = client(Arc::clone(&attempts))
        .await
        .update_sla(
            &ServiceReference::new("support"),
            vec![],
            4_102_444_800_000,
            "sla-42:update",
        )
        .await
        .unwrap_err();
    let DirectGrpcCallError::Status { status, evidence } = error else {
        panic!("expected native status")
    };
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    assert_eq!(status.message(), "stale revision");
    assert_eq!(status.metadata().get("x-support-revision").unwrap(), "41");
    assert_eq!(evidence.decision, "initial_policy_attempt_limit");
    assert_eq!(evidence.grpc_code, Some(tonic::Code::FailedPrecondition));
    assert_eq!(evidence.attempts, 2);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn keyed_operation_without_key_is_never_attempted() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let error = client(Arc::clone(&attempts))
        .await
        .update_sla(
            &ServiceReference::new("support"),
            vec![],
            4_102_444_800_000,
            "",
        )
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "idempotency_key_required");
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn declared_unknown_safety_operation_is_not_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let error = client(Arc::clone(&attempts))
        .await
        .probe_sla(&ServiceReference::new("support"), vec![], 4_102_444_800_000)
        .await
        .unwrap_err();
    let DirectGrpcCallError::Status { status, evidence } = error else {
        panic!("expected native status")
    };
    assert_eq!(status.code(), tonic::Code::Unavailable);
    assert_eq!(evidence.decision, "operation_retry_safety_unknown");
    assert_eq!(evidence.attempts, 1);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
