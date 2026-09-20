//! Deterministic test entrypoints that dispatch through the real event Ingress.
//!
//! This module intentionally owns no routes, Endpoint dispatch, credential
//! extraction, middleware, or response projection. [`SimulatedWebHost`] sends
//! every request to [`lenso_web_ingress_plugin::WebIngressEventFactory`], which
//! is the same event Ingress used by [`crate::NativeWebHost::start_event`].
//! A test runtime such as `lenso_test::TestApp` owns execution and virtual time.

use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use bytes::Bytes;
use http::{Request, Response};
use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::{CancellationToken, RuntimeFailure};
use lenso_native_adapter::NativePluginRegistry;
use lenso_web_ingress_plugin::{
    WebIngressEventBody, WebIngressEventFactory, WebIngressResponseStream, WebIngressRouteManifest,
    WebSocketSession,
};

/// Prepared, native-only event Host input for one deterministic test App.
///
/// Build this from [`crate::NativeWebHost::prepare_simulated`], then pass the
/// returned Plan and Registry to the real native test App. The accompanying
/// [`SimulatedWebHost`] can only use the exact `WebIngressEventFactory` that
/// was installed in that Registry.
#[derive(Debug)]
pub struct PreparedSimulatedWebHost {
    plan: ResolvedAppPlan,
    registry: NativePluginRegistry,
    ingress: WebIngressEventFactory,
}

impl PreparedSimulatedWebHost {
    pub(crate) fn new(
        plan: ResolvedAppPlan,
        registry: NativePluginRegistry,
        ingress: WebIngressEventFactory,
    ) -> Self {
        Self {
            plan,
            registry,
            ingress,
        }
    }

    /// Returns the exact immutable Plan that the test App must boot.
    #[must_use]
    pub const fn plan(&self) -> &ResolvedAppPlan {
        &self.plan
    }

    /// Returns the exact native Registry that contains the event Ingress.
    ///
    /// This consumes the preparation because native factory Registries are
    /// single-use Host assembly input.
    #[must_use]
    pub fn into_parts(self) -> (ResolvedAppPlan, NativePluginRegistry, SimulatedWebHost) {
        (
            self.plan,
            self.registry,
            SimulatedWebHost {
                ingress: self.ingress,
            },
        )
    }
}

/// A socket-free Web test surface backed by real event Ingress.
///
/// Construct this only through [`PreparedSimulatedWebHost::into_parts`]. Keep
/// the native test App that booted the paired Plan alive while using this
/// surface. The surface does not provide a second router or direct Endpoint
/// invocation path.
#[derive(Clone, Debug)]
pub struct SimulatedWebHost {
    ingress: WebIngressEventFactory,
}

impl SimulatedWebHost {
    /// Creates a controllable one-shot request. Call [`SimulatedWebRequest::disconnect`]
    /// to model client disconnect/cancellation before or during dispatch.
    #[must_use]
    pub fn begin_request(&self, request: Request<Bytes>) -> SimulatedWebRequest {
        SimulatedWebRequest {
            ingress: self.ingress.clone(),
            request: Rc::new(RefCell::new(Some(request))),
            cancellation: CancellationToken::new(),
        }
    }

    /// Dispatches one buffered HTTP request through real event Ingress.
    ///
    /// Streaming and WebSocket responses intentionally fail closed here. Use
    /// [`Self::open_stream`] or [`Self::open_websocket`] so their real transport
    /// lifecycles remain visible to the test.
    pub async fn request(
        &self,
        request: Request<Bytes>,
    ) -> Result<Response<Bytes>, SimulatedWebRequestError> {
        self.begin_request(request).send_buffered().await
    }

    /// Opens a real streaming response through event Ingress.
    pub async fn open_stream(
        &self,
        request: Request<Bytes>,
    ) -> Result<Response<WebIngressResponseStream>, SimulatedWebRequestError> {
        self.begin_request(request).open_stream().await
    }

    /// Opens a real WebSocket session through event Ingress.
    pub async fn open_websocket(
        &self,
        request: Request<Bytes>,
    ) -> Result<Response<WebSocketSession>, SimulatedWebRequestError> {
        self.begin_request(request).open_websocket().await
    }

    /// Returns the canonical route manifest published by the active event Ingress.
    #[must_use]
    pub fn route_manifest(&self) -> Option<WebIngressRouteManifest> {
        self.ingress.route_manifest()
    }
}

/// A one-shot simulated client request with explicit cancellation ownership.
///
/// Clones share the same request and cancellation token so a test can drive
/// dispatch on one clone and model a disconnect from another. A request cannot
/// be replayed after it has been dispatched.
#[derive(Clone, Debug)]
pub struct SimulatedWebRequest {
    ingress: WebIngressEventFactory,
    request: Rc<RefCell<Option<Request<Bytes>>>>,
    cancellation: CancellationToken,
}

impl SimulatedWebRequest {
    /// Returns this request's explicit cancellation token.
    #[must_use]
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Models the client disconnecting before it observes a response.
    pub fn disconnect(&self) {
        self.cancellation.cancel();
    }

    /// Returns whether the simulated client has disconnected.
    #[must_use]
    pub fn is_disconnected(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Sends this request and accepts only a buffered response.
    pub async fn send_buffered(&self) -> Result<Response<Bytes>, SimulatedWebRequestError> {
        let response = self.send_response().await?;
        let (parts, body) = response.into_parts();
        match body {
            WebIngressEventBody::Buffered(body) => Ok(Response::from_parts(parts, body)),
            WebIngressEventBody::Streaming(_) => Err(SimulatedWebRequestError::UnexpectedBody {
                expected: "a buffered HTTP response",
                actual: "a streaming HTTP response",
            }),
            WebIngressEventBody::WebSocket(_) => Err(SimulatedWebRequestError::UnexpectedBody {
                expected: "a buffered HTTP response",
                actual: "a WebSocket upgrade",
            }),
        }
    }

    /// Sends this request and accepts only a streaming response.
    pub async fn open_stream(
        &self,
    ) -> Result<Response<WebIngressResponseStream>, SimulatedWebRequestError> {
        let response = self.send_response().await?;
        let (parts, body) = response.into_parts();
        match body {
            WebIngressEventBody::Streaming(stream) => Ok(Response::from_parts(parts, stream)),
            WebIngressEventBody::Buffered(_) => Err(SimulatedWebRequestError::UnexpectedBody {
                expected: "a streaming HTTP response",
                actual: "a buffered HTTP response",
            }),
            WebIngressEventBody::WebSocket(_) => Err(SimulatedWebRequestError::UnexpectedBody {
                expected: "a streaming HTTP response",
                actual: "a WebSocket upgrade",
            }),
        }
    }

    /// Sends this request and accepts only a WebSocket upgrade.
    pub async fn open_websocket(
        &self,
    ) -> Result<Response<WebSocketSession>, SimulatedWebRequestError> {
        let response = self.send_response().await?;
        let (parts, body) = response.into_parts();
        match body {
            WebIngressEventBody::WebSocket(session) => Ok(Response::from_parts(parts, session)),
            WebIngressEventBody::Buffered(_) => Err(SimulatedWebRequestError::UnexpectedBody {
                expected: "a WebSocket upgrade",
                actual: "a buffered HTTP response",
            }),
            WebIngressEventBody::Streaming(_) => Err(SimulatedWebRequestError::UnexpectedBody {
                expected: "a WebSocket upgrade",
                actual: "a streaming HTTP response",
            }),
        }
    }

    async fn send_response(
        &self,
    ) -> Result<Response<WebIngressEventBody>, SimulatedWebRequestError> {
        let request = self
            .request
            .borrow_mut()
            .take()
            .ok_or(SimulatedWebRequestError::AlreadyDispatched)?;
        self.ingress
            .handle_response(request, self.cancellation.clone())
            .await
            .map_err(SimulatedWebRequestError::Runtime)
    }
}

/// A deterministic client-side dispatch error.
#[derive(Debug)]
pub enum SimulatedWebRequestError {
    /// A one-shot simulated request was sent more than once.
    AlreadyDispatched,
    /// Real event Ingress rejected or failed the request.
    Runtime(RuntimeFailure),
    /// The selected high-level operation did not match the real Ingress body kind.
    UnexpectedBody {
        /// The response kind required by the operation.
        expected: &'static str,
        /// The response kind actually returned by Ingress.
        actual: &'static str,
    },
}

impl fmt::Display for SimulatedWebRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyDispatched => {
                formatter.write_str("simulated request was already dispatched")
            }
            Self::Runtime(error) => write!(formatter, "event Ingress request failed: {error:?}"),
            Self::UnexpectedBody { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected}, but event Ingress returned {actual}"
                )
            }
        }
    }
}

impl Error for SimulatedWebRequestError {}
