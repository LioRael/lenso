use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use futures::future::{LocalBoxFuture, ready};
use lenso_app_plan::PluginInstancePlan;
use lenso_kernel::{
    EventCapability, InvocationContext, NativeEventEndpoint, NativeStreamEndpoint,
    NativeStreamHandle, NativeStreamItem, NativeStreamSession, NoopPluginLifecycle, RuntimeFailure,
    StreamCapability,
};

use super::{ConformancePlugin, ConformancePluginFactory};

/// Stable identity for the bidirectional stream conformance Capability.
pub const STREAM_PROBE_CAPABILITY_ID: &str = "lenso.runtime.conformance.stream-probe@1";
/// Exact stream conformance Descriptor version.
pub const STREAM_PROBE_DESCRIPTOR_VERSION: &str = "1.0.0";
/// Stream Operation exercised by Runtime Driver and Execution Adapter tests.
pub const STREAM_PROBE_OPERATION: &str = "exchange";
/// Provider package used by the stream conformance suite.
pub const STREAM_PROBE_PROVIDER_PACKAGE_ID: &str =
    "lenso.runtime.conformance.stream-probe-provider";
/// Consumer package used by the stream conformance suite.
pub const STREAM_PROBE_CONSUMER_PACKAGE_ID: &str =
    "lenso.runtime.conformance.stream-probe-consumer";

/// Request used to open one conformance stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamProbeOpen {
    pub value: String,
}

/// Ordered value exchanged in both directions by the conformance stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamProbeMessage {
    pub sequence: u64,
    pub value: String,
}

/// Capability-defined stream opening or terminal failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StreamProbeError {
    Rejected,
}

/// Typed bidirectional stream conformance Capability.
#[derive(Debug)]
pub struct StreamProbe;

impl StreamCapability for StreamProbe {
    type OpenRequest = StreamProbeOpen;
    type Message = StreamProbeMessage;
    type DomainError = StreamProbeError;

    const ID: &'static str = STREAM_PROBE_CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = STREAM_PROBE_DESCRIPTOR_VERSION;
}

/// Consumer wrapper shared by Runtime Driver and Execution Adapter tests.
#[derive(Debug)]
pub struct StreamProbeClient {
    handle: NativeStreamHandle<StreamProbe>,
}

impl StreamProbeClient {
    pub fn new(handle: NativeStreamHandle<StreamProbe>) -> Self {
        Self { handle }
    }

    pub fn from_dependencies(
        dependencies: &lenso_kernel::PluginDependencies,
    ) -> Result<Self, RuntimeFailure> {
        Ok(Self::new(dependencies.one_stream::<StreamProbe>()?))
    }

    pub async fn open(
        &self,
        request: StreamProbeOpen,
    ) -> Result<Result<lenso_kernel::NativeStream<StreamProbe>, StreamProbeError>, RuntimeFailure>
    {
        self.handle.open(STREAM_PROBE_OPERATION, request).await
    }
}

/// In-memory endpoint that echoes sent values while preserving stream protocol semantics.
#[derive(Clone, Debug, Default)]
pub struct StreamProbeEndpoint;

impl NativeStreamEndpoint for StreamProbeEndpoint {
    fn capability_id(&self) -> &'static str {
        STREAM_PROBE_CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        STREAM_PROBE_DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[STREAM_PROBE_OPERATION]
    }

    fn open(
        &self,
        operation: &str,
        request: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn NativeStreamSession>, Box<dyn Any>>, RuntimeFailure>,
    > {
        if operation != STREAM_PROBE_OPERATION {
            return Box::pin(ready(Err(RuntimeFailure::UnknownOperation {
                capability: STREAM_PROBE_CAPABILITY_ID,
                operation: operation.to_owned(),
            })));
        }
        let Ok(request) = request.downcast::<StreamProbeOpen>() else {
            return Box::pin(ready(Err(RuntimeFailure::ProtocolViolation {
                capability: STREAM_PROBE_CAPABILITY_ID,
            })));
        };
        if request.value == "reject" {
            return Box::pin(ready(Ok(Err(
                Box::new(StreamProbeError::Rejected) as Box<dyn Any>
            ))));
        }
        let session: Box<dyn NativeStreamSession> =
            Box::new(EchoStreamSession::new(context.request_id(), request.value));
        Box::pin(ready(Ok(Ok(session))))
    }
}

#[derive(Debug)]
struct EchoStreamState {
    open_value: String,
    pending: VecDeque<NativeStreamItem>,
    send_closed: bool,
    cancelled: bool,
}

#[derive(Debug)]
struct EchoStreamSession {
    request_id: u64,
    state: Rc<RefCell<EchoStreamState>>,
}

impl EchoStreamSession {
    fn new(request_id: u64, open_value: String) -> Self {
        Self {
            request_id,
            state: Rc::new(RefCell::new(EchoStreamState {
                open_value,
                pending: VecDeque::new(),
                send_closed: false,
                cancelled: false,
            })),
        }
    }
}

impl NativeStreamSession for EchoStreamSession {
    fn send(&self, message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let Ok(message) = message.downcast::<StreamProbeMessage>() else {
            return Box::pin(ready(Err(RuntimeFailure::ProtocolViolation {
                capability: STREAM_PROBE_CAPABILITY_ID,
            })));
        };
        let mut state = self.state.borrow_mut();
        if state.cancelled {
            return Box::pin(ready(Err(RuntimeFailure::Cancelled {
                request_id: self.request_id,
            })));
        }
        if state.send_closed {
            return Box::pin(ready(Err(RuntimeFailure::ProtocolViolation {
                capability: STREAM_PROBE_CAPABILITY_ID,
            })));
        }
        let message = StreamProbeMessage {
            sequence: message.sequence,
            value: format!("{}: {}", state.open_value, message.value),
        };
        state
            .pending
            .push_back(NativeStreamItem::Message(Box::new(message)));
        Box::pin(ready(Ok(())))
    }

    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        let result =
            self.state
                .borrow_mut()
                .pending
                .pop_front()
                .ok_or_else(|| RuntimeFailure::Internal {
                    detail: "stream conformance fixture has no pending item".to_owned(),
                });
        Box::pin(ready(result))
    }

    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let mut state = self.state.borrow_mut();
        if state.cancelled {
            return Box::pin(ready(Err(RuntimeFailure::Cancelled {
                request_id: self.request_id,
            })));
        }
        if !state.send_closed {
            state.send_closed = true;
            state.pending.push_back(NativeStreamItem::PeerHalfClosed);
            state.pending.push_back(NativeStreamItem::Terminal(Ok(())));
        }
        Box::pin(ready(Ok(())))
    }

    fn cancel(&self) {
        let mut state = self.state.borrow_mut();
        state.cancelled = true;
        state.pending.clear();
    }
}

/// Factory for the default stream conformance endpoint.
#[derive(Debug)]
pub struct StreamProbeProviderFactory;

impl ConformancePluginFactory for StreamProbeProviderFactory {
    fn package_id(&self) -> &'static str {
        STREAM_PROBE_PROVIDER_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn instantiate(
        &self,
        _instance: &PluginInstancePlan,
    ) -> Result<ConformancePlugin, RuntimeFailure> {
        Ok(ConformancePlugin::with_stream_endpoints(
            vec![Rc::new(StreamProbeEndpoint)],
            NoopPluginLifecycle,
        ))
    }
}

/// Stable identity for the ephemeral Event conformance Capability.
pub const EVENT_PROBE_CAPABILITY_ID: &str = "lenso.runtime.conformance.event-probe@1";
/// Exact Event conformance Descriptor version.
pub const EVENT_PROBE_DESCRIPTOR_VERSION: &str = "1.0.0";
/// Event Operation exercised by Runtime Driver and Execution Adapter tests.
pub const EVENT_PROBE_OPERATION: &str = "publish";
/// Provider package used by the Event conformance suite.
pub const EVENT_PROBE_PROVIDER_PACKAGE_ID: &str = "lenso.runtime.conformance.event-probe-provider";
/// Consumer package used by the Event conformance suite.
pub const EVENT_PROBE_CONSUMER_PACKAGE_ID: &str = "lenso.runtime.conformance.event-probe-consumer";

/// Ordered value published through the ephemeral Event conformance seam.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventProbeValue {
    pub sequence: u64,
    pub value: String,
}

/// Typed ephemeral Event conformance Capability.
#[derive(Debug)]
pub struct EventProbe;

impl EventCapability for EventProbe {
    type Event = EventProbeValue;

    const ID: &'static str = EVENT_PROBE_CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = EVENT_PROBE_DESCRIPTOR_VERSION;
}

/// Shared observation handle for accepted Event values.
#[derive(Clone, Debug, Default)]
pub struct EventProbeRecorder {
    seen: Rc<RefCell<Vec<EventProbeValue>>>,
}

impl EventProbeRecorder {
    pub fn values(&self) -> Vec<EventProbeValue> {
        self.seen.borrow().clone()
    }
}

/// In-memory Event endpoint used by the conformance Adapter.
#[derive(Clone, Debug)]
pub struct EventProbeEndpoint {
    recorder: EventProbeRecorder,
    available: Rc<Cell<bool>>,
}

impl EventProbeEndpoint {
    pub fn new(recorder: EventProbeRecorder) -> Self {
        Self {
            recorder,
            available: Rc::new(Cell::new(true)),
        }
    }

    pub fn set_available(&self, available: bool) {
        self.available.set(available);
    }
}

impl NativeEventEndpoint for EventProbeEndpoint {
    fn capability_id(&self) -> &'static str {
        EVENT_PROBE_CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        EVENT_PROBE_DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[EVENT_PROBE_OPERATION]
    }

    fn publish(
        &self,
        operation: &str,
        event: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        if operation != EVENT_PROBE_OPERATION {
            return Box::pin(ready(Err(RuntimeFailure::UnknownOperation {
                capability: EVENT_PROBE_CAPABILITY_ID,
                operation: operation.to_owned(),
            })));
        }
        if !self.available.get() {
            return Box::pin(ready(Err(RuntimeFailure::Unavailable {
                capability: EVENT_PROBE_CAPABILITY_ID,
            })));
        }
        let Ok(event) = event.downcast::<EventProbeValue>() else {
            return Box::pin(ready(Err(RuntimeFailure::ProtocolViolation {
                capability: EVENT_PROBE_CAPABILITY_ID,
            })));
        };
        self.recorder.seen.borrow_mut().push(*event);
        Box::pin(ready(Ok(())))
    }
}

/// Factory for one observable Event conformance endpoint.
#[derive(Clone, Debug)]
pub struct EventProbeProviderFactory {
    recorder: EventProbeRecorder,
}

impl EventProbeProviderFactory {
    pub fn new(recorder: EventProbeRecorder) -> Self {
        Self { recorder }
    }
}

impl ConformancePluginFactory for EventProbeProviderFactory {
    fn package_id(&self) -> &'static str {
        EVENT_PROBE_PROVIDER_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn instantiate(
        &self,
        _instance: &PluginInstancePlan,
    ) -> Result<ConformancePlugin, RuntimeFailure> {
        Ok(ConformancePlugin::with_event_endpoints(
            vec![Rc::new(EventProbeEndpoint::new(self.recorder.clone()))],
            NoopPluginLifecycle,
        ))
    }
}
