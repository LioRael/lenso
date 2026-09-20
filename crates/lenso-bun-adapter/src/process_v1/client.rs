use std::{
    io::Read as _,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    process::{Child, ExitStatus},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac as _};
use lenso_process_protocol::{
    CANCEL_METHOD, CancelParams, ControlAck, HANDSHAKE_METHOD, HandshakeIdentity, HandshakeParams,
    HandshakeResult, JsonRpcRequest, JsonRpcSuccess, REQUEST_METHOD, RequestParams, RequestResult,
    SHUTDOWN_METHOD, ShutdownParams, child_proof_message, decode_strict, encode_compact,
    handshake_params_digest, host_proof_message,
};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::CONTENT_TYPE,
    redirect::Policy,
};
use sha2::Sha256;

use super::{
    HostProcessLimits, InvocationTerminal, SpawnedProcessV1, TerminalArbiter, TerminalKind,
};

/// Generated Capability codec checks applied on both sides of the process wire.
pub trait ProcessV1ValueValidator: std::fmt::Debug + Sync {
    fn capability_id(&self) -> &str;
    fn descriptor_version(&self) -> &str;
    fn descriptor_digest(&self) -> &str;
    fn validate_request(
        &self,
        operation: &str,
        value: &serde_json::Value,
    ) -> Result<(), ProcessV1ValidationError>;
    fn validate_success(
        &self,
        operation: &str,
        value: &serde_json::Value,
    ) -> Result<(), ProcessV1ValidationError>;
    fn validate_domain_error(
        &self,
        operation: &str,
        value: &serde_json::Value,
    ) -> Result<(), ProcessV1ValidationError>;
}

/// The generated Capability value codec rejected a Process V1 wire value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessV1ValidationError {
    /// The value does not conform to the generated Capability contract.
    Rejected,
}

/// Authenticated Process V1 data/control client for one child generation.
#[derive(Debug)]
pub struct ProcessV1Session {
    child: Mutex<Option<Child>>,
    client: Client,
    data_address: SocketAddr,
    control_address: SocketAddr,
    identity: HandshakeIdentity,
    session: String,
    next_control_id: AtomicU64,
    accepting: AtomicBool,
    terminal: Mutex<TerminalArbiter>,
    limits: HostProcessLimits,
}

impl ProcessV1Session {
    pub fn session(&self) -> &str {
        &self.session
    }

    /// Sends an idempotent control cancellation on the independently bounded listener.
    pub fn cancel(&self, correlation_id: String) -> Result<(), ClientError> {
        let numeric_id = correlation_id
            .parse::<u64>()
            .map_err(|_| ClientError::Protocol)?;
        self.terminal
            .lock()
            .map_err(|_| ClientError::Internal)?
            .cancel::<RequestResult>(numeric_id)
            .map_err(|_| ClientError::Protocol)?;
        let control_id = self
            .next_control_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let response: JsonRpcSuccess<ControlAck> = match self.send(
            self.control_address,
            "/control",
            &JsonRpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: control_id.clone(),
                method: CANCEL_METHOD.to_owned(),
                params: CancelParams {
                    session: self.session.clone(),
                    correlation_id,
                },
            },
            self.identity.peer_limits.max_control_http_body_bytes,
            Some(self.limits.cancel_ack_timeout),
        ) {
            Ok(response) => response,
            Err(error) => {
                self.retire();
                return Err(error);
            }
        };
        if response.jsonrpc != "2.0"
            || response.id != control_id
            || response.result.session != self.session
            || response.result.validate().is_err()
        {
            self.retire();
            return Err(ClientError::Protocol);
        }
        Ok(())
    }

    /// Sends one already Schema-encoded request with an exact correlation ID.
    // The terminal state machine intentionally remains linear: moving any of these
    // ordered transitions into independent helpers makes cancellation/deadline races harder to audit.
    #[expect(clippy::too_many_lines)]
    pub fn request(
        &self,
        mut params: RequestParams,
        validator: &dyn ProcessV1ValueValidator,
    ) -> Result<RequestResult, ClientError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(ClientError::Retired);
        }
        params.validate().map_err(|_| ClientError::Protocol)?;
        if params.session != self.session {
            return Err(ClientError::Protocol);
        }
        let Some(descriptor) = self
            .identity
            .provided_capabilities
            .iter()
            .find(|descriptor| descriptor.capability_id == params.capability_id)
        else {
            self.retire();
            return Err(ClientError::Protocol);
        };
        if descriptor.descriptor_version != params.descriptor_version
            || descriptor.descriptor_digest != params.descriptor_digest
            || validator.capability_id() != params.capability_id
            || validator.descriptor_version() != params.descriptor_version
            || validator.descriptor_digest() != params.descriptor_digest
            || !descriptor.operations.iter().any(|operation| {
                operation.operation == params.operation
                    && operation.interaction == params.interaction
            })
        {
            self.retire();
            return Err(ClientError::Protocol);
        }
        if validator
            .validate_request(&params.operation, &params.payload)
            .is_err()
        {
            return Err(ClientError::Protocol);
        }
        let correlation_id = params.correlation_id.clone();
        let operation = params.operation.clone();
        let numeric_id = correlation_id
            .parse::<u64>()
            .map_err(|_| ClientError::Protocol)?;
        let deadline = params
            .remaining_timeout_nanos
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| ClientError::Protocol)?
            .and_then(|nanos| Instant::now().checked_add(Duration::from_nanos(nanos)));
        self.terminal
            .lock()
            .map_err(|_| ClientError::Internal)?
            .admit(numeric_id, deadline)
            .map_err(|_| ClientError::Protocol)?;
        let request_timeout = if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = self
                    .terminal
                    .lock()
                    .map_err(|_| ClientError::Internal)?
                    .expire::<RequestResult>(numeric_id, Instant::now());
                return Err(ClientError::DeadlineExceeded);
            }
            params.remaining_timeout_nanos =
                Some(remaining.as_nanos().min(u128::from(u64::MAX)).to_string());
            Some(remaining)
        } else {
            None
        };
        let response: JsonRpcSuccess<RequestResult> = match self.send(
            self.data_address,
            "/rpc",
            &JsonRpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: correlation_id.clone(),
                method: REQUEST_METHOD.to_owned(),
                params,
            },
            self.identity.peer_limits.max_http_body_bytes,
            request_timeout,
        ) {
            Ok(response) => response,
            Err(error) => {
                let terminal = self
                    .terminal
                    .lock()
                    .map_err(|_| ClientError::Internal)?
                    .expire::<RequestResult>(numeric_id, Instant::now())
                    .map_err(|_| ClientError::Protocol)?;
                return if terminal.is_some() {
                    Err(ClientError::DeadlineExceeded)
                } else {
                    self.retire();
                    Err(error)
                };
            }
        };
        if response.jsonrpc != "2.0" || response.id != correlation_id {
            self.retire();
            return Err(ClientError::Protocol);
        }
        if response.result.validate().is_err() {
            self.retire();
            return Err(ClientError::Protocol);
        }
        if response.result.session != self.session || response.result.correlation_id != response.id
        {
            self.retire();
            return Err(ClientError::Protocol);
        }
        let value_is_valid = match &response.result.outcome {
            lenso_process_protocol::ProcessOutcome::Success { value } => {
                validator.validate_success(&operation, value)
            }
            lenso_process_protocol::ProcessOutcome::Domain { error } => {
                validator.validate_domain_error(&operation, error)
            }
            lenso_process_protocol::ProcessOutcome::Runtime { .. } => Ok(()),
        };
        if value_is_valid.is_err() {
            self.retire();
            return Err(ClientError::Protocol);
        }
        let terminal = self
            .terminal
            .lock()
            .map_err(|_| ClientError::Internal)?
            .respond(numeric_id, response.result, Instant::now());
        match terminal {
            Ok(InvocationTerminal::Response(result)) => {
                if matches!(
                    &result.outcome,
                    lenso_process_protocol::ProcessOutcome::Runtime {
                        failure: lenso_process_protocol::ChildRuntimeFailure::PluginFailure { .. }
                    }
                ) {
                    self.retire();
                }
                Ok(result)
            }
            Ok(InvocationTerminal::Cancelled) => Err(ClientError::Cancelled),
            Ok(InvocationTerminal::DeadlineExceeded) => Err(ClientError::DeadlineExceeded),
            Err(_) => match self
                .terminal
                .lock()
                .map_err(|_| ClientError::Internal)?
                .retired_terminal(numeric_id)
            {
                Some(TerminalKind::Cancelled) => Err(ClientError::Cancelled),
                Some(TerminalKind::DeadlineExceeded) => Err(ClientError::DeadlineExceeded),
                _ => {
                    self.retire();
                    Err(ClientError::Protocol)
                }
            },
        }
    }

    /// Executes acknowledged shutdown, process-group escalation, and bounded reap.
    pub fn shutdown(self) -> ShutdownDiagnostic {
        self.accepting.store(false, Ordering::Release);
        let active = self
            .terminal
            .lock()
            .map(|terminal| terminal.active_ids())
            .unwrap_or_default();
        for correlation_id in active {
            let _ = self.cancel(correlation_id.to_string());
        }
        let Some(mut child) = self.child.lock().ok().and_then(|mut child| child.take()) else {
            return ShutdownDiagnostic::ReapFailure;
        };
        let control_id = self
            .next_control_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let acknowledgement: Result<JsonRpcSuccess<ControlAck>, _> = self.send(
            self.control_address,
            "/control",
            &JsonRpcRequest {
                jsonrpc: "2.0".to_owned(),
                id: control_id.clone(),
                method: SHUTDOWN_METHOD.to_owned(),
                params: ShutdownParams {
                    session: self.session.clone(),
                },
            },
            self.identity.peer_limits.max_control_http_body_bytes,
            Some(self.limits.shutdown_ack_timeout),
        );
        let acknowledged = acknowledgement.is_ok_and(|response| {
            response.jsonrpc == "2.0"
                && response.id == control_id
                && response.result.session == self.session
                && response.result.validate().is_ok()
        });
        if let Ok(Some(status)) = wait_for_exit(&mut child, self.limits.shutdown_exit_timeout) {
            if super::process_group_exists(child.id()) {
                return terminate_remaining_group(child, status, acknowledged, &self.limits);
            }
            return if acknowledged && status.success() {
                ShutdownDiagnostic::Clean(status)
            } else if acknowledged {
                ShutdownDiagnostic::CrashedAfterAcknowledgement(status)
            } else {
                ShutdownDiagnostic::ExitedWithoutAcknowledgement(status)
            };
        }
        if signal_group(&child, Signal::SIGTERM).is_err() {
            let _ = child.kill();
        }
        if let Ok(Some(status)) = wait_for_exit(&mut child, self.limits.term_grace) {
            return if acknowledged {
                ShutdownDiagnostic::TerminatedAfterAcknowledgement(status)
            } else {
                ShutdownDiagnostic::TerminatedWithoutAcknowledgement(status)
            };
        }
        if signal_group(&child, Signal::SIGKILL).is_err() {
            let _ = child.kill();
        }
        match wait_for_exit(&mut child, self.limits.kill_reap_timeout) {
            Ok(Some(status)) => ShutdownDiagnostic::ForcedKill(status),
            Ok(None) => ShutdownDiagnostic::ReapTimeout,
            Err(()) => ShutdownDiagnostic::ReapFailure,
        }
    }

    fn send<P: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        address: SocketAddr,
        path: &str,
        request: &JsonRpcRequest<P>,
        maximum_bytes: u64,
        timeout: Option<Duration>,
    ) -> Result<R, ClientError> {
        let body = encode_compact(request).map_err(|_| ClientError::Protocol)?;
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > maximum_bytes {
            return Err(ClientError::OutboundTooLarge);
        }
        let request = self
            .client
            .post(format!("http://{address}{path}"))
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        let request = match timeout {
            Some(timeout) => request.timeout(timeout),
            None => request,
        };
        let response = request.send().map_err(|_| ClientError::Transport)?;
        decode_response(response, maximum_bytes)
    }

    fn retire(&self) {
        self.accepting.store(false, Ordering::Release);
        if let Ok(mut child) = self.child.lock()
            && let Some(mut child) = child.take()
        {
            super::retire_process_group(&mut child);
        }
    }
}

/// Authenticates exactly once and erases the host's bootstrap secret on return.
pub fn authenticate_process_v1(
    mut spawned: SpawnedProcessV1,
    identity: HandshakeIdentity,
    limits: HostProcessLimits,
) -> Result<ProcessV1Session, ClientError> {
    if identity.validate().is_err() {
        spawned.retire();
        return Err(ClientError::Protocol);
    }
    let mut nonce = [0_u8; 32];
    if getrandom::fill(&mut nonce).is_err() {
        spawned.retire();
        return Err(ClientError::Random);
    }
    let Ok(client) = Client::builder()
        .http1_only()
        .no_proxy()
        .redirect(Policy::none())
        .build()
    else {
        spawned.retire();
        return Err(ClientError::Transport);
    };
    let (child, readiness, bootstrap_secret) =
        spawned.into_parts().map_err(|_| ClientError::Internal)?;
    let data_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), readiness.data_port);
    let control_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), readiness.control_port);
    let maximum_retired =
        usize::try_from(identity.peer_limits.max_retired_correlation_ids).unwrap_or(usize::MAX);
    let mut temporary = ProcessV1Session {
        child: Mutex::new(Some(child)),
        client,
        data_address,
        control_address,
        identity,
        session: String::new(),
        next_control_id: AtomicU64::new(1),
        accepting: AtomicBool::new(true),
        terminal: Mutex::new(TerminalArbiter::new(maximum_retired)),
        limits,
    };
    let mut params = HandshakeParams {
        identity: temporary.identity.clone(),
        host_nonce: URL_SAFE_NO_PAD.encode(nonce),
        host_proof: URL_SAFE_NO_PAD.encode([0_u8; 32]),
    };
    let digest = handshake_params_digest(&params).map_err(|_| ClientError::Protocol)?;
    let mut host_mac = Hmac::<Sha256>::new_from_slice(bootstrap_secret.expose())
        .map_err(|_| ClientError::Random)?;
    host_mac.update(&host_proof_message(&digest));
    params.host_proof = URL_SAFE_NO_PAD.encode(host_mac.finalize().into_bytes());
    let response: Result<JsonRpcSuccess<HandshakeResult>, _> = temporary.send(
        control_address,
        "/control",
        &JsonRpcRequest {
            jsonrpc: "2.0".to_owned(),
            id: "0".to_owned(),
            method: HANDSHAKE_METHOD.to_owned(),
            params,
        },
        temporary.identity.peer_limits.max_control_http_body_bytes,
        Some(temporary.limits.startup_timeout),
    );
    let response = response?;
    if response.jsonrpc != "2.0" || response.id != "0" {
        return Err(ClientError::Protocol);
    }
    response
        .result
        .validate_against(&temporary.identity)
        .map_err(|_| ClientError::Protocol)?;
    let child_proof = URL_SAFE_NO_PAD
        .decode(&response.result.child_proof)
        .map_err(|_| ClientError::Protocol)?;
    let mut child_mac = Hmac::<Sha256>::new_from_slice(bootstrap_secret.expose())
        .map_err(|_| ClientError::Random)?;
    child_mac.update(
        &child_proof_message(&digest, &response.result.session)
            .map_err(|_| ClientError::Protocol)?,
    );
    child_mac
        .verify_slice(&child_proof)
        .map_err(|_| ClientError::Authentication)?;
    drop(bootstrap_secret);
    temporary.session = response.result.session;
    Ok(temporary)
}

fn decode_response<R: serde::de::DeserializeOwned>(
    mut response: Response,
    maximum_bytes: u64,
) -> Result<R, ClientError> {
    if response.status() != StatusCode::OK {
        return Err(ClientError::Protocol);
    }
    if response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some("application/json")
    {
        return Err(ClientError::Protocol);
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes)
    {
        return Err(ClientError::InboundTooLarge);
    }
    let mut body = Vec::new();
    response
        .by_ref()
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|_| ClientError::Transport)?;
    if u64::try_from(body.len()).unwrap_or(u64::MAX) > maximum_bytes {
        return Err(ClientError::InboundTooLarge);
    }
    decode_strict(&body).map_err(|_| ClientError::Protocol)
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<Option<ExitStatus>, ()> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {}
            Err(_) => return Err(()),
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_remaining_group(
    mut child: Child,
    leader_status: ExitStatus,
    acknowledged: bool,
    limits: &HostProcessLimits,
) -> ShutdownDiagnostic {
    let _ = signal_group(&child, Signal::SIGTERM);
    let deadline = Instant::now() + limits.term_grace;
    while super::process_group_exists(child.id()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !super::process_group_exists(child.id()) {
        return if acknowledged {
            ShutdownDiagnostic::DescendantsTerminatedAfterAcknowledgement(leader_status)
        } else {
            ShutdownDiagnostic::DescendantsTerminatedWithoutAcknowledgement(leader_status)
        };
    }
    let _ = signal_group(&child, Signal::SIGKILL);
    let deadline = Instant::now() + limits.kill_reap_timeout;
    while super::process_group_exists(child.id()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if super::process_group_exists(child.id()) {
        super::retire_process_group(&mut child);
        ShutdownDiagnostic::ReapTimeout
    } else {
        ShutdownDiagnostic::DescendantsForcedKill(leader_status)
    }
}

fn signal_group(child: &Child, signal: Signal) -> Result<(), ()> {
    let process_group = i32::try_from(child.id()).map_err(|_| ())?;
    killpg(Pid::from_raw(process_group), signal).map_err(|_| ())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientError {
    Random,
    Transport,
    Protocol,
    Authentication,
    OutboundTooLarge,
    InboundTooLarge,
    Cancelled,
    DeadlineExceeded,
    Retired,
    Internal,
}

#[derive(Debug)]
pub enum ShutdownDiagnostic {
    Clean(ExitStatus),
    CrashedAfterAcknowledgement(ExitStatus),
    ExitedWithoutAcknowledgement(ExitStatus),
    TerminatedAfterAcknowledgement(ExitStatus),
    TerminatedWithoutAcknowledgement(ExitStatus),
    ForcedKill(ExitStatus),
    DescendantsTerminatedAfterAcknowledgement(ExitStatus),
    DescendantsTerminatedWithoutAcknowledgement(ExitStatus),
    DescendantsForcedKill(ExitStatus),
    ReapTimeout,
    ReapFailure,
}

impl Drop for ProcessV1Session {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut()
            && let Some(mut child) = child.take()
        {
            super::retire_process_group(&mut child);
        }
    }
}
