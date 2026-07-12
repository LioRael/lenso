use crate::{
    EndpointResolver, RetryDecision, ServiceReference,
    support_grpc_v1::{
        GetSlaRequest, ProbeSlaRequest, SlaResponse, UpdateSlaRequest,
        support_service_client::SupportServiceClient,
    },
};
use prost::Message;
use prost_types::FileDescriptorSet;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tonic::{Code, Request, Status, metadata::MetadataValue, transport::Channel};

const DEADLINE_METADATA: &str = "x-lenso-deadline-unix-ms";
const IDEMPOTENCY_METADATA: &str = "idempotency-key";

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
    pub request_type: String,
    pub response_type: String,
}

impl DirectGrpcOperation {
    fn retry_decision(&self, code: Code, attempt: u32, key: Option<&str>) -> RetryDecision {
        if attempt > 1 {
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
            GrpcIdempotency::Unknown => RetryDecision::no("operation_retry_safety_unknown"),
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
    pub grpc_code: Option<Code>,
    pub grpc_message: Option<String>,
}

#[derive(Debug)]
pub struct DirectGrpcResponse {
    pub payload: Vec<u8>,
    pub metadata: tonic::metadata::MetadataMap,
    pub evidence: DirectGrpcEvidence,
}

#[derive(Debug)]
pub struct DirectGrpcClient<R> {
    resolver: R,
    bindings: DirectGrpcBindings,
}

impl<R: EndpointResolver> DirectGrpcClient<R> {
    #[must_use]
    pub fn new(resolver: R, bindings: DirectGrpcBindings) -> Self {
        Self { resolver, bindings }
    }

    pub async fn get_sla(
        &self,
        service: &ServiceReference,
        payload: Vec<u8>,
        deadline: u64,
    ) -> Result<DirectGrpcResponse, DirectGrpcCallError> {
        let (mut client, operation) = self.prepare(service, "GetSla", deadline).await?;
        for attempt in 1..=2 {
            let request = request(
                GetSlaRequest {
                    payload: payload.clone(),
                },
                deadline,
                None,
            )?;
            match client.get_sla(request).await {
                Ok(response) => return Ok(success(operation, response, attempt)),
                Err(status) => match failure(operation, status, attempt, None) {
                    Ok(error) => return Err(error),
                    Err(()) => continue,
                },
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
        let (mut client, operation) = self.prepare(service, "UpdateSla", deadline).await?;
        for attempt in 1..=2 {
            let request = request(
                UpdateSlaRequest {
                    payload: payload.clone(),
                },
                deadline,
                Some(idempotency_key),
            )?;
            match client.update_sla(request).await {
                Ok(response) => return Ok(success(operation, response, attempt)),
                Err(status) => match failure(operation, status, attempt, Some(idempotency_key)) {
                    Ok(error) => return Err(error),
                    Err(()) => continue,
                },
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
        let (mut client, operation) = self.prepare(service, "ProbeSla", deadline).await?;
        let request = request(ProbeSlaRequest { payload }, deadline, None)?;
        match client.probe_sla(request).await {
            Ok(response) => Ok(success(operation, response, 1)),
            Err(status) => {
                failure(operation, status, 1, None).map_or_else(|()| unreachable!(), Err)
            }
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
    Ok(request)
}

fn success(
    operation: &DirectGrpcOperation,
    response: tonic::Response<SlaResponse>,
    attempts: u32,
) -> DirectGrpcResponse {
    let (metadata, value, _) = response.into_parts();
    DirectGrpcResponse {
        payload: value.payload,
        metadata,
        evidence: DirectGrpcEvidence {
            operation_id: operation.operation_id.clone(),
            decision: "call_completed".to_owned(),
            attempts,
            grpc_code: Some(Code::Ok),
            grpc_message: None,
        },
    }
}

fn failure(
    operation: &DirectGrpcOperation,
    status: Status,
    attempt: u32,
    key: Option<&str>,
) -> Result<DirectGrpcCallError, ()> {
    let decision = operation.retry_decision(status.code(), attempt, key);
    if decision.should_retry {
        return Err(());
    }
    let evidence = DirectGrpcEvidence {
        operation_id: operation.operation_id.clone(),
        decision: decision.reason.to_owned(),
        attempts: attempt,
        grpc_code: Some(status.code()),
        grpc_message: Some(status.message().to_owned()),
    };
    Ok(DirectGrpcCallError::Status { status, evidence })
}

fn transport_evidence(operation_id: &str) -> DirectGrpcEvidence {
    DirectGrpcEvidence {
        operation_id: operation_id.to_owned(),
        decision: "transport_failure_no_retry".to_owned(),
        attempts: 0,
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
}
impl std::fmt::Display for DirectGrpcCallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(value) | Self::Resolution(value) => formatter.write_str(value),
            Self::InvalidEndpoint(error) => error.fmt(formatter),
            Self::Transport { source, .. } => source.fmt(formatter),
            Self::Status { status, .. } => status.fmt(formatter),
        }
    }
}
impl std::error::Error for DirectGrpcCallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEndpoint(error) => Some(error),
            Self::Transport { source, .. } => Some(source),
            Self::Status { status, .. } => Some(status),
            Self::Contract(_) | Self::Resolution(_) => None,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
