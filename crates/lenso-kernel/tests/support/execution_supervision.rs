use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    time::Duration,
};

use futures::{channel::oneshot, future::LocalBoxFuture};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan, RestartPolicy,
};
use lenso_kernel::{
    DeterministicDriver, EventCapability, ExecutionLease, InvocationContext, Kernel, NativeApp,
    NativeEventEndpoint, NativeExecutionAdapter, NativeRequestEndpoint, NativeStreamEndpoint,
    NativeStreamItem, NativeStreamSession, NoopPluginLifecycle, PreparedBinding,
    PreparedEventBinding, PreparedNativeApp, PreparedNativePlugin, PreparedStreamBinding,
    RequestCapability, RuntimeDriver, RuntimeFailure, StreamCapability,
};

pub const CAP: &str = "test.execution-supervision@1";
const VERSION: &str = "1.0.0";
pub const OP: &str = "work";

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Success,
    Domain,
    Exhausted,
    Fail,
    CancelAndFail,
    RetainAndFail,
}

#[derive(Debug)]
pub struct Contract;

impl RequestCapability for Contract {
    type Request = Action;
    type Response = ();
    type DomainError = ();
    const ID: &'static str = CAP;
    const DESCRIPTOR_VERSION: &'static str = VERSION;
}
impl StreamCapability for Contract {
    type OpenRequest = Action;
    type Message = Action;
    type DomainError = ();
    const ID: &'static str = CAP;
    const DESCRIPTOR_VERSION: &'static str = VERSION;
}
impl EventCapability for Contract {
    type Event = Action;
    const ID: &'static str = CAP;
    const DESCRIPTOR_VERSION: &'static str = VERSION;
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Request,
    Stream,
    Event { adapter_admission: bool },
}

#[derive(Debug, Default)]
pub struct Probe {
    gate: RefCell<Option<oneshot::Receiver<()>>>,
    pub calls: Cell<usize>,
    pub finished: Cell<usize>,
    pub cancels: Cell<usize>,
    pub retained: RefCell<Option<ExecutionLease>>,
}

impl Probe {
    fn execute(
        self: &Rc<Self>,
        action: Action,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<Result<(), ()>, RuntimeFailure>> {
        let probe = self.clone();
        let gate = self.gate.borrow_mut().take();
        Box::pin(async move {
            probe.calls.set(probe.calls.get() + 1);
            if matches!(action, Action::RetainAndFail) {
                probe
                    .retained
                    .replace(Some(context.retain_execution().unwrap()));
            }
            if let Some(gate) = gate {
                gate.await.unwrap();
            }
            probe.finished.set(probe.finished.get() + 1);
            match action {
                Action::Success => Ok(Ok(())),
                Action::Domain => Ok(Err(())),
                Action::Exhausted => Err(RuntimeFailure::ResourceExhausted {
                    capability: CAP,
                    operation: OP.to_owned(),
                }),
                Action::Fail | Action::RetainAndFail | Action::CancelAndFail => {
                    if matches!(action, Action::CancelAndFail) {
                        context.cancellation().cancel();
                    }
                    Err(RuntimeFailure::PluginFailure {
                        detail: "observed provider failure".into(),
                    })
                }
            }
        })
    }
}

#[derive(Debug)]
struct Endpoint {
    probe: Rc<Probe>,
    kind: Kind,
}

impl NativeRequestEndpoint for Endpoint {
    fn capability_id(&self) -> &'static str {
        CAP
    }
    fn descriptor_version(&self) -> &'static str {
        VERSION
    }
    fn operations(&self) -> &'static [&'static str] {
        &[OP]
    }
    fn invoke(
        &self,
        _: &str,
        request: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>> {
        let future = self
            .probe
            .execute(*request.downcast::<Action>().unwrap(), context);
        Box::pin(async move {
            future.await.map(|result| {
                result
                    .map(|()| Box::new(()) as Box<dyn Any>)
                    .map_err(|()| Box::new(()) as Box<dyn Any>)
            })
        })
    }
}

impl NativeStreamEndpoint for Endpoint {
    fn capability_id(&self) -> &'static str {
        CAP
    }
    fn descriptor_version(&self) -> &'static str {
        VERSION
    }
    fn operations(&self) -> &'static [&'static str] {
        &[OP]
    }
    fn open(
        &self,
        _: &str,
        request: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn NativeStreamSession>, Box<dyn Any>>, RuntimeFailure>,
    > {
        let action = *request.downcast::<Action>().unwrap();
        let probe = self.probe.clone();
        Box::pin(async move {
            // A successful open only constructs a session. Its operations own
            // their individual completion futures; no concurrent reads occur.
            if !matches!(action, Action::Success) {
                let _ = probe.execute(action, context.clone()).await?;
            }
            Ok(Ok(
                Box::new(Session { probe, context }) as Box<dyn NativeStreamSession>
            ))
        })
    }
}

#[derive(Debug)]
struct Session {
    probe: Rc<Probe>,
    context: InvocationContext,
}

impl NativeStreamSession for Session {
    fn send(&self, message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let future = self
            .probe
            .execute(*message.downcast::<Action>().unwrap(), self.context.clone());
        Box::pin(async move { future.await.map(|_| ()) })
    }
    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        let future = self.probe.execute(Action::Fail, self.context.clone());
        Box::pin(async move { future.await.map(|_| NativeStreamItem::Terminal(Ok(()))) })
    }
    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let future = self.probe.execute(Action::Fail, self.context.clone());
        Box::pin(async move { future.await.map(|_| ()) })
    }
    fn cancel(&self) {
        self.probe.cancels.set(self.probe.cancels.get() + 1);
    }
}

impl NativeEventEndpoint for Endpoint {
    fn capability_id(&self) -> &'static str {
        CAP
    }
    fn descriptor_version(&self) -> &'static str {
        VERSION
    }
    fn operations(&self) -> &'static [&'static str] {
        &[OP]
    }
    fn owns_event_admission(&self) -> bool {
        matches!(
            self.kind,
            Kind::Event {
                adapter_admission: true
            }
        )
    }
    fn publish(
        &self,
        _: &str,
        event: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        // The failure precedes endpoint admission acknowledgement. It is not
        // a subscriber handler error after acknowledged publication.
        let future = self
            .probe
            .execute(*event.downcast::<Action>().unwrap(), context);
        Box::pin(async move { future.await.map(|_| ()) })
    }
}

#[derive(Debug)]
struct Adapter {
    probe: Rc<Probe>,
    kind: Kind,
    recreates: Rc<Cell<usize>>,
}

impl Adapter {
    fn endpoint(&self) -> Rc<Endpoint> {
        Rc::new(Endpoint {
            probe: self.probe.clone(),
            kind: self.kind,
        })
    }
    fn plugin(&self, endpoint: Rc<Endpoint>) -> PreparedNativePlugin {
        match self.kind {
            Kind::Request => PreparedNativePlugin::new(vec![endpoint], NoopPluginLifecycle),
            Kind::Stream => {
                PreparedNativePlugin::with_stream_endpoints(vec![endpoint], NoopPluginLifecycle)
            }
            Kind::Event { .. } => {
                PreparedNativePlugin::with_event_endpoints(vec![endpoint], NoopPluginLifecycle)
            }
        }
    }
}

impl NativeExecutionAdapter for Adapter {
    fn supports_runtime_profile(&self, version: u32, profile: &str) -> bool {
        version == 2 && profile == "lenso.native-authoring@2"
    }
    fn prepare(&self, _: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let endpoint = self.endpoint();
        let bindings = if matches!(self.kind, Kind::Request) {
            vec![
                PreparedBinding::new("consumer", "provider", endpoint.clone())
                    .with_requirement_id("work"),
            ]
        } else {
            vec![]
        };
        let app = PreparedNativeApp::new(
            bindings,
            BTreeMap::from([
                (
                    "consumer".into(),
                    PreparedNativePlugin::new(vec![], NoopPluginLifecycle),
                ),
                ("provider".into(), self.plugin(endpoint.clone())),
            ]),
        );
        Ok(match self.kind {
            Kind::Request => app,
            Kind::Stream => app.with_stream_bindings(vec![
                PreparedStreamBinding::new("consumer", "provider", endpoint)
                    .with_requirement_id("work"),
            ]),
            Kind::Event { .. } => app.with_event_bindings(vec![
                PreparedEventBinding::new("consumer", "provider", endpoint)
                    .with_requirement_id("work"),
            ]),
        })
    }
    fn recreate(
        &self,
        _: &ResolvedAppPlan,
        _: &str,
    ) -> Result<PreparedNativePlugin, RuntimeFailure> {
        self.recreates.set(self.recreates.get() + 1);
        Ok(self.plugin(self.endpoint()))
    }
}

pub struct Harness {
    pub app: NativeApp,
    pub driver: DeterministicDriver,
    pub probe: Rc<Probe>,
    pub recreates: Rc<Cell<usize>>,
}

impl Harness {
    pub fn new(kind: Kind, gate: Option<oneshot::Receiver<()>>) -> Self {
        Self::with_policy(
            kind,
            gate,
            RestartPolicy::on_failure(
                1,
                Duration::from_secs(60),
                Duration::ZERO,
                Duration::ZERO,
                Duration::from_secs(60),
            ),
        )
    }
    pub fn with_policy(
        kind: Kind,
        gate: Option<oneshot::Receiver<()>>,
        policy: RestartPolicy,
    ) -> Self {
        let endpoint = CapabilityEndpointPlan::new(CAP, VERSION, [OP]);
        let endpoint = match kind {
            Kind::Request => endpoint,
            Kind::Stream => endpoint.with_stream_operation(OP),
            Kind::Event { .. } => endpoint.with_event_operation(OP).with_event_capacity(1),
        };
        let plan = AppComposition::new(
            vec![
                PluginInstancePlan::new("consumer", "test.consumer")
                    .with_authoring(2, "lenso.native-authoring@2")
                    .with_requirement(
                        CapabilityRequirementPlan::one(CAP, VERSION).with_requirement_id("work"),
                    ),
                PluginInstancePlan::new("provider", "test.provider")
                    .with_authoring(2, "lenso.native-authoring@2")
                    .with_restart_policy(policy)
                    .with_capability(endpoint),
            ],
            vec![
                CapabilityBinding::new("consumer", CAP, VERSION, "provider")
                    .with_requirement_id("work"),
            ],
        )
        .resolve()
        .unwrap();
        let probe = Rc::new(Probe {
            gate: RefCell::new(gate),
            ..Probe::default()
        });
        let recreates = Rc::new(Cell::new(0));
        let driver = DeterministicDriver::new();
        let app = driver
            .run(Kernel::start_native(
                plan,
                driver.clone(),
                Adapter {
                    probe: probe.clone(),
                    kind,
                    recreates: recreates.clone(),
                },
            ))
            .unwrap();
        Self {
            app,
            driver,
            probe,
            recreates,
        }
    }
    pub fn pump(&self) {
        self.driver.run(async {
            for _ in 0..8 {
                self.driver.yield_now().await;
            }
        });
    }
    pub fn assert_restarted_once(&self) {
        assert_eq!(
            self.recreates.get(),
            1,
            "execution owner must report PluginFailure once"
        );
        assert_eq!(self.app.plugin_generation("provider"), Some(2));
    }
}
