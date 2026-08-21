use std::{
    cell::{Cell, RefCell},
    net::SocketAddr,
    rc::Rc,
    str::FromStr,
    time::{Duration, Instant},
};

use lenso_auth_sdk::{AuthOutcome, CredentialEvidence, authenticate_request, decode_auth_response};
use lenso_capability_auth::{AuthClient, AuthInvocationError, AuthenticateError};
use lenso_capability_game_session::{
    GameSessionClient, GameSessionInvocationError, PlayError, PlayRequest, PlayResponse,
};
use lenso_kernel::{
    ActivateContext, CancellationToken, ModuleDependencies, ModuleLifecycle, NativeStream,
    RuntimeFailure, StreamEvent,
};
use lenso_native_adapter::{NativeModuleFactory, NativeModuleFactoryContext, NativeModuleInstance};
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};

use crate::{
    auth::GAME_CREDENTIAL_SCHEME,
    frame::{
        ClientFrame, FrameError, ServerFrame, TerminalFrame, read_client_frame, write_server_frame,
    },
};

/// Native package identity for the protocol Module.
pub const PROTOCOL_PACKAGE_ID: &str = "fixture.game.protocol";

const DEFAULT_MAX_FRAME_BYTES: usize = 16 * 1024;
const DEFAULT_MAX_CONNECTIONS: usize = 8;
const DEFAULT_IDLE_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_SESSION_TIMEOUT_MS: u64 = 30_000;
const MAX_FRAME_BYTES: usize = 64 * 1024;
const MAX_CONNECTIONS: usize = 1_024;

/// Configuration owned by the protocol Module and selected by Composition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProtocolConfig {
    /// TCP listen address. Port zero asks the OS for an available fixture port.
    pub bind: String,
    /// Maximum encoded JSON payload in one length-prefixed frame.
    pub max_frame_bytes: usize,
    /// Maximum number of accepted connections in this Module generation.
    pub max_connections: usize,
    /// Maximum time without a frame while an established session is active.
    pub idle_timeout_ms: u64,
    /// Maximum lifetime of one authenticated session.
    pub session_timeout_ms: u64,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:0".to_owned(),
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            idle_timeout_ms: DEFAULT_IDLE_TIMEOUT_MS,
            session_timeout_ms: DEFAULT_SESSION_TIMEOUT_MS,
        }
    }
}

impl ProtocolConfig {
    /// Serializes this immutable Module configuration for an App Plan.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("protocol configuration must serialize")
    }

    pub(crate) fn from_json(value: &str) -> Result<Self, RuntimeFailure> {
        let config: Self =
            serde_json::from_str(value).map_err(|error| RuntimeFailure::InvalidResolvedPlan {
                detail: format!("protocol configuration is invalid JSON: {error}"),
            })?;
        config.validate().map(|_| config)
    }

    pub(crate) fn validate(&self) -> Result<SocketAddr, RuntimeFailure> {
        let address = SocketAddr::from_str(&self.bind).map_err(|error| {
            RuntimeFailure::InvalidResolvedPlan {
                detail: format!("protocol bind address is invalid: {error}"),
            }
        })?;
        if self.max_frame_bytes < 256 || self.max_frame_bytes > MAX_FRAME_BYTES {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "protocol max_frame_bytes must be between 256 and {MAX_FRAME_BYTES}"
                ),
            });
        }
        if self.max_connections == 0 || self.max_connections > MAX_CONNECTIONS {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!("protocol max_connections must be between 1 and {MAX_CONNECTIONS}"),
            });
        }
        if self.idle_timeout_ms == 0 {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: "protocol idle_timeout_ms must be greater than zero".to_owned(),
            });
        }
        if self.session_timeout_ms < self.idle_timeout_ms {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: "protocol session_timeout_ms must be at least idle_timeout_ms".to_owned(),
            });
        }
        Ok(address)
    }

    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }

    pub fn max_connections(&self) -> usize {
        self.max_connections
    }

    fn idle_timeout(&self) -> Duration {
        Duration::from_millis(self.idle_timeout_ms)
    }

    fn session_timeout(&self) -> Duration {
        Duration::from_millis(self.session_timeout_ms)
    }
}

#[derive(Debug)]
struct ProtocolState {
    local_addr: Cell<Option<SocketAddr>>,
    listener: RefCell<Option<TcpListener>>,
    active_connections: Cell<usize>,
}

impl ProtocolState {
    fn new() -> Self {
        Self {
            local_addr: Cell::new(None),
            listener: RefCell::new(None),
            active_connections: Cell::new(0),
        }
    }
}

/// Factory for the replaceable native protocol Module.
#[derive(Clone, Debug)]
pub struct GameProtocolFactory {
    state: Rc<ProtocolState>,
}

impl GameProtocolFactory {
    /// Creates a protocol Module factory with inspectable bound-address state.
    pub fn new() -> Self {
        Self {
            state: Rc::new(ProtocolState::new()),
        }
    }

    /// Returns the concrete address after the Module has prepared its listener.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.state.local_addr.get()
    }
}

impl Default for GameProtocolFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeModuleFactory for GameProtocolFactory {
    fn package_id(&self) -> &'static str {
        PROTOCOL_PACKAGE_ID
    }

    fn instantiate(
        &self,
        context: NativeModuleFactoryContext<'_>,
    ) -> Result<NativeModuleInstance, RuntimeFailure> {
        let config = ProtocolConfig::from_json(context.configuration())?;
        Ok(NativeModuleInstance::with_lifecycle(
            Vec::new(),
            ProtocolLifecycle {
                config,
                state: self.state.clone(),
            },
        ))
    }
}

#[derive(Debug)]
struct ProtocolLifecycle {
    config: ProtocolConfig,
    state: Rc<ProtocolState>,
}

impl ModuleLifecycle for ProtocolLifecycle {
    fn prepare(&self, _context: lenso_kernel::PrepareContext) -> lenso_kernel::ModuleFuture {
        let config = self.config.clone();
        let state = self.state.clone();
        Box::pin(async move {
            let address = config.validate()?;
            let listener = TcpListener::bind(address).await.map_err(|error| {
                RuntimeFailure::ModuleFailure {
                    detail: format!("protocol listener bind failed: {error}"),
                }
            })?;
            let local_addr =
                listener
                    .local_addr()
                    .map_err(|error| RuntimeFailure::ModuleFailure {
                        detail: format!("protocol listener address failed: {error}"),
                    })?;
            state.local_addr.set(Some(local_addr));
            state.listener.borrow_mut().replace(listener);
            Ok(())
        })
    }

    fn activate(&self, context: ActivateContext) -> lenso_kernel::ModuleFuture {
        let auth = match AuthClient::from_dependencies(context.dependencies()) {
            Ok(client) => Rc::new(client),
            Err(error) => return Box::pin(futures::future::ready(Err(error))),
        };
        let game = match GameSessionClient::from_dependencies(context.dependencies()) {
            Ok(client) => Rc::new(client),
            Err(error) => return Box::pin(futures::future::ready(Err(error))),
        };
        let Some(listener) = self.state.listener.borrow_mut().take() else {
            return Box::pin(futures::future::ready(Err(RuntimeFailure::Internal {
                detail: "protocol listener was not prepared".to_owned(),
            })));
        };
        let config = self.config.clone();
        let state = self.state.clone();
        let readiness = context.readiness();
        let cancellation = context.cancellation();
        let dependencies = context.dependencies().clone();
        let tasks = context.tasks().clone();
        let runtime = ProtocolRuntime {
            connection: ConnectionRuntime {
                config,
                auth,
                game,
                dependencies,
                module_cancellation: cancellation.clone(),
            },
            state,
            tasks: tasks.clone(),
            cancellation,
        };
        let spawn = tasks.spawn_local(Box::pin(async move {
            readiness.wait().await;
            accept_connections(listener, runtime).await;
        }));
        match spawn {
            Ok(_) => Box::pin(futures::future::ready(Ok(()))),
            Err(error) => Box::pin(futures::future::ready(Err(RuntimeFailure::Internal {
                detail: format!("protocol accept loop could not start: {error:?}"),
            }))),
        }
    }
}

#[derive(Clone, Debug)]
struct ConnectionRuntime {
    config: ProtocolConfig,
    auth: Rc<AuthClient>,
    game: Rc<GameSessionClient>,
    dependencies: ModuleDependencies,
    module_cancellation: CancellationToken,
}

#[derive(Clone, Debug)]
struct ProtocolRuntime {
    connection: ConnectionRuntime,
    state: Rc<ProtocolState>,
    tasks: lenso_kernel::ManagedTaskScope,
    cancellation: CancellationToken,
}

async fn accept_connections(listener: TcpListener, runtime: ProtocolRuntime) {
    let ProtocolRuntime {
        connection,
        state,
        tasks,
        cancellation,
    } = runtime;
    let config = &connection.config;
    loop {
        tokio::select! {
            () = cancellation.cancelled() => break,
            accepted = listener.accept() => {
                let Ok((stream, _peer)) = accepted else {
                    continue;
                };
                if state.active_connections.get() >= config.max_connections() {
                    let mut stream = stream;
                    let _ = send_frame(
                        &mut stream,
                        config,
                        &ServerFrame::Runtime {
                            code: "resource_exhausted".to_owned(),
                        },
                    ).await;
                    continue;
                }
                state.active_connections.set(state.active_connections.get() + 1);
                let guard = ActiveConnectionGuard { state: state.clone() };
                let connection = connection.clone();
                let _ = tasks.spawn_local(Box::pin(async move {
                    let _guard = guard;
                    serve_connection(stream, connection).await;
                }));
            }
        }
    }
}

#[derive(Debug)]
struct ActiveConnectionGuard {
    state: Rc<ProtocolState>,
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.state
            .active_connections
            .set(self.state.active_connections.get().saturating_sub(1));
    }
}

async fn serve_connection(mut socket: TcpStream, connection: ConnectionRuntime) {
    let Some((stream, session_deadline, room)) = establish_session(&mut socket, &connection).await
    else {
        return;
    };
    if send_frame(
        &mut socket,
        &connection.config,
        &ServerFrame::Ready { room },
    )
    .await
    .is_err()
    {
        stream.cancel();
        return;
    }
    match forward_stream_event(&mut socket, &connection.config, &stream, &session_deadline).await {
        Ok(ForwardOutcome::Continue) => {}
        Ok(ForwardOutcome::Terminal) | Err(()) => {
            stream.cancel();
            return;
        }
    }
    run_session_loop(
        &mut socket,
        &connection.config,
        stream,
        &session_deadline,
        connection.module_cancellation,
    )
    .await;
}

async fn establish_session(
    socket: &mut TcpStream,
    connection: &ConnectionRuntime,
) -> Option<(
    NativeStream<lenso_capability_game_session::GameSession>,
    Instant,
    String,
)> {
    let config = &connection.config;
    let session_deadline = Instant::now() + config.session_timeout();
    let hello = match read_frame_with_deadline(
        socket,
        config.max_frame_bytes(),
        config.idle_timeout(),
        session_deadline,
    )
    .await
    {
        Ok(Some(frame)) => frame,
        Ok(None) => return None,
        Err(error) => {
            let _ = send_read_error(socket, config, error).await;
            return None;
        }
    };
    let ClientFrame::Hello {
        token,
        room,
        deadline_ms,
    } = hello
    else {
        let _ = send_runtime(socket, config, "protocol_violation").await;
        return None;
    };
    if room.trim().is_empty() {
        let _ = send_rejected(socket, config, "invalid_room").await;
        return None;
    }

    let requested_timeout =
        deadline_ms.map_or_else(|| config.session_timeout(), Duration::from_millis);
    let session_timeout = requested_timeout.min(config.session_timeout());
    if session_timeout.is_zero() {
        let _ = send_runtime(socket, config, "deadline_exceeded").await;
        return None;
    }
    let session_deadline = Instant::now() + session_timeout;
    let cancellation = CancellationToken::new();
    let Ok(context) = connection
        .dependencies
        .invocation_context_after(session_timeout, cancellation.clone())
    else {
        let _ = send_runtime(socket, config, "admission_closed").await;
        return None;
    };
    let context = context.with_caller_instance("protocol");
    let evidence = token.map(|token| CredentialEvidence::new(GAME_CREDENTIAL_SCHEME, token));
    let auth_response = match connection
        .auth
        .authenticate_with_context(context.clone(), authenticate_request(evidence))
        .await
    {
        Ok(response) => response,
        Err(AuthInvocationError::Domain(error)) => {
            let _ = send_rejected(socket, config, auth_error_code(&error)).await;
            return None;
        }
        Err(AuthInvocationError::Runtime(error)) => {
            let _ = send_runtime(socket, config, &runtime_error_code(&error)).await;
            return None;
        }
    };
    let Ok(outcome) = decode_auth_response(auth_response) else {
        let _ = send_runtime(socket, config, "protocol_violation").await;
        return None;
    };
    let assertion = match outcome {
        AuthOutcome::Absent => {
            let _ = send_rejected(socket, config, "credential_required").await;
            return None;
        }
        AuthOutcome::Authenticated(assertion) => assertion,
    };
    let Ok(context) = assertion.attach(context) else {
        let _ = send_runtime(socket, config, "protocol_violation").await;
        return None;
    };
    let stream = match connection
        .game
        .play_with_context(context, PlayRequest { room: room.clone() })
        .await
    {
        Ok(stream) => stream,
        Err(GameSessionInvocationError::Domain(error)) => {
            let _ = send_rejected(socket, config, game_error_code(&error)).await;
            return None;
        }
        Err(GameSessionInvocationError::Runtime(error)) => {
            let _ = send_runtime(socket, config, &runtime_error_code(&error)).await;
            return None;
        }
    };
    Some((stream, session_deadline, room))
}

async fn run_session_loop(
    socket: &mut TcpStream,
    config: &ProtocolConfig,
    stream: NativeStream<lenso_capability_game_session::GameSession>,
    session_deadline: &Instant,
    module_cancellation: CancellationToken,
) {
    loop {
        let frame = tokio::select! {
            () = module_cancellation.cancelled() => {
                stream.cancel();
                return;
            }
            result = read_frame_with_deadline(
                socket,
                config.max_frame_bytes(),
                config.idle_timeout(),
                *session_deadline,
            ) => result,
        };
        let frame = match frame {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                stream.cancel();
                return;
            }
            Err(error) => {
                stream.cancel();
                let _ = send_read_error(socket, config, error).await;
                return;
            }
        };
        match frame {
            ClientFrame::Message { action } if !action.is_empty() => {
                if let Err(error) = stream.send(PlayResponse { action }).await {
                    let _ = send_runtime(socket, config, &runtime_error_code(&error)).await;
                    return;
                }
                match forward_stream_event(socket, config, &stream, session_deadline).await {
                    Ok(ForwardOutcome::Continue) => {}
                    Ok(ForwardOutcome::Terminal) | Err(()) => {
                        stream.cancel();
                        return;
                    }
                }
            }
            ClientFrame::Message { .. } | ClientFrame::Hello { .. } => {
                stream.cancel();
                let _ = send_runtime(socket, config, "protocol_violation").await;
                return;
            }
            ClientFrame::CloseSend => {
                if let Err(error) = stream.close_send().await {
                    let _ = send_runtime(socket, config, &runtime_error_code(&error)).await;
                    return;
                }
                loop {
                    match forward_stream_event(socket, config, &stream, session_deadline).await {
                        Ok(ForwardOutcome::Continue) => {}
                        Ok(ForwardOutcome::Terminal) => return,
                        Err(()) => {
                            stream.cancel();
                            return;
                        }
                    }
                }
            }
            ClientFrame::Cancel => {
                stream.cancel();
                let _ = send_runtime(socket, config, "cancelled").await;
                return;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForwardOutcome {
    Continue,
    Terminal,
}

async fn forward_stream_event(
    socket: &mut TcpStream,
    config: &ProtocolConfig,
    stream: &NativeStream<lenso_capability_game_session::GameSession>,
    session_deadline: &Instant,
) -> Result<ForwardOutcome, ()> {
    let event = match receive_stream_event(stream, config, *session_deadline).await {
        Ok(event) => event,
        Err(StreamReceiveError::Runtime(error)) => {
            let _ = send_runtime(socket, config, &runtime_error_code(&error)).await;
            return Err(());
        }
        Err(StreamReceiveError::Timeout(code)) => {
            let _ = send_runtime(socket, config, code).await;
            return Err(());
        }
    };
    match event {
        StreamEvent::Message(message) => send_frame(
            socket,
            config,
            &ServerFrame::Message {
                action: message.action,
            },
        )
        .await
        .map(|()| ForwardOutcome::Continue)
        .map_err(|_| ()),
        StreamEvent::PeerHalfClosed => send_frame(socket, config, &ServerFrame::PeerHalfClosed)
            .await
            .map(|()| ForwardOutcome::Continue)
            .map_err(|_| ()),
        StreamEvent::Terminal(Ok(())) => send_frame(
            socket,
            config,
            &ServerFrame::Terminal {
                outcome: TerminalFrame::Success,
            },
        )
        .await
        .map(|()| ForwardOutcome::Terminal)
        .map_err(|_| ()),
        StreamEvent::Terminal(Err(error)) => send_frame(
            socket,
            config,
            &ServerFrame::Terminal {
                outcome: TerminalFrame::Domain {
                    code: game_error_code(&error),
                },
            },
        )
        .await
        .map(|()| ForwardOutcome::Terminal)
        .map_err(|_| ()),
    }
}

#[derive(Debug)]
enum StreamReceiveError {
    Runtime(RuntimeFailure),
    Timeout(&'static str),
}

async fn receive_stream_event(
    stream: &NativeStream<lenso_capability_game_session::GameSession>,
    config: &ProtocolConfig,
    session_deadline: Instant,
) -> Result<StreamEvent<PlayResponse, PlayError>, StreamReceiveError> {
    let remaining = session_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        stream.cancel();
        return Err(StreamReceiveError::Timeout("deadline_exceeded"));
    }
    let Ok(result) =
        tokio::time::timeout(config.idle_timeout().min(remaining), stream.receive()).await
    else {
        stream.cancel();
        return if Instant::now() >= session_deadline {
            Err(StreamReceiveError::Timeout("deadline_exceeded"))
        } else {
            Err(StreamReceiveError::Timeout("idle_timeout"))
        };
    };
    result.map_err(StreamReceiveError::Runtime)
}

#[derive(Debug)]
enum ReadFrameError {
    Frame(FrameError),
    Timeout(&'static str),
}

async fn read_frame_with_deadline(
    stream: &mut TcpStream,
    max_frame_bytes: usize,
    idle_timeout: Duration,
    deadline: Instant,
) -> Result<Option<ClientFrame>, ReadFrameError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(ReadFrameError::Timeout("deadline_exceeded"));
    }
    match tokio::time::timeout(
        idle_timeout.min(remaining),
        read_client_frame(stream, max_frame_bytes),
    )
    .await
    {
        Ok(result) => result.map_err(ReadFrameError::Frame),
        Err(_) => {
            if Instant::now() >= deadline {
                Err(ReadFrameError::Timeout("deadline_exceeded"))
            } else {
                Err(ReadFrameError::Timeout("idle_timeout"))
            }
        }
    }
}

async fn send_read_error(
    socket: &mut TcpStream,
    config: &ProtocolConfig,
    error: ReadFrameError,
) -> Result<(), FrameError> {
    match error {
        ReadFrameError::Timeout(code) => send_runtime(socket, config, code).await,
        ReadFrameError::Frame(FrameError::TooLarge) => {
            send_runtime(socket, config, "frame_too_large").await
        }
        ReadFrameError::Frame(FrameError::Malformed | FrameError::Truncated) => {
            send_runtime(socket, config, "protocol_violation").await
        }
        ReadFrameError::Frame(FrameError::Io(error)) => {
            let _ = error.kind();
            send_runtime(socket, config, "transport_error").await
        }
        ReadFrameError::Frame(FrameError::Timeout) => Ok(()),
    }
}

async fn send_frame(
    socket: &mut TcpStream,
    config: &ProtocolConfig,
    frame: &ServerFrame,
) -> Result<(), FrameError> {
    match tokio::time::timeout(
        config.idle_timeout(),
        write_server_frame(socket, frame, config.max_frame_bytes()),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(FrameError::Timeout),
    }
}

async fn send_runtime(
    socket: &mut TcpStream,
    config: &ProtocolConfig,
    code: &str,
) -> Result<(), FrameError> {
    send_frame(
        socket,
        config,
        &ServerFrame::Runtime {
            code: code.to_owned(),
        },
    )
    .await
}

async fn send_rejected(
    socket: &mut TcpStream,
    config: &ProtocolConfig,
    code: impl Into<String>,
) -> Result<(), FrameError> {
    send_frame(socket, config, &ServerFrame::Rejected { code: code.into() }).await
}

fn auth_error_code(error: &AuthenticateError) -> String {
    match error {
        AuthenticateError::Expired => "expired".to_owned(),
        AuthenticateError::Invalid => "invalid".to_owned(),
        AuthenticateError::Revoked => "revoked".to_owned(),
        AuthenticateError::Unsupported => "unsupported".to_owned(),
        AuthenticateError::Unknown(error) => error.code.clone(),
    }
}

fn game_error_code(error: &PlayError) -> String {
    match error {
        PlayError::ActorRequired => "actor_required".to_owned(),
        PlayError::InvalidAction => "invalid_action".to_owned(),
        PlayError::NotAllowed => "not_allowed".to_owned(),
        PlayError::RoomClosed => "room_closed".to_owned(),
        PlayError::Unknown(error) => error.code.clone(),
    }
}

fn runtime_error_code(error: &RuntimeFailure) -> String {
    match error {
        RuntimeFailure::Unavailable { .. } => "unavailable".to_owned(),
        RuntimeFailure::UnknownOperation { .. } => "unknown_operation".to_owned(),
        RuntimeFailure::AmbiguousBinding { .. } => "ambiguous_binding".to_owned(),
        RuntimeFailure::ProtocolViolation { .. } => "protocol_violation".to_owned(),
        RuntimeFailure::MissingModuleFactory { .. } => "missing_module_factory".to_owned(),
        RuntimeFailure::UnavailableExecutionClass { .. } => {
            "unavailable_execution_class".to_owned()
        }
        RuntimeFailure::InvalidResolvedPlan { .. } => "invalid_resolved_plan".to_owned(),
        RuntimeFailure::AdmissionClosed => "admission_closed".to_owned(),
        RuntimeFailure::ResourceExhausted { .. } => "resource_exhausted".to_owned(),
        RuntimeFailure::DeadlineExceeded { .. } => "deadline_exceeded".to_owned(),
        RuntimeFailure::Cancelled { .. } => "cancelled".to_owned(),
        RuntimeFailure::Internal { .. } => "internal".to_owned(),
        RuntimeFailure::ModuleFailure { .. } => "module_failure".to_owned(),
        RuntimeFailure::ModuleRestartExhausted { .. } => "module_restart_exhausted".to_owned(),
    }
}
