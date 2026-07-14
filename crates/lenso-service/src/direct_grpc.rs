use crate::{
    AuthenticatedServicePrincipal, AuthenticatedTransportBinding, CallPolicyDeclaration,
    CallPolicyEvent, CallPolicyEvidence, CallPolicyFailure, CallPolicyPermit, CallPolicyRuntime,
    CallPolicyTerminalOutcome, EndpointResolver, RetryDecision, ServiceReference,
    WorkloadIdentityProvider, WorkloadIdentityVerification,
    support_grpc_v1::{
        GetSlaRequest, ProbeSlaRequest, SlaResponse, UpdateSlaRequest,
        support_service_client::SupportServiceClient,
    },
};
use prost::Message;
use prost_types::FileDescriptorSet;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tonic::{Code, Request, Status, metadata::MetadataValue, transport::Channel};

const DEADLINE_METADATA: &str = "x-lenso-deadline-unix-ms";
const IDEMPOTENCY_METADATA: &str = "idempotency-key";
const AUTHORIZATION_METADATA: &str = "authorization";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrpcIdempotency {
    Unknown,
    Idempotent,
    RequiresKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectGrpcOperation {
    pub operation_id: String,
    pub path: String,
    pub idempotency: GrpcIdempotency,
    pub call_policy: CallPolicyDeclaration,
    pub request_type: String,
    pub response_type: String,
}

impl DirectGrpcOperation {
    fn retry_decision(&self, code: Code, attempt: u32, key: Option<&str>) -> RetryDecision {
        if self.idempotency == GrpcIdempotency::Unknown {
            return RetryDecision::no("operation_retry_safety_unknown");
        }
        if attempt >= self.call_policy.max_attempts {
            return RetryDecision::no("initial_policy_attempt_limit");
        }
        if !matches!(code, Code::Unavailable | Code::ResourceExhausted) {
            return RetryDecision::no("failure_not_retryable");
        }
        match self.idempotency {
            GrpcIdempotency::Idempotent => RetryDecision::yes(),
            GrpcIdempotency::RequiresKey if key.is_some_and(|value| !value.is_empty()) => {
                RetryDecision::yes()
            }
            GrpcIdempotency::RequiresKey => RetryDecision::no("idempotency_key_required"),
            GrpcIdempotency::Unknown => unreachable!("unknown safety returns before matching"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectGrpcBindings {
    pub contract_id: String,
    pub version: String,
    pub service_name: String,
    pub operations: Vec<DirectGrpcOperation>,
}

impl DirectGrpcBindings {
    #[must_use]
    pub fn operation(&self, id: &str) -> Option<&DirectGrpcOperation> {
        self.operations
            .iter()
            .find(|operation| operation.operation_id == id)
    }
}

#[derive(Debug, Clone)]
pub struct DirectGrpcServerPolicy {
    bindings: DirectGrpcBindings,
    runtime: CallPolicyRuntime,
    workload_identity: Option<DirectGrpcWorkloadIdentity>,
}

pub struct DirectGrpcAdmission {
    pub call_policy_permit: CallPolicyPermit,
    pub authenticated_service_principal: Option<AuthenticatedServicePrincipal>,
}

impl std::fmt::Debug for DirectGrpcAdmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectGrpcAdmission")
            .field(
                "authenticated_service_principal",
                &self.authenticated_service_principal,
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
struct DirectGrpcWorkloadIdentity {
    provider: Arc<dyn WorkloadIdentityProvider>,
    audience: String,
}

impl DirectGrpcServerPolicy {
    #[must_use]
    pub fn new(
        bindings: DirectGrpcBindings,
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
    ) -> Self {
        Self::new_unchecked(bindings).with_workload_identity(provider, audience)
    }

    #[must_use]
    #[cfg(debug_assertions)]
    pub fn new_without_workload_identity(bindings: DirectGrpcBindings) -> Self {
        Self::new_unchecked(bindings)
    }

    fn new_unchecked(bindings: DirectGrpcBindings) -> Self {
        Self {
            bindings,
            runtime: CallPolicyRuntime::default(),
            workload_identity: None,
        }
    }

    #[must_use]
    pub fn with_policy_runtime(mut self, runtime: CallPolicyRuntime) -> Self {
        self.runtime = runtime;
        self
    }

    #[must_use]
    pub fn with_workload_identity(
        mut self,
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
    ) -> Self {
        self.workload_identity = Some(DirectGrpcWorkloadIdentity {
            provider,
            audience: audience.into(),
        });
        self
    }

    pub fn admit<T>(
        &self,
        operation_id: &str,
        request: &Request<T>,
    ) -> Result<DirectGrpcAdmission, DirectGrpcAdmissionError> {
        let operation = self.bindings.operation(operation_id).ok_or_else(|| {
            DirectGrpcAdmissionError::Contract("operation_not_declared".to_owned())
        })?;
        let authenticated_service_principal = if let Some(identity) = &self.workload_identity {
            let credential = request
                .metadata()
                .get(AUTHORIZATION_METADATA)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .ok_or_else(|| {
                    DirectGrpcAdmissionError::Unauthenticated(
                        "workload_identity_required".to_owned(),
                    )
                })?;
            let binding = request
                .extensions()
                .get::<AuthenticatedTransportBinding>()
                .ok_or_else(|| {
                    DirectGrpcAdmissionError::Unauthenticated(
                        "authenticated_transport_binding_required".to_owned(),
                    )
                })?;
            let principal = identity
                .provider
                .verify(
                    credential,
                    &WorkloadIdentityVerification::new(
                        &identity.audience,
                        &binding.0,
                        self.runtime.now_ms(),
                    ),
                )
                .map_err(|error| {
                    DirectGrpcAdmissionError::Unauthenticated(error.evidence.outcome)
                })?;
            Some(principal)
        } else {
            None
        };
        let deadline = request
            .metadata()
            .get(DEADLINE_METADATA)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if deadline.is_none_or(|deadline| deadline <= self.runtime.now_ms()) {
            return Err(DirectGrpcAdmissionError::DeadlineExpired {
                evidence: rejected_evidence(CallPolicyEvent::DeadlineExpired),
            });
        }
        let idempotency_key = request
            .metadata()
            .get(IDEMPOTENCY_METADATA)
            .and_then(|value| value.to_str().ok());
        if operation.idempotency == GrpcIdempotency::RequiresKey
            && idempotency_key.is_none_or(str::is_empty)
        {
            return Err(DirectGrpcAdmissionError::IdempotencyKeyRequired {
                evidence: rejected_evidence(CallPolicyEvent::CallFailed),
            });
        }
        let operation_key = format!("{}:{}", self.bindings.contract_id, operation.operation_id);
        let call_policy_permit = self
            .runtime
            .admit(operation_key, &operation.call_policy)
            .map_err(|event| DirectGrpcAdmissionError::Overloaded {
                evidence: rejected_evidence(event),
            })?;
        Ok(DirectGrpcAdmission {
            call_policy_permit,
            authenticated_service_principal,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGrpcAdmissionError {
    Contract(String),
    Unauthenticated(String),
    DeadlineExpired { evidence: CallPolicyEvidence },
    IdempotencyKeyRequired { evidence: CallPolicyEvidence },
    Overloaded { evidence: CallPolicyEvidence },
}

impl DirectGrpcAdmissionError {
    #[must_use]
    pub fn status(&self) -> Status {
        match self {
            Self::Contract(message) => Status::unimplemented(message.clone()),
            Self::Unauthenticated(reason) => Status::unauthenticated(reason.clone()),
            Self::DeadlineExpired { .. } => Status::deadline_exceeded("deadline_expired"),
            Self::IdempotencyKeyRequired { .. } => {
                Status::invalid_argument("idempotency_key_required")
            }
            Self::Overloaded { .. } => Status::resource_exhausted("overload_rejected"),
        }
    }
}

fn rejected_evidence(event: CallPolicyEvent) -> CallPolicyEvidence {
    CallPolicyEvidence {
        events: vec![event],
        attempts: 0,
        terminal_outcome: CallPolicyTerminalOutcome::Rejected,
        fallback_handler: None,
    }
}

pub fn generate_direct_grpc_bindings(
    contract_id: impl Into<String>,
    version: impl Into<String>,
    proto_source: &str,
    descriptor_bytes: &[u8],
) -> Result<DirectGrpcBindings, String> {
    let descriptor =
        FileDescriptorSet::decode(descriptor_bytes).map_err(|error| error.to_string())?;
    let file = descriptor
        .file
        .first()
        .ok_or("Protobuf descriptor requires a file")?;
    let service = file
        .service
        .first()
        .ok_or("Protobuf descriptor requires a service")?;
    let package = file
        .package
        .as_deref()
        .ok_or("Protobuf descriptor requires a package")?;
    let service_name = service
        .name
        .as_deref()
        .ok_or("Protobuf service requires a name")?;
    let idempotency = protobuf_idempotency_annotations(proto_source)?;
    let call_policies = parse_protobuf_call_policies(
        proto_source,
        service.method.iter().map(|method| {
            let name = method.name.as_deref().unwrap_or_default();
            (
                name,
                idempotency
                    .get(name)
                    .copied()
                    .unwrap_or(GrpcIdempotency::Unknown),
            )
        }),
    )?;
    let mut operations = service
        .method
        .iter()
        .map(|method| {
            let name = method
                .name
                .clone()
                .ok_or("Protobuf method requires a name")?;
            let request_type = method
                .input_type
                .clone()
                .ok_or("Protobuf method requires an input type")?;
            let idempotency = idempotency
                .get(&name)
                .copied()
                .unwrap_or(GrpcIdempotency::Unknown);
            Ok(DirectGrpcOperation {
                path: format!("/{package}.{service_name}/{name}"),
                idempotency,
                call_policy: call_policies
                    .get(&name)
                    .cloned()
                    .expect("every descriptor method receives a call policy"),
                operation_id: name,
                request_type,
                response_type: method
                    .output_type
                    .clone()
                    .ok_or("Protobuf method requires an output type")?,
            })
        })
        .collect::<Result<Vec<_>, &str>>()?;
    operations.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    Ok(DirectGrpcBindings {
        contract_id: contract_id.into(),
        version: version.into(),
        service_name: format!("{package}.{service_name}"),
        operations,
    })
}

pub fn parse_protobuf_call_policies<I, S>(
    source: &str,
    operations: I,
) -> Result<std::collections::BTreeMap<String, CallPolicyDeclaration>, String>
where
    I: IntoIterator<Item = (S, GrpcIdempotency)>,
    S: AsRef<str>,
{
    let operations = operations
        .into_iter()
        .map(|(name, idempotency)| (name.as_ref().to_owned(), idempotency))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut declared = std::collections::BTreeMap::new();
    let mut pending: Option<&str> = None;
    for line in source.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("// lenso-call-policy: ") {
            pending = Some(value);
        } else if let Some(method) = line
            .strip_prefix("rpc ")
            .and_then(|line| line.split('(').next())
        {
            let method = method.trim();
            if let Some(value) = pending.take() {
                let policy: CallPolicyDeclaration =
                    serde_json::from_str(value).map_err(|error| {
                        format!("rpc {method} has invalid lenso-call-policy: {error}")
                    })?;
                declared.insert(method.to_owned(), policy);
            }
        } else if !line.starts_with("//") && !line.is_empty() {
            pending = None;
        }
    }
    let mut policies = std::collections::BTreeMap::new();
    for (method, idempotency) in operations {
        let retry_safe = idempotency != GrpcIdempotency::Unknown;
        let policy = declared
            .remove(&method)
            .ok_or_else(|| format!("rpc {method} requires lenso-call-policy"))?;
        if let Some(issue) = policy.validate(retry_safe).into_iter().next() {
            return Err(format!(
                "rpc {method} lenso-call-policy.{}: {}",
                issue.path, issue.code
            ));
        }
        policies.insert(method, policy);
    }
    Ok(policies)
}

fn protobuf_idempotency_annotations(
    source: &str,
) -> Result<std::collections::BTreeMap<String, GrpcIdempotency>, String> {
    let mut annotations = std::collections::BTreeMap::new();
    let mut pending = None;
    for line in source.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("// lenso-idempotency: ") {
            pending = Some(match value {
                "idempotent" => GrpcIdempotency::Idempotent,
                "requires_key" => GrpcIdempotency::RequiresKey,
                other => return Err(format!("unsupported lenso-idempotency `{other}`")),
            });
        } else if let Some(method) = line
            .strip_prefix("rpc ")
            .and_then(|line| line.split('(').next())
        {
            if let Some(value) = pending.take() {
                annotations.insert(method.trim().to_owned(), value);
            }
        } else if !line.starts_with("//") && !line.is_empty() {
            pending = None;
        }
    }
    Ok(annotations)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectGrpcEvidence {
    pub operation_id: String,
    pub decision: String,
    pub attempts: u32,
    pub call_policy: CallPolicyEvidence,
    pub grpc_code: Option<Code>,
    pub grpc_message: Option<String>,
}

#[derive(Debug)]
pub struct DirectGrpcResponse {
    pub payload: Vec<u8>,
    pub metadata: tonic::metadata::MetadataMap,
    pub evidence: DirectGrpcEvidence,
}

pub struct DirectGrpcClient<R> {
    resolver: R,
    bindings: DirectGrpcBindings,
    policy_runtime: CallPolicyRuntime,
    fallbacks: BTreeMap<String, Arc<GrpcFallback>>,
    workload_credential: Option<String>,
}
type GrpcFallback = dyn Fn(CallPolicyFailure) -> Vec<u8> + Send + Sync;

impl<R> std::fmt::Debug for DirectGrpcClient<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DirectGrpcClient")
            .field("bindings", &self.bindings)
            .field("policy_runtime", &self.policy_runtime)
            .field("fallbacks", &self.fallbacks.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

enum GrpcPolicyStart {
    Permit(CallPolicyPermit),
    Fallback(DirectGrpcResponse),
}

impl<R: EndpointResolver> DirectGrpcClient<R> {
    #[must_use]
    pub fn new(resolver: R, bindings: DirectGrpcBindings) -> Self {
        Self {
            resolver,
            bindings,
            policy_runtime: CallPolicyRuntime::default(),
            fallbacks: BTreeMap::new(),
            workload_credential: None,
        }
    }

    #[must_use]
    pub fn with_policy_runtime(mut self, policy_runtime: CallPolicyRuntime) -> Self {
        self.policy_runtime = policy_runtime;
        self
    }

    #[must_use]
    pub fn with_fallback<F>(mut self, handler: impl Into<String>, fallback: F) -> Self
    where
        F: Fn(CallPolicyFailure) -> Vec<u8> + Send + Sync + 'static,
    {
        self.fallbacks.insert(handler.into(), Arc::new(fallback));
        self
    }

    #[must_use]
    pub fn with_workload_credential(mut self, credential: impl Into<String>) -> Self {
        self.workload_credential = Some(credential.into());
        self
    }

    pub async fn get_sla(
        &self,
        service: &ServiceReference,
        payload: Vec<u8>,
        deadline: u64,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        let operation = self.operation("GetSla")?;
        if deadline <= now_ms() {
            return self.deadline_failure(operation, 0, Vec::new());
        }
        let mut permit = match self.start_policy(service, operation)? {
            GrpcPolicyStart::Permit(permit) => Some(permit),
            GrpcPolicyStart::Fallback(response) => return Ok(response),
        };
        let (mut client, _) = match self.prepare(service, "GetSla", deadline).await {
            Ok(prepared) => prepared,
            Err(error) => {
                return self.finish_prepare_error(
                    operation,
                    permit.take().expect("policy permit exists"),
                    error,
                );
            }
        };
        let mut retry_events = Vec::new();
        for attempt in 1..=operation.call_policy.max_attempts {
            let request = match request(
                GetSlaRequest {
                    payload: payload.clone(),
                },
                deadline,
                None,
                self.workload_credential.as_deref(),
            ) {
                Ok(request) => request,
                Err(DirectGrpcCallError::Contract(message)) if message == "deadline_expired" => {
                    retry_events.push(CallPolicyEvent::DeadlineExpired);
                    let events = permit
                        .take()
                        .expect("policy permit exists")
                        .failure_after(retry_events);
                    return self.deadline_failure(operation, attempt - 1, events);
                }
                Err(error) => return Err(error),
            };
            match client.get_sla(request).await {
                Ok(response) => {
                    return Ok(success(
                        operation,
                        response,
                        attempt,
                        permit.take().expect("policy permit exists"),
                        retry_events,
                    ));
                }
                Err(status) => {
                    let decision = operation.retry_decision(status.code(), attempt, None);
                    if decision.should_retry {
                        retry_events.push(CallPolicyEvent::RetryScheduled);
                        continue;
                    }
                    return self.finish_status(
                        operation,
                        status,
                        attempt,
                        permit.take().expect("policy permit exists"),
                        retry_events,
                        decision,
                    );
                }
            }
        }
        unreachable!()
    }

    pub async fn update_sla(
        &self,
        service: &ServiceReference,
        payload: Vec<u8>,
        deadline: u64,
        idempotency_key: &str,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        if idempotency_key.is_empty() {
            return Err(DirectGrpcCallError::Contract(
                "idempotency_key_required".to_owned(),
            ));
        }
        let operation = self.operation("UpdateSla")?;
        if deadline <= now_ms() {
            return self.deadline_failure(operation, 0, Vec::new());
        }
        let mut permit = match self.start_policy(service, operation)? {
            GrpcPolicyStart::Permit(permit) => Some(permit),
            GrpcPolicyStart::Fallback(response) => return Ok(response),
        };
        let (mut client, _) = match self.prepare(service, "UpdateSla", deadline).await {
            Ok(prepared) => prepared,
            Err(error) => {
                return self.finish_prepare_error(
                    operation,
                    permit.take().expect("policy permit exists"),
                    error,
                );
            }
        };
        let mut retry_events = Vec::new();
        for attempt in 1..=operation.call_policy.max_attempts {
            let request = match request(
                UpdateSlaRequest {
                    payload: payload.clone(),
                },
                deadline,
                Some(idempotency_key),
                self.workload_credential.as_deref(),
            ) {
                Ok(request) => request,
                Err(DirectGrpcCallError::Contract(message)) if message == "deadline_expired" => {
                    retry_events.push(CallPolicyEvent::DeadlineExpired);
                    let events = permit
                        .take()
                        .expect("policy permit exists")
                        .failure_after(retry_events);
                    return self.deadline_failure(operation, attempt - 1, events);
                }
                Err(error) => return Err(error),
            };
            match client.update_sla(request).await {
                Ok(response) => {
                    return Ok(success(
                        operation,
                        response,
                        attempt,
                        permit.take().expect("policy permit exists"),
                        retry_events,
                    ));
                }
                Err(status) => {
                    let decision =
                        operation.retry_decision(status.code(), attempt, Some(idempotency_key));
                    if decision.should_retry {
                        retry_events.push(CallPolicyEvent::RetryScheduled);
                        continue;
                    }
                    return self.finish_status(
                        operation,
                        status,
                        attempt,
                        permit.take().expect("policy permit exists"),
                        retry_events,
                        decision,
                    );
                }
            }
        }
        unreachable!()
    }

    pub async fn probe_sla(
        &self,
        service: &ServiceReference,
        payload: Vec<u8>,
        deadline: u64,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        let operation = self.operation("ProbeSla")?;
        if deadline <= now_ms() {
            return self.deadline_failure(operation, 0, Vec::new());
        }
        let mut permit = match self.start_policy(service, operation)? {
            GrpcPolicyStart::Permit(permit) => Some(permit),
            GrpcPolicyStart::Fallback(response) => return Ok(response),
        };
        let (mut client, _) = match self.prepare(service, "ProbeSla", deadline).await {
            Ok(prepared) => prepared,
            Err(error) => {
                return self.finish_prepare_error(
                    operation,
                    permit.take().expect("policy permit exists"),
                    error,
                );
            }
        };
        let request = match request(
            ProbeSlaRequest { payload },
            deadline,
            None,
            self.workload_credential.as_deref(),
        ) {
            Ok(request) => request,
            Err(DirectGrpcCallError::Contract(message)) if message == "deadline_expired" => {
                let events = permit
                    .take()
                    .expect("policy permit exists")
                    .failure_after(vec![CallPolicyEvent::DeadlineExpired]);
                return self.deadline_failure(operation, 0, events);
            }
            Err(error) => return Err(error),
        };
        match client.probe_sla(request).await {
            Ok(response) => Ok(success(
                operation,
                response,
                1,
                permit.take().expect("policy permit exists"),
                Vec::new(),
            )),
            Err(status) => {
                let decision = operation.retry_decision(status.code(), 1, None);
                self.finish_status(
                    operation,
                    status,
                    1,
                    permit.take().expect("policy permit exists"),
                    Vec::new(),
                    decision,
                )
            }
        }
    }

    fn start_policy(
        &self,
        service: &ServiceReference,
        operation: &DirectGrpcOperation,
    ) -> Result<GrpcPolicyStart, DirectGrpcCallError> {
        let operation_key = format!(
            "{}:{}:{}",
            service.as_str(),
            self.bindings.contract_id,
            operation.operation_id
        );
        match self
            .policy_runtime
            .begin_call(operation_key, &operation.call_policy)
        {
            Ok(permit) => Ok(GrpcPolicyStart::Permit(permit)),
            Err(event) => {
                let failure = match event {
                    CallPolicyEvent::CircuitOpen => CallPolicyFailure::CircuitOpen,
                    CallPolicyEvent::BulkheadSaturated => CallPolicyFailure::BulkheadSaturated,
                    _ => CallPolicyFailure::NonRetryableFailure,
                };
                let evidence = CallPolicyEvidence {
                    events: vec![event],
                    attempts: 0,
                    terminal_outcome: CallPolicyTerminalOutcome::Rejected,
                    fallback_handler: None,
                };
                if let Some(response) =
                    self.fallback_response(operation, failure, 0, evidence.events.clone(), None)
                {
                    return Ok(GrpcPolicyStart::Fallback(response));
                }
                Err(DirectGrpcCallError::Policy { failure, evidence })
            }
        }
    }

    fn operation(&self, operation_id: &str) -> Result<&DirectGrpcOperation, DirectGrpcCallError> {
        self.bindings
            .operation(operation_id)
            .ok_or_else(|| DirectGrpcCallError::Contract("operation_not_declared".to_owned()))
    }

    fn finish_status(
        &self,
        operation: &DirectGrpcOperation,
        status: Status,
        attempt: u32,
        permit: CallPolicyPermit,
        mut retry_events: Vec<CallPolicyEvent>,
        decision: RetryDecision,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        let retryable = matches!(status.code(), Code::Unavailable | Code::ResourceExhausted);
        let deadline_expired = status.code() == Code::DeadlineExceeded;
        if deadline_expired {
            retry_events.push(CallPolicyEvent::DeadlineExpired);
        } else if status.code() == Code::ResourceExhausted {
            retry_events.push(CallPolicyEvent::OverloadRejected);
        } else {
            retry_events.push(CallPolicyEvent::CallFailed);
        }
        let events = if retryable || deadline_expired {
            permit.failure_after(retry_events)
        } else {
            permit.success_after(retry_events)
        };
        let failure = if deadline_expired {
            CallPolicyFailure::DeadlineExpired
        } else if status.code() == Code::ResourceExhausted {
            CallPolicyFailure::OverloadRejected
        } else if retryable {
            CallPolicyFailure::RetryableFailure
        } else {
            CallPolicyFailure::NonRetryableFailure
        };
        if let Some(response) =
            self.fallback_response(operation, failure, attempt, events.clone(), Some(&status))
        {
            return Ok(response);
        }
        let evidence = DirectGrpcEvidence {
            operation_id: operation.operation_id.clone(),
            decision: if deadline_expired {
                "deadline_expired".to_owned()
            } else {
                decision.reason.to_owned()
            },
            attempts: attempt,
            call_policy: CallPolicyEvidence {
                events,
                attempts: attempt,
                terminal_outcome: CallPolicyTerminalOutcome::Failed,
                fallback_handler: None,
            },
            grpc_code: Some(status.code()),
            grpc_message: Some(status.message().to_owned()),
        };
        Err(DirectGrpcCallError::Status { status, evidence })
    }

    fn fallback_response(
        &self,
        operation: &DirectGrpcOperation,
        failure: CallPolicyFailure,
        attempts: u32,
        mut events: Vec<CallPolicyEvent>,
        native_status: Option<&Status>,
    ) -> Option<DirectGrpcResponse> {
        let declaration = operation.call_policy.fallback_for(failure)?;
        let fallback = self.fallbacks.get(&declaration.handler)?;
        events.push(CallPolicyEvent::FallbackApplied);
        Some(DirectGrpcResponse {
            payload: fallback(failure),
            metadata: tonic::metadata::MetadataMap::new(),
            evidence: DirectGrpcEvidence {
                operation_id: operation.operation_id.clone(),
                decision: "fallback_applied".to_owned(),
                attempts,
                call_policy: CallPolicyEvidence {
                    events,
                    attempts,
                    terminal_outcome: CallPolicyTerminalOutcome::Fallback,
                    fallback_handler: Some(declaration.handler.clone()),
                },
                grpc_code: native_status.map(Status::code),
                grpc_message: native_status.map(|status| status.message().to_owned()),
            },
        })
    }

    fn deadline_failure(
        &self,
        operation: &DirectGrpcOperation,
        attempts: u32,
        mut events: Vec<CallPolicyEvent>,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        if !events.contains(&CallPolicyEvent::DeadlineExpired) {
            events.push(CallPolicyEvent::DeadlineExpired);
        }
        if let Some(response) = self.fallback_response(
            operation,
            CallPolicyFailure::DeadlineExpired,
            attempts,
            events.clone(),
            None,
        ) {
            return Ok(response);
        }
        Err(DirectGrpcCallError::Policy {
            failure: CallPolicyFailure::DeadlineExpired,
            evidence: CallPolicyEvidence {
                events,
                attempts,
                terminal_outcome: CallPolicyTerminalOutcome::Failed,
                fallback_handler: None,
            },
        })
    }

    fn finish_prepare_error(
        &self,
        operation: &DirectGrpcOperation,
        permit: CallPolicyPermit,
        error: DirectGrpcCallError,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        match error {
            DirectGrpcCallError::Contract(message) if message == "deadline_expired" => {
                let events = permit.failure_after(vec![CallPolicyEvent::DeadlineExpired]);
                self.deadline_failure(operation, 0, events)
            }
            DirectGrpcCallError::Transport {
                source,
                mut evidence,
            } => {
                let events = permit.failure_after(vec![CallPolicyEvent::CallFailed]);
                if let Some(response) = self.fallback_response(
                    operation,
                    CallPolicyFailure::TransportFailure,
                    0,
                    events.clone(),
                    None,
                ) {
                    return Ok(response);
                }
                evidence.call_policy = CallPolicyEvidence {
                    events,
                    attempts: 0,
                    terminal_outcome: CallPolicyTerminalOutcome::Failed,
                    fallback_handler: None,
                };
                Err(DirectGrpcCallError::Transport { source, evidence })
            }
            error => Err(error),
        }
    }

    async fn prepare<'a>(
        &'a self,
        service: &ServiceReference,
        operation_id: &str,
        deadline: u64,
    ) -> Result<(SupportServiceClient<Channel>, &'a DirectGrpcOperation), DirectGrpcCallError> {
        if deadline <= now_ms() {
            return Err(DirectGrpcCallError::Contract("deadline_expired".to_owned()));
        }
        let operation = self
            .bindings
            .operation(operation_id)
            .ok_or_else(|| DirectGrpcCallError::Contract("operation_not_declared".to_owned()))?;
        let endpoint = self
            .resolver
            .resolve(service)
            .map_err(|error| DirectGrpcCallError::Resolution(error.to_string()))?
            .endpoints
            .into_iter()
            .next()
            .ok_or_else(|| DirectGrpcCallError::Resolution("no usable endpoint".to_owned()))?;
        let remaining = deadline.saturating_sub(now_ms());
        if remaining == 0 {
            return Err(DirectGrpcCallError::Contract("deadline_expired".to_owned()));
        }
        let channel = Channel::from_shared(endpoint.address)
            .map_err(DirectGrpcCallError::InvalidEndpoint)?
            .connect_timeout(Duration::from_millis(remaining))
            .connect()
            .await
            .map_err(|source| DirectGrpcCallError::Transport {
                source,
                evidence: transport_evidence(operation_id),
            })?;
        Ok((SupportServiceClient::new(channel), operation))
    }
}

fn request<T>(
    message: T,
    deadline: u64,
    key: Option<&str>,
    workload_credential: Option<&str>,
) -> Result<Request<T>, DirectGrpcCallError> {
    let remaining = deadline.saturating_sub(now_ms());
    if remaining == 0 {
        return Err(DirectGrpcCallError::Contract("deadline_expired".to_owned()));
    }
    let mut request = Request::new(message);
    request.set_timeout(Duration::from_millis(remaining));
    request.metadata_mut().insert(
        DEADLINE_METADATA,
        MetadataValue::try_from(deadline.to_string())
            .map_err(|error| DirectGrpcCallError::Contract(error.to_string()))?,
    );
    if let Some(key) = key {
        request.metadata_mut().insert(
            IDEMPOTENCY_METADATA,
            MetadataValue::try_from(key)
                .map_err(|error| DirectGrpcCallError::Contract(error.to_string()))?,
        );
    }
    if let Some(credential) = workload_credential {
        request.metadata_mut().insert(
            AUTHORIZATION_METADATA,
            MetadataValue::try_from(format!("Bearer {credential}"))
                .map_err(|error| DirectGrpcCallError::Contract(error.to_string()))?,
        );
    }
    Ok(request)
}

fn success(
    operation: &DirectGrpcOperation,
    response: tonic::Response<SlaResponse>,
    attempts: u32,
    permit: CallPolicyPermit,
    mut retry_events: Vec<CallPolicyEvent>,
) -> DirectGrpcResponse {
    let (metadata, value, _) = response.into_parts();
    retry_events.push(CallPolicyEvent::CallCompleted);
    let events = permit.success_after(retry_events);
    DirectGrpcResponse {
        payload: value.payload,
        metadata,
        evidence: DirectGrpcEvidence {
            operation_id: operation.operation_id.clone(),
            decision: "call_completed".to_owned(),
            attempts,
            call_policy: CallPolicyEvidence {
                events,
                attempts,
                terminal_outcome: CallPolicyTerminalOutcome::Completed,
                fallback_handler: None,
            },
            grpc_code: Some(Code::Ok),
            grpc_message: None,
        },
    }
}

fn transport_evidence(operation_id: &str) -> DirectGrpcEvidence {
    DirectGrpcEvidence {
        operation_id: operation_id.to_owned(),
        decision: "transport_failure_no_retry".to_owned(),
        attempts: 0,
        call_policy: CallPolicyEvidence {
            events: vec![CallPolicyEvent::CallFailed],
            attempts: 0,
            terminal_outcome: CallPolicyTerminalOutcome::Failed,
            fallback_handler: None,
        },
        grpc_code: None,
        grpc_message: None,
    }
}

#[derive(Debug)]
pub enum DirectGrpcCallError {
    Contract(String),
    Resolution(String),
    InvalidEndpoint(http::uri::InvalidUri),
    Transport {
        source: tonic::transport::Error,
        evidence: DirectGrpcEvidence,
    },
    Status {
        status: Status,
        evidence: DirectGrpcEvidence,
    },
    Policy {
        failure: CallPolicyFailure,
        evidence: CallPolicyEvidence,
    },
}
impl std::fmt::Display for DirectGrpcCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(value) | Self::Resolution(value) => formatter.write_str(value),
            Self::InvalidEndpoint(error) => error.fmt(formatter),
            Self::Transport { source, .. } => source.fmt(formatter),
            Self::Status { status, .. } => status.fmt(formatter),
            Self::Policy { failure, .. } => write!(formatter, "call policy rejected: {failure:?}"),
        }
    }
}
impl std::error::Error for DirectGrpcCallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEndpoint(error) => Some(error),
            Self::Transport { source, .. } => Some(source),
            Self::Status { status, .. } => Some(status),
            Self::Contract(_) | Self::Resolution(_) | Self::Policy { .. } => None,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
