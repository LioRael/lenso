use crate::response::{
    InvocationResponseError, ResponseBodyPolicy, decode_invocation_response,
    decode_json_response_with_policy,
};
use crate::{
    PROVIDER_PROTOCOL, ProviderConfig, ProviderErrorBody, ProviderHostEffectCoordinator,
    ProviderInvocation, ProviderInvocationAcknowledgement, ProviderInvocationMode,
    ProviderOperationKind, ProviderOutcome, ProviderOutcomeStatus, ProviderTransport,
};
use platform_core::error::ErrorDetail;
use platform_core::{ActorContext, AppError, AppResult, ErrorCode, TraceContext};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const MAX_PROVIDER_OUTCOME_BYTES: usize = 1024 * 1024;
const MAX_PROVIDER_EFFECT_EVIDENCE_ITEMS: usize = 100;

pub(crate) struct InvocationContext {
    pub invocation_id: String,
    pub request_id: String,
    pub attempt: u32,
    pub actor: ActorContext,
    pub tenant_id: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub trace: TraceContext,
}

pub(crate) fn build(
    config: &ProviderConfig,
    kind: ProviderOperationKind,
    operation_name: impl Into<String>,
    operation_version: impl Into<String>,
    mode: ProviderInvocationMode,
    context: InvocationContext,
    payload: Value,
) -> AppResult<ProviderInvocation> {
    let service_release_digest = locked(&config.service_release_digest, "Service Release")?;
    let module_release_digest = locked(&config.module_release_digest, "Module Release")?;
    let manifest_digest = locked(&config.manifest_digest, "Manifest")?;
    let contract_digest = match config.contract_digests.as_slice() {
        [digest] => digest.clone(),
        [] => {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Export has no locked operation contract digest",
            ));
        }
        _ => {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Export has multiple unkeyed contract digests; an exact operation contract cannot be selected",
            ));
        }
    };
    if config.export_key.is_empty() {
        return Err(AppError::new(
            ErrorCode::Validation,
            "Provider Export key is not locked",
        ));
    }
    let deadline = chrono::Utc::now()
        + chrono::Duration::milliseconds(i64::try_from(config.timeout_ms).unwrap_or(i64::MAX));
    Ok(ProviderInvocation {
        protocol: PROVIDER_PROTOCOL.to_owned(),
        invocation_id: context.invocation_id,
        request_id: context.request_id,
        attempt: context.attempt,
        deadline: deadline.to_rfc3339(),
        service_release_digest,
        export_key: config.export_key.clone(),
        module_release_digest,
        manifest_digest,
        operation_kind: kind,
        operation_name: operation_name.into(),
        operation_version: operation_version.into(),
        mode,
        input_contract_digest: contract_digest.clone(),
        output_contract_digest: contract_digest,
        tenant_id: context.tenant_id,
        actor: context.actor,
        delegation: None,
        locale: None,
        context: std::collections::BTreeMap::new(),
        correlation_id: context.correlation_id,
        causation_id: context.causation_id,
        trace: context.trace,
        content_type: "application/json".to_owned(),
        payload,
    })
}

pub(crate) async fn send(
    client: &reqwest::Client,
    config: &ProviderConfig,
    effects: &ProviderHostEffectCoordinator,
    binding: &str,
    invocation: &ProviderInvocation,
) -> AppResult<ProviderOutcome> {
    if config.transport == ProviderTransport::Grpc {
        let outcome = match crate::grpc::invoke(config, binding, invocation).await {
            Ok(outcome) => outcome,
            Err(crate::grpc::GrpcInvocationError::Ambiguous(invoke_error)) => {
                match crate::grpc::get_invocation(config, &invocation.invocation_id).await {
                    Ok(outcome) => outcome,
                    Err(_) => return Err(invoke_error),
                }
            }
            Err(crate::grpc::GrpcInvocationError::Received(invoke_error)) => {
                return Err(invoke_error);
            }
        };
        return finalize(client, config, effects, invocation, outcome).await;
    }
    let url = format!(
        "{}/exports/{}/{}",
        config.base_url, config.export_key, binding
    );
    let mut request = client.post(url).json(invocation);
    if let Some(token) = &config.auth_token {
        request = request.bearer_auth(token);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(source) => {
            let error = AppError::new(
                ErrorCode::ExternalDependency,
                "Provider invocation failed before its outcome could be read",
            )
            .with_source(source)
            .retryable();
            let outcome =
                recover_http_invocation(client, config, &invocation.invocation_id, error).await?;
            return finalize(client, config, effects, invocation, outcome).await;
        }
    };
    let outcome = match decode_invocation_response::<ProviderOutcome>(
        response,
        "invocation",
        provider_json_response_policy(),
    )
    .await
    {
        Ok(Some(outcome)) => outcome,
        Ok(None) => {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                "Provider invocation returned an empty outcome",
            ));
        }
        Err(InvocationResponseError::Ambiguous(error)) => {
            recover_http_invocation(client, config, &invocation.invocation_id, error).await?
        }
        Err(InvocationResponseError::Received(error)) => return Err(error),
    };
    finalize(client, config, effects, invocation, outcome).await
}

async fn recover_http_invocation(
    client: &reqwest::Client,
    config: &ProviderConfig,
    invocation_id: &str,
    original_error: AppError,
) -> AppResult<ProviderOutcome> {
    match get_http_invocation(client, config, invocation_id).await {
        Ok(outcome) => Ok(outcome),
        Err(_) => Err(original_error),
    }
}

async fn get_http_invocation(
    client: &reqwest::Client,
    config: &ProviderConfig,
    invocation_id: &str,
) -> AppResult<ProviderOutcome> {
    let url = format!("{}/invocations/{invocation_id}", config.base_url);
    let mut request = client.get(url);
    if let Some(token) = &config.auth_token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("Provider invocation recovery failed: {error}"),
        )
        .retryable()
    })?;
    decode_json_response_with_policy::<ProviderOutcome>(
        response,
        "invocation recovery",
        false,
        provider_json_response_policy(),
    )
    .await?
    .ok_or_else(|| {
        AppError::new(
            ErrorCode::ExternalDependency,
            "Provider invocation recovery returned an empty outcome",
        )
    })
}

async fn finalize(
    client: &reqwest::Client,
    config: &ProviderConfig,
    effects: &ProviderHostEffectCoordinator,
    invocation: &ProviderInvocation,
    outcome: ProviderOutcome,
) -> AppResult<ProviderOutcome> {
    let outcome = validate_outcome(invocation, outcome)?;
    let has_host_effects = !outcome.host_effects.events.is_empty()
        || !outcome.host_effects.runtime_function_requests.is_empty();
    effects.commit(config, invocation, &outcome).await?;
    let acknowledgement = ProviderInvocationAcknowledgement {
        invocation_id: outcome.invocation_id.clone(),
        outcome_digest: outcome.outcome_digest.clone(),
    };
    if config.transport == ProviderTransport::Grpc {
        crate::grpc::acknowledge_invocation(config, &acknowledgement).await?;
    } else {
        let url = format!(
            "{}/invocations/{}:ack",
            config.base_url, acknowledgement.invocation_id
        );
        let mut request = client.post(url).json(&acknowledgement);
        if let Some(token) = &config.auth_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("Provider invocation acknowledgement failed: {error}"),
            )
            .retryable()
        })?;
        decode_json_response_with_policy::<crate::ProviderInvocationReference>(
            response,
            "invocation acknowledgement",
            false,
            provider_json_response_policy(),
        )
        .await?;
    }
    if has_host_effects {
        effects
            .mark_acknowledged(
                &acknowledgement.invocation_id,
                &acknowledgement.outcome_digest,
            )
            .await?;
    }
    Ok(outcome)
}

fn provider_json_response_policy() -> ResponseBodyPolicy {
    ResponseBodyPolicy {
        max_bytes: Some(MAX_PROVIDER_OUTCOME_BYTES as u64),
        require_json_content_type: true,
        allow_empty_success: false,
    }
}

pub(crate) fn result(
    invocation: &ProviderInvocation,
    outcome: ProviderOutcome,
) -> AppResult<Value> {
    let outcome = validate_outcome(invocation, outcome)?;
    match outcome.status {
        ProviderOutcomeStatus::Succeeded => Ok(outcome.result.unwrap_or(Value::Null)),
        ProviderOutcomeStatus::Pending => Err(provider_outcome_error(
            ErrorCode::ExternalDependency,
            "Provider invocation remains pending",
            outcome.error,
            true,
        )),
        ProviderOutcomeStatus::Rejected => Err(provider_outcome_error(
            ErrorCode::Validation,
            "Provider invocation was rejected",
            outcome.error,
            false,
        )),
        ProviderOutcomeStatus::Failed => Err(provider_outcome_error(
            ErrorCode::ExternalDependency,
            "Provider invocation failed",
            outcome.error,
            false,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PROVIDER_PROTOCOL, ProviderErrorEnvelope, ProviderHostEffectBatch, ProviderInvocationMode,
        ProviderInvocationReference, ProviderOperationKind,
    };
    use platform_core::{ActorContext, TraceContext};
    use serde_json::json;
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tonic::codegen::tokio_stream::wrappers::TcpListenerStream;
    use tonic::codegen::{Body, BoxFuture, Service, StdError, http};
    use tonic::server::{NamedService, UnaryService};
    use tonic::{Request, Status};

    const TEST_PROVIDER_PREFIX: &str = "/lenso.provider.v1.Provider/";

    #[derive(Clone, PartialEq, prost::Message)]
    struct TestJsonEnvelope {
        #[prost(string, tag = "1")]
        payload_json: String,
    }

    #[derive(Debug, Clone, Copy)]
    enum InvokeReply {
        ProviderRateLimited,
        AmbiguousUnavailable,
        InvalidJson,
    }

    #[derive(Debug, Clone, Default)]
    struct MethodCounts {
        invoke: Arc<AtomicUsize>,
        get: Arc<AtomicUsize>,
        acknowledge: Arc<AtomicUsize>,
    }

    #[derive(Debug, Clone)]
    struct TestGrpcState {
        reply: InvokeReply,
        outcome: ProviderOutcome,
        counts: MethodCounts,
    }

    #[derive(Debug, Clone)]
    struct TestProviderGrpcServer {
        state: TestGrpcState,
    }

    impl<B> Service<http::Request<B>> for TestProviderGrpcServer
    where
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::Body>;
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: http::Request<B>) -> Self::Future {
            let Some(method) = request.uri().path().strip_prefix(TEST_PROVIDER_PREFIX) else {
                return unimplemented_response();
            };
            if !matches!(
                method,
                "InvokeRuntimeFunction" | "GetInvocation" | "AcknowledgeInvocation"
            ) {
                return unimplemented_response();
            }

            struct JsonService {
                method: String,
                state: TestGrpcState,
            }

            impl UnaryService<TestJsonEnvelope> for JsonService {
                type Response = TestJsonEnvelope;
                type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

                fn call(&mut self, request: Request<TestJsonEnvelope>) -> Self::Future {
                    let method = self.method.clone();
                    let state = self.state.clone();
                    Box::pin(async move {
                        let payload = handle_test_grpc_request(
                            &state,
                            &method,
                            &request.into_inner().payload_json,
                        )
                        .await?;
                        Ok(tonic::Response::new(TestJsonEnvelope {
                            payload_json: payload,
                        }))
                    })
                }
            }

            let method = method.to_owned();
            let state = self.state.clone();
            Box::pin(async move {
                let codec =
                    tonic_prost::ProstCodec::<TestJsonEnvelope, TestJsonEnvelope>::default();
                let mut grpc = tonic::server::Grpc::new(codec);
                Ok(grpc.unary(JsonService { method, state }, request).await)
            })
        }
    }

    impl NamedService for TestProviderGrpcServer {
        const NAME: &'static str = "lenso.provider.v1.Provider";
    }

    #[tokio::test]
    async fn send_does_not_get_after_received_retryable_provider_error() {
        let invocation = invocation("invoke-grpc-rate-limited");
        let state = TestGrpcState {
            reply: InvokeReply::ProviderRateLimited,
            outcome: succeeded_outcome(&invocation),
            counts: MethodCounts::default(),
        };
        let counts = state.counts.clone();
        let (base_url, server) = spawn_test_grpc_server(state).await;
        let config = grpc_config(base_url);

        let error = send(
            &reqwest::Client::new(),
            &config,
            &ProviderHostEffectCoordinator::rejecting(),
            "runtime:invoke",
            &invocation,
        )
        .await
        .expect_err("a received Provider error must remain an error");

        assert_eq!(error.code, ErrorCode::RateLimited);
        assert!(error.retryable);
        assert_eq!(error.retry_after_ms, Some(2_500));
        assert_eq!(
            error.provider_trace_reference.as_deref(),
            Some("provider-trace-1")
        );
        assert_eq!(counts.invoke.load(Ordering::SeqCst), 1);
        assert_eq!(counts.get.load(Ordering::SeqCst), 0);
        assert_eq!(counts.acknowledge.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[tokio::test]
    async fn send_gets_stored_outcome_and_acknowledges_after_ambiguous_status() {
        let invocation = invocation("invoke-grpc-ambiguous");
        let expected = succeeded_outcome(&invocation);
        let state = TestGrpcState {
            reply: InvokeReply::AmbiguousUnavailable,
            outcome: expected.clone(),
            counts: MethodCounts::default(),
        };
        let counts = state.counts.clone();
        let (base_url, server) = spawn_test_grpc_server(state).await;
        let config = grpc_config(base_url);

        let recovered = send(
            &reqwest::Client::new(),
            &config,
            &ProviderHostEffectCoordinator::rejecting(),
            "runtime:invoke",
            &invocation,
        )
        .await
        .expect("the Host should recover a stored outcome after an ambiguous status");

        assert_eq!(recovered.invocation_id, expected.invocation_id);
        assert_eq!(recovered.outcome_digest, expected.outcome_digest);
        assert_eq!(counts.invoke.load(Ordering::SeqCst), 1);
        assert_eq!(counts.get.load(Ordering::SeqCst), 1);
        assert_eq!(counts.acknowledge.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn send_does_not_get_after_fully_received_invalid_json() {
        let invocation = invocation("invoke-grpc-invalid-json");
        let state = TestGrpcState {
            reply: InvokeReply::InvalidJson,
            outcome: succeeded_outcome(&invocation),
            counts: MethodCounts::default(),
        };
        let counts = state.counts.clone();
        let (base_url, server) = spawn_test_grpc_server(state).await;
        let config = grpc_config(base_url);

        let error = send(
            &reqwest::Client::new(),
            &config,
            &ProviderHostEffectCoordinator::rejecting(),
            "runtime:invoke",
            &invocation,
        )
        .await
        .expect_err("invalid received JSON must fail without recovery");

        assert_eq!(error.code, ErrorCode::ExternalDependency);
        assert!(error.public_message.contains("response was invalid JSON"));
        assert_eq!(counts.invoke.load(Ordering::SeqCst), 1);
        assert_eq!(counts.get.load(Ordering::SeqCst), 0);
        assert_eq!(counts.acknowledge.load(Ordering::SeqCst), 0);
        server.abort();
    }

    async fn spawn_test_grpc_server(state: TestGrpcState) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test Provider listener should bind");
        let address = listener
            .local_addr()
            .expect("test Provider listener should have an address");
        let incoming = TcpListenerStream::new(listener);
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(TestProviderGrpcServer { state })
                .serve_with_incoming(incoming)
                .await
                .expect("test Provider should serve gRPC");
        });
        (format!("grpc://{address}"), server)
    }

    async fn handle_test_grpc_request(
        state: &TestGrpcState,
        method: &str,
        payload_json: &str,
    ) -> Result<String, Status> {
        match method {
            "InvokeRuntimeFunction" => {
                state.counts.invoke.fetch_add(1, Ordering::SeqCst);
                let invocation: ProviderInvocation = decode_test_json(payload_json)?;
                if invocation.invocation_id != state.outcome.invocation_id {
                    return Err(Status::invalid_argument(
                        "invocation identity did not match the test outcome",
                    ));
                }
                match state.reply {
                    InvokeReply::ProviderRateLimited => {
                        let envelope = ProviderErrorEnvelope {
                            error: ProviderErrorBody {
                                code: "rate_limited".to_owned(),
                                message: "Provider throttled".to_owned(),
                                retryable: true,
                                retry_after_ms: Some(2_500),
                                provider_trace_reference: Some("provider-trace-1".to_owned()),
                                details: Vec::new(),
                            },
                        };
                        Err(Status::resource_exhausted(encode_test_json(&envelope)?))
                    }
                    InvokeReply::AmbiguousUnavailable => Err(Status::unavailable(
                        "connection lost after durable completion",
                    )),
                    InvokeReply::InvalidJson => Ok("not-json".to_owned()),
                }
            }
            "GetInvocation" => {
                state.counts.get.fetch_add(1, Ordering::SeqCst);
                let reference: ProviderInvocationReference = decode_test_json(payload_json)?;
                if reference.invocation_id != state.outcome.invocation_id {
                    return Err(Status::not_found("invocation was not found"));
                }
                encode_test_json(&state.outcome)
            }
            "AcknowledgeInvocation" => {
                state.counts.acknowledge.fetch_add(1, Ordering::SeqCst);
                let acknowledgement: ProviderInvocationAcknowledgement =
                    decode_test_json(payload_json)?;
                if acknowledgement.invocation_id != state.outcome.invocation_id
                    || acknowledgement.outcome_digest != state.outcome.outcome_digest
                {
                    return Err(Status::failed_precondition(
                        "acknowledgement did not match the stored outcome",
                    ));
                }
                encode_test_json(&ProviderInvocationReference {
                    invocation_id: acknowledgement.invocation_id,
                })
            }
            _ => Err(Status::unimplemented("unsupported test Provider method")),
        }
    }

    fn encode_test_json(value: &impl serde::Serialize) -> Result<String, Status> {
        serde_json::to_string(value).map_err(|error| Status::internal(error.to_string()))
    }

    fn decode_test_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
        serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
    }

    fn unimplemented_response() -> BoxFuture<http::Response<tonic::body::Body>, Infallible> {
        Box::pin(async move {
            let mut response = http::Response::new(tonic::body::Body::default());
            response.headers_mut().insert(
                tonic::Status::GRPC_STATUS,
                (tonic::Code::Unimplemented as i32).into(),
            );
            response.headers_mut().insert(
                http::header::CONTENT_TYPE,
                tonic::metadata::GRPC_CONTENT_TYPE,
            );
            Ok(response)
        })
    }

    fn grpc_config(base_url: String) -> ProviderConfig {
        ProviderConfig::new("lenso/email", base_url).with_export_key("email")
    }

    fn succeeded_outcome(invocation: &ProviderInvocation) -> ProviderOutcome {
        let mut outcome = ProviderOutcome {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: invocation.invocation_id.clone(),
            status: ProviderOutcomeStatus::Succeeded,
            result: Some(json!({ "receiptId": "remote-123" })),
            error: None,
            effect_evidence: Vec::new(),
            host_effects: ProviderHostEffectBatch::default(),
            outcome_digest: String::new(),
        };
        outcome.outcome_digest = outcome_digest(&outcome);
        outcome
    }

    #[test]
    fn failed_outcome_preserves_bounded_provider_retry_metadata() {
        let invocation = invocation("invoke-1");
        let mut outcome = ProviderOutcome {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: invocation.invocation_id.clone(),
            status: ProviderOutcomeStatus::Failed,
            result: None,
            error: Some(ProviderErrorBody {
                code: "rate_limited".to_owned(),
                message: "Retry later".to_owned(),
                retryable: true,
                retry_after_ms: Some(u64::MAX),
                provider_trace_reference: Some(format!("trace\n{}", "x".repeat(1_000))),
                details: Vec::new(),
            }),
            effect_evidence: Vec::new(),
            host_effects: ProviderHostEffectBatch::default(),
            outcome_digest: String::new(),
        };
        outcome.outcome_digest = outcome_digest(&outcome);

        let error = result(&invocation, outcome).expect_err("failed outcome should fail");
        assert!(error.retryable);
        assert_eq!(error.retry_after_ms, Some(86_400_000));
        let trace = error.provider_trace_reference.expect("trace reference");
        assert!(trace.len() <= 512);
        assert!(!trace.chars().any(char::is_control));
    }

    #[test]
    fn rejected_outcome_never_retries_even_if_provider_marks_it_retryable() {
        let invocation = invocation("invoke-2");
        let mut outcome = ProviderOutcome {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: invocation.invocation_id.clone(),
            status: ProviderOutcomeStatus::Rejected,
            result: None,
            error: Some(ProviderErrorBody {
                code: "rejected".to_owned(),
                message: "Request rejected".to_owned(),
                retryable: true,
                retry_after_ms: Some(5_000),
                provider_trace_reference: None,
                details: Vec::new(),
            }),
            effect_evidence: Vec::new(),
            host_effects: ProviderHostEffectBatch::default(),
            outcome_digest: String::new(),
        };
        outcome.outcome_digest = outcome_digest(&outcome);

        let error = result(&invocation, outcome).expect_err("rejected outcome should fail");
        assert!(!error.retryable);
    }

    #[test]
    fn provider_outcome_digest_matches_the_typescript_jcs_vector() {
        let outcome = ProviderOutcome {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: "invocation-failed".to_owned(),
            status: ProviderOutcomeStatus::Failed,
            result: None,
            error: Some(ProviderErrorBody {
                code: "smtp_unavailable".to_owned(),
                message: "SMTP is temporarily unavailable".to_owned(),
                retryable: true,
                retry_after_ms: Some(2_500),
                provider_trace_reference: Some("smtp-attempt-3".to_owned()),
                details: Vec::new(),
            }),
            effect_evidence: Vec::new(),
            host_effects: ProviderHostEffectBatch::default(),
            outcome_digest: String::new(),
        };

        assert_eq!(
            outcome_digest(&outcome),
            "sha256:a7d26349366917a4012bf47b4f207416171819f558dc69ca851c05146936681f"
        );
    }

    fn invocation(id: &str) -> ProviderInvocation {
        ProviderInvocation {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: id.to_owned(),
            request_id: id.to_owned(),
            attempt: 1,
            deadline: "2026-08-13T00:00:00Z".to_owned(),
            service_release_digest: format!("sha256:{}", "1".repeat(64)),
            export_key: "email".to_owned(),
            module_release_digest: format!("sha256:{}", "2".repeat(64)),
            manifest_digest: format!("sha256:{}", "3".repeat(64)),
            operation_kind: ProviderOperationKind::RuntimeFunction,
            operation_name: "email.send.v1".to_owned(),
            operation_version: "1".to_owned(),
            mode: ProviderInvocationMode::Durable,
            input_contract_digest: format!("sha256:{}", "4".repeat(64)),
            output_contract_digest: format!("sha256:{}", "4".repeat(64)),
            tenant_id: None,
            actor: ActorContext::System,
            delegation: None,
            locale: None,
            context: Default::default(),
            correlation_id: "correlation-1".to_owned(),
            causation_id: None,
            trace: TraceContext::default(),
            content_type: "application/json".to_owned(),
            payload: json!({}),
        }
    }

    fn outcome_digest(outcome: &ProviderOutcome) -> String {
        let mut input = outcome.clone();
        input.outcome_digest.clear();
        let bytes = serde_json_canonicalizer::to_vec(&input).unwrap();
        sha256_digest(&bytes)
    }
}

fn provider_outcome_error(
    fallback_code: ErrorCode,
    fallback_message: &str,
    provider: Option<ProviderErrorBody>,
    pending_is_retryable: bool,
) -> AppError {
    let Some(provider) = provider else {
        let error = AppError::new(fallback_code, fallback_message);
        return if pending_is_retryable {
            error.retryable()
        } else {
            error
        };
    };
    let retryable =
        pending_is_retryable || (fallback_code != ErrorCode::Validation && provider.retryable);
    let retry_after_ms = retryable.then_some(provider.retry_after_ms).flatten();
    let mut error = AppError::new(
        fallback_code,
        format!("{}: {}", provider.code, provider.message),
    )
    .with_retry_after_ms(retry_after_ms)
    .with_provider_trace_reference(provider.provider_trace_reference);
    error.details = provider
        .details
        .into_iter()
        .map(|detail| ErrorDetail {
            field: detail.field,
            reason: detail.reason,
        })
        .collect();
    if retryable {
        error = error.retryable();
    }
    error
}

fn validate_outcome(
    invocation: &ProviderInvocation,
    outcome: ProviderOutcome,
) -> AppResult<ProviderOutcome> {
    if outcome.protocol != PROVIDER_PROTOCOL || outcome.invocation_id != invocation.invocation_id {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            "Provider outcome identity did not match the invocation",
        ));
    }
    if outcome.effect_evidence.len() > MAX_PROVIDER_EFFECT_EVIDENCE_ITEMS {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            format!(
                "Provider outcome exceeds the {MAX_PROVIDER_EFFECT_EVIDENCE_ITEMS} effect evidence item limit"
            ),
        ));
    }
    let has_host_effects = !outcome.host_effects.events.is_empty()
        || !outcome.host_effects.runtime_function_requests.is_empty();
    let invalid_shape = match outcome.status {
        ProviderOutcomeStatus::Succeeded => outcome.error.is_some(),
        ProviderOutcomeStatus::Pending => has_host_effects,
        ProviderOutcomeStatus::Rejected => {
            outcome.error.as_ref().is_none_or(|error| error.retryable) || has_host_effects
        }
        ProviderOutcomeStatus::Failed => outcome.error.is_none() || has_host_effects,
    };
    if invalid_shape {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            "Provider outcome status, error, and Host effects were inconsistent",
        ));
    }
    let mut digest_input = outcome.clone();
    digest_input.outcome_digest.clear();
    let encoded = serde_json_canonicalizer::to_vec(&digest_input).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("Provider outcome digest input could not be encoded: {error}"),
        )
    })?;
    if encoded.len() > MAX_PROVIDER_OUTCOME_BYTES {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            format!("Provider outcome exceeds {MAX_PROVIDER_OUTCOME_BYTES} bytes"),
        ));
    }
    let expected = sha256_digest(&encoded);
    if outcome.outcome_digest != expected {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            "Provider outcome digest did not match its content",
        ));
    }
    Ok(outcome)
}

fn sha256_digest(bytes: &[u8]) -> String {
    let mut digest = String::from("sha256:");
    for byte in Sha256::digest(bytes) {
        write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
    }
    digest
}

fn locked(value: &Option<String>, label: &str) -> AppResult<String> {
    value.clone().ok_or_else(|| {
        AppError::new(
            ErrorCode::Validation,
            format!("Provider {label} digest is not locked"),
        )
    })
}
