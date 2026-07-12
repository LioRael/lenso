use lenso_service::{
    DirectGrpcCallError, DirectGrpcClient, Endpoint, EndpointState, GrpcIdempotency,
    ServiceReference, StaticEndpointResolver, generate_direct_grpc_bindings,
    support_grpc_v1::{
        GetSlaRequest, ProbeSlaRequest, SlaResponse, UpdateSlaRequest,
        support_service_server::{SupportService, SupportServiceServer},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
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
}

#[tonic::async_trait]
impl SupportService for SupportFixture {
    async fn get_sla(
        &self,
        request: Request<GetSlaRequest>,
    ) -> Result<Response<SlaResponse>, Status> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
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
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Err(Status::unavailable("probe unavailable"))
    }
}

async fn client(attempts: Arc<AtomicUsize>) -> DirectGrpcClient<StaticEndpointResolver> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(SupportServiceServer::new(SupportFixture { attempts }))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    let resolver = StaticEndpointResolver::new([EndpointState::new(
        ServiceReference::new("support"),
        vec![Endpoint::new(format!("http://{address}"))],
    )])
    .unwrap();
    DirectGrpcClient::new(resolver, bindings())
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
