use std::{
    any::Any,
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures::future::LocalBoxFuture;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan, RestartPolicy,
};
use lenso_capability_http_endpoint as request;
use lenso_capability_http_stream_endpoint as stream;
use lenso_kernel::{
    InvocationContext, NativeRequestEndpoint, NativeRequestFuture, NativeStreamEndpoint,
    NativeStreamItem, NativeStreamSession, NoopPluginLifecycle, RuntimeFailure,
};
use lenso_native_adapter::{NativePluginFactory, NativePluginFactoryContext, NativePluginInstance};
use lenso_web_ingress_plugin::{
    PACKAGE_ID, WebIngressMiddleware, WebIngressMiddlewareOutcome, WebIngressRequest,
    WebIngressResponse,
};

const PACKAGE: &str = "fixture.http-lifecycle";

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Request,
    Stream,
}

pub fn plan(kind: Kind) -> ResolvedAppPlan {
    let (capability, version, describe, handle) = match kind {
        Kind::Request => (
            request::CAPABILITY_ID,
            request::DESCRIPTOR_VERSION,
            request::DESCRIBE_OPERATION,
            request::HANDLE_OPERATION,
        ),
        Kind::Stream => (
            stream::CAPABILITY_ID,
            stream::DESCRIPTOR_VERSION,
            stream::DESCRIBE_OPERATION,
            stream::HANDLE_OPERATION,
        ),
    };
    let endpoint = CapabilityEndpointPlan::new(capability, version, [describe, handle]);
    let endpoint = if matches!(kind, Kind::Stream) {
        endpoint.with_stream_operation(handle)
    } else {
        endpoint
    };
    AppComposition::new(
        vec![
            PluginInstancePlan::new("provider", PACKAGE)
                .with_authoring(2, "lenso.native-authoring@2")
                .with_restart_policy(RestartPolicy::on_failure(
                    1,
                    Duration::from_secs(60),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::from_secs(60),
                ))
                .with_capability(endpoint),
            PluginInstancePlan::new("web-ingress", PACKAGE_ID)
                .with_requirement(CapabilityRequirementPlan::many(capability, version)),
        ],
        vec![CapabilityBinding::new(
            "web-ingress",
            capability,
            version,
            "provider",
        )],
    )
    .resolve()
    .unwrap()
}

#[derive(Debug)]
pub struct Probe {
    gate: RefCell<Option<tokio::sync::oneshot::Receiver<()>>>,
    pub instantiations: Cell<usize>,
    pub calls: Cell<usize>,
    pub started: Cell<bool>,
    pub failures: Cell<usize>,
    pub execution_drops: Cell<usize>,
    pub cancels: Cell<usize>,
    pub session_drops: Cell<usize>,
    pub context: RefCell<Option<InvocationContext>>,
}

impl Probe {
    pub fn new(gate: tokio::sync::oneshot::Receiver<()>) -> Rc<Self> {
        Rc::new(Self {
            gate: RefCell::new(Some(gate)),
            instantiations: Cell::new(0),
            calls: Cell::new(0),
            started: Cell::new(false),
            failures: Cell::new(0),
            execution_drops: Cell::new(0),
            cancels: Cell::new(0),
            session_drops: Cell::new(0),
            context: RefCell::new(None),
        })
    }
    fn failure(self: &Rc<Self>) -> LocalBoxFuture<'static, RuntimeFailure> {
        let gate = self
            .gate
            .borrow_mut()
            .take()
            .expect("one failed execution only");
        let probe = self.clone();
        Box::pin(async move {
            let _execution = ExecutionLifetime(probe.clone());
            probe.started.set(true);
            gate.await
                .expect("test must explicitly release late provider completion");
            probe.failures.set(probe.failures.get() + 1);
            RuntimeFailure::PluginFailure {
                detail: "late failure after loopback disconnect".into(),
            }
        })
    }
}

struct ExecutionLifetime(Rc<Probe>);
impl Drop for ExecutionLifetime {
    fn drop(&mut self) {
        self.0.execution_drops.set(self.0.execution_drops.get() + 1);
    }
}

#[derive(Clone, Debug, Default)]
pub struct DisconnectWitness {
    pub disposed: Arc<AtomicBool>,
    pub completed: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct RequestLifetime(Arc<AtomicBool>);
impl Drop for RequestLifetime {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl WebIngressMiddleware for DisconnectWitness {
    fn identity(&self) -> &'static str {
        "fixture.http-lifecycle-disconnect:v1"
    }
    fn before_request<'a>(
        &'a self,
        request: &'a mut WebIngressRequest,
    ) -> LocalBoxFuture<'a, Result<WebIngressMiddlewareOutcome, RuntimeFailure>> {
        // middleware::run retains this normalized Request across dispatch.
        // The only owner of this Arc is its extensions; its final drop proves
        // server dispatch disposal without adding cancellation of our own.
        request
            .extensions_mut()
            .insert(Arc::new(RequestLifetime(self.disposed.clone())));
        Box::pin(async { Ok(WebIngressMiddlewareOutcome::Continue) })
    }
    fn after_response<'a>(
        &'a self,
        _: &'a WebIngressRequest,
        _: &'a mut WebIngressResponse,
    ) -> LocalBoxFuture<'a, Result<(), RuntimeFailure>> {
        self.completed.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

#[derive(Debug)]
pub struct Factory {
    pub kind: Kind,
    pub probe: Rc<Probe>,
}

impl NativePluginFactory for Factory {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn package_version(&self) -> &'static str {
        "0.1.0"
    }
    fn instantiate(
        &self,
        _: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        let generation = self.probe.instantiations.get() + 1;
        self.probe.instantiations.set(generation);
        let endpoint = Endpoint {
            generation,
            probe: self.probe.clone(),
        };
        Ok(match self.kind {
            Kind::Request => {
                NativePluginInstance::new(vec![Rc::new(request::EndpointEndpoint::new(endpoint))])
            }
            Kind::Stream => {
                let endpoint = Rc::new(stream::StreamEndpointEndpoint::new(endpoint));
                let request: Rc<dyn NativeRequestEndpoint> = endpoint.clone();
                let stream: Rc<dyn NativeStreamEndpoint> = endpoint;
                NativePluginInstance::with_endpoints(
                    vec![request],
                    vec![stream],
                    NoopPluginLifecycle,
                )
            }
        })
    }
}

#[derive(Debug)]
struct Endpoint {
    generation: usize,
    probe: Rc<Probe>,
}

impl request::EndpointProvider for Endpoint {
    fn describe(
        &self,
        _: InvocationContext,
        _: request::DescribeRequest,
    ) -> NativeRequestFuture<request::EndpointDescribe> {
        Box::pin(async {
            Ok(Ok(request::DescribeResponse {
                routes: vec![request::DescribeResponseRoutesItem {
                    method: "GET".into(),
                    path: "/work".into(),
                    route_id: "work".into(),
                    openapi: None,
                }],
            }))
        })
    }
    fn handle(
        &self,
        context: InvocationContext,
        _: request::HandleRequest,
    ) -> NativeRequestFuture<request::EndpointHandle> {
        self.probe.calls.set(self.probe.calls.get() + 1);
        self.probe.context.replace(Some(context));
        if self.generation == 1 {
            let failure = self.probe.failure();
            Box::pin(async move { Err(failure.await) })
        } else {
            let body = format!("generation-{}", self.generation)
                .into_bytes()
                .into();
            Box::pin(async move {
                Ok(Ok(request::HandleResponse {
                    body,
                    headers: vec![],
                    status: 200,
                }))
            })
        }
    }
}

impl stream::StreamEndpointProvider for Endpoint {
    fn describe_stream(
        &self,
        _: InvocationContext,
        _: stream::DescribeRequest,
    ) -> NativeRequestFuture<stream::StreamEndpointDescribe> {
        Box::pin(async {
            Ok(Ok(stream::DescribeResponse {
                routes: vec![stream::DescribeResponseRoutesItem {
                    method: "GET".into(),
                    path: "/work".into(),
                    route_id: "work".into(),
                }],
            }))
        })
    }
    fn handle_stream(
        &self,
        context: InvocationContext,
        _: stream::HandleRequest,
    ) -> LocalBoxFuture<
        'static,
        Result<Box<dyn NativeStreamSession>, stream::StreamEndpointHandleInvocationError>,
    > {
        self.probe.calls.set(self.probe.calls.get() + 1);
        self.probe.context.replace(Some(context));
        let session = Session {
            generation: self.generation,
            probe: self.probe.clone(),
            next: Cell::new(0),
            cancelled: Cell::new(false),
        };
        Box::pin(async move { Ok(Box::new(session) as Box<dyn NativeStreamSession>) })
    }
}

#[derive(Debug)]
struct Session {
    generation: usize,
    probe: Rc<Probe>,
    next: Cell<usize>,
    cancelled: Cell<bool>,
}

impl NativeStreamSession for Session {
    fn send(&self, _: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(async { Ok(()) })
    }
    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(async { Ok(()) })
    }
    fn cancel(&self) {
        if !self.cancelled.replace(true) {
            self.probe.cancels.set(self.probe.cancels.get() + 1);
        }
    }
    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        let next = self.next.get();
        self.next.set(next + 1);
        match next {
            0 => Box::pin(async {
                Ok(NativeStreamItem::Message(Box::new(
                    stream::HandleResponse {
                        body: None,
                        headers: Some(vec![]),
                        kind: stream::HandleResponseKind::Head,
                        status: Some(200),
                    },
                )))
            }),
            1 => {
                let body = if self.generation == 1 {
                    "first-chunk".to_owned()
                } else {
                    format!("generation-{}", self.generation)
                };
                Box::pin(async move {
                    Ok(NativeStreamItem::Message(Box::new(
                        stream::HandleResponse {
                            body: Some(body.into_bytes().into()),
                            headers: None,
                            kind: stream::HandleResponseKind::Chunk,
                            status: None,
                        },
                    )))
                })
            }
            2 if self.generation == 1 => {
                let failure = self.probe.failure();
                Box::pin(async move { Err(failure.await) })
            }
            2 => Box::pin(async { Ok(NativeStreamItem::Terminal(Ok(()))) }),
            _ => panic!("no frame may be requested after the sole terminal"),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.probe
            .session_drops
            .set(self.probe.session_drops.get() + 1);
    }
}
