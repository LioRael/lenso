use std::{
    any::Any,
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Duration,
};

use futures::future::{LocalBoxFuture, ready};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ModuleInstancePlan, ResolvedAppPlan, RestartPolicy,
};
use lenso_kernel::{
    DeterministicDriver, InvocationContext, NativeExecutionAdapter, NativeStreamEndpoint,
    NativeStreamItem, NativeStreamSession, NoopModuleLifecycle, PreparedBinding, PreparedNativeApp,
    PreparedNativeModule, PreparedStreamBinding, RuntimeDriver, RuntimeFailure, StreamCapability,
    StreamEvent,
};

const CAPABILITY_ID: &str = "example.chat@1";
const DESCRIPTOR_VERSION: &str = "1.0.0";
const OPERATION: &str = "chat";

#[derive(Debug)]
struct Chat;

impl StreamCapability for Chat {
    type OpenRequest = String;
    type Message = String;
    type DomainError = String;

    const ID: &'static str = CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = DESCRIPTOR_VERSION;
}

#[derive(Debug, Default)]
struct SessionState {
    events: VecDeque<NativeStreamItem>,
    cancelled: bool,
}

#[derive(Debug)]
struct EchoSession {
    state: Rc<RefCell<SessionState>>,
}

impl NativeStreamSession for EchoSession {
    fn send(&self, message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let state = self.state.clone();
        Box::pin(ready(
            message
                .downcast::<String>()
                .map(|message| {
                    state
                        .borrow_mut()
                        .events
                        .push_back(NativeStreamItem::Message(message));
                })
                .map_err(|_| RuntimeFailure::ProtocolViolation {
                    capability: CAPABILITY_ID,
                }),
        ))
    }

    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        let event =
            self.state
                .borrow_mut()
                .events
                .pop_front()
                .ok_or_else(|| RuntimeFailure::Internal {
                    detail: "test stream has no queued event".to_owned(),
                });
        Box::pin(ready(event))
    }

    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        self.state
            .borrow_mut()
            .events
            .push_back(NativeStreamItem::PeerHalfClosed);
        self.state
            .borrow_mut()
            .events
            .push_back(NativeStreamItem::Terminal(Ok(())));
        Box::pin(ready(Ok(())))
    }

    fn cancel(&self) {
        self.state.borrow_mut().cancelled = true;
    }
}

#[derive(Debug)]
struct ChatEndpoint;

#[derive(Debug)]
struct CrashSession;

impl NativeStreamSession for CrashSession {
    fn send(&self, _message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(ready(Err(RuntimeFailure::ModuleFailure {
            detail: "test provider generation failed".to_owned(),
        })))
    }

    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        Box::pin(futures::future::pending())
    }

    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(futures::future::pending())
    }

    fn cancel(&self) {}
}

impl NativeStreamEndpoint for ChatEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn open(
        &self,
        operation: &str,
        request: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn NativeStreamSession>, Box<dyn Any>>, RuntimeFailure>,
    > {
        if operation != OPERATION {
            return Box::pin(ready(Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            })));
        }
        let request = request
            .downcast::<String>()
            .expect("the generated open request should cross the native seam");
        if *request == "domain-error" {
            let error: Box<dyn Any> = Box::new("room_closed".to_owned());
            return Box::pin(ready(Ok(Err(error))));
        }
        if *request == "crash-session" {
            return Box::pin(ready(Ok(Ok(
                Box::new(CrashSession) as Box<dyn NativeStreamSession>
            ))));
        }
        let _ = request;
        Box::pin(ready(Ok(Ok(Box::new(EchoSession {
            state: Rc::new(RefCell::new(SessionState::default())),
        }) as Box<dyn NativeStreamSession>))))
    }
}

#[derive(Debug)]
struct BlockingSession;

impl NativeStreamSession for BlockingSession {
    fn send(&self, _message: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(futures::future::pending())
    }

    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        Box::pin(futures::future::pending())
    }

    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(futures::future::pending())
    }

    fn cancel(&self) {}
}

#[derive(Debug)]
struct BlockingEndpoint;

impl NativeStreamEndpoint for BlockingEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn open(
        &self,
        operation: &str,
        _request: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn NativeStreamSession>, Box<dyn Any>>, RuntimeFailure>,
    > {
        if operation != OPERATION {
            return Box::pin(ready(Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            })));
        }
        Box::pin(ready(Ok(Ok(
            Box::new(BlockingSession) as Box<dyn NativeStreamSession>
        ))))
    }
}

#[derive(Debug)]
struct StreamAdapter {
    endpoint: Rc<dyn NativeStreamEndpoint>,
}

impl NativeExecutionAdapter for StreamAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let endpoint = self.endpoint.clone();
        Ok(PreparedNativeApp::new(
            Vec::<PreparedBinding>::new(),
            BTreeMap::from([
                (
                    "consumer".to_owned(),
                    PreparedNativeModule::new(Vec::new(), NoopModuleLifecycle),
                ),
                (
                    "provider".to_owned(),
                    PreparedNativeModule::with_stream_endpoints(
                        vec![endpoint.clone()],
                        NoopModuleLifecycle,
                    ),
                ),
            ]),
        )
        .with_stream_bindings(vec![PreparedStreamBinding::new(
            "consumer", "provider", endpoint,
        )]))
    }

    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        if instance_key != "provider" {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!("unexpected recreation for `{instance_key}`"),
            });
        }
        Ok(PreparedNativeModule::with_stream_endpoints(
            vec![self.endpoint.clone()],
            NoopModuleLifecycle,
        ))
    }
}

fn plan(queue_capacity: usize, max_concurrency: usize) -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::one(CAPABILITY_ID, DESCRIPTOR_VERSION),
            ),
            ModuleInstancePlan::new("provider", "package.provider")
                .with_restart_policy(RestartPolicy::on_failure(
                    2,
                    Duration::from_secs(1),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                ))
                .with_capability(
                    CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, [OPERATION])
                        .with_stream_operation(OPERATION)
                        .with_limits(queue_capacity, max_concurrency),
                ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            "provider",
        )],
    )
    .resolve()
    .expect("the stream Composition should resolve")
}

fn stream_app(
    queue_capacity: usize,
    max_concurrency: usize,
) -> (lenso_kernel::NativeApp, DeterministicDriver) {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(queue_capacity, max_concurrency),
            driver.clone(),
            StreamAdapter {
                endpoint: Rc::new(ChatEndpoint),
            },
        ))
        .expect("the stream App should start");
    (app, driver)
}

use lenso_kernel::Kernel;

#[test]
fn stream_is_full_duplex_and_half_close_is_independent() {
    let (app, driver) = stream_app(0, 1);
    let handle = app
        .stream_handle::<Chat>("consumer")
        .expect("the stream binding should resolve");
    let stream = driver
        .run(handle.open(OPERATION, "room-1".to_owned()))
        .expect("stream open should not fail")
        .expect("stream open should not return a Domain Error");

    driver
        .run(stream.send("hello".to_owned()))
        .expect("send should succeed");
    assert_eq!(
        driver.run(stream.receive()).expect("message should arrive"),
        StreamEvent::Message("hello".to_owned())
    );

    driver
        .run(stream.close_send())
        .expect("local half-close should succeed");
    assert_eq!(
        driver
            .run(stream.receive())
            .expect("peer half-close should arrive"),
        StreamEvent::PeerHalfClosed
    );
    assert_eq!(
        driver
            .run(stream.receive())
            .expect("terminal should arrive after half-close"),
        StreamEvent::Terminal(Ok(()))
    );
    assert!(matches!(
        driver.run(stream.receive()),
        Err(RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID
        })
    ));
}

#[test]
fn stream_open_preserves_domain_errors_and_bounded_admission() {
    let (app, driver) = stream_app(0, 1);
    let handle = app
        .stream_handle::<Chat>("consumer")
        .expect("the stream binding should resolve");
    let first = driver
        .run(handle.open(OPERATION, "room-1".to_owned()))
        .expect("stream open should not fail")
        .expect("first stream should open");
    assert_eq!(handle.binding_count(), 1);

    assert!(matches!(
        driver.run(handle.open(OPERATION, "room-2".to_owned())),
        Err(RuntimeFailure::ResourceExhausted {
            capability: CAPABILITY_ID,
            operation
        }) if operation == OPERATION
    ));

    drop(first);
    assert!(matches!(
        driver
            .run(handle.open(OPERATION, "domain-error".to_owned()))
            .expect("Domain Error is not a Runtime Failure"),
        Err(error) if error == "room_closed"
    ));
}

#[test]
fn stream_cancel_is_idempotent_and_completes_as_cancelled() {
    let (app, driver) = stream_app(0, 1);
    let handle = app
        .stream_handle::<Chat>("consumer")
        .expect("the stream binding should resolve");
    let stream = driver
        .run(handle.open(OPERATION, "room-1".to_owned()))
        .expect("stream open should not fail")
        .expect("stream should open");
    let request_id = stream.request_id();
    stream.cancel();
    stream.cancel();

    assert!(matches!(
        driver.run(stream.send("late".to_owned())),
        Err(RuntimeFailure::Cancelled {
            request_id: cancelled_request_id
        }) if cancelled_request_id == request_id
    ));
    assert!(matches!(
        driver.run(stream.receive()),
        Err(RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID
        })
    ));
    assert!(matches!(
        driver.run(stream.close_send()),
        Err(RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID
        })
    ));
}

#[test]
fn blocked_stream_operations_observe_cancellation_and_deadline() {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan(0, 2),
            driver.clone(),
            StreamAdapter {
                endpoint: Rc::new(BlockingEndpoint),
            },
        ))
        .expect("the blocking stream App should start");
    let handle = app
        .stream_handle::<Chat>("consumer")
        .expect("the blocking stream binding should resolve");

    let cancellation = lenso_kernel::CancellationToken::new();
    let context = app.invocation_context(None, cancellation.clone());
    let stream = driver
        .run(handle.open_with_context(OPERATION, context, "room".to_owned()))
        .expect("blocking stream open should not fail")
        .expect("blocking stream open should succeed");
    let send = stream.send("cancelled".to_owned());
    let cancel = async {
        driver.yield_now().await;
        cancellation.cancel();
    };
    let cancelled = driver.run(async { futures::future::join(send, cancel).await.0 });
    assert!(matches!(cancelled, Err(RuntimeFailure::Cancelled { .. })));
    assert!(matches!(
        driver.run(stream.receive()),
        Err(RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID
        })
    ));
    drop(stream);

    let stream = driver
        .run(handle.open(OPERATION, "room".to_owned()))
        .expect("explicitly cancelled stream open should not fail")
        .expect("explicitly cancelled stream open should succeed");
    let send = stream.send("explicitly cancelled".to_owned());
    let cancel = async {
        driver.yield_now().await;
        stream.cancel();
    };
    let cancelled = driver.run(async { futures::future::join(send, cancel).await.0 });
    assert!(matches!(cancelled, Err(RuntimeFailure::Cancelled { .. })));
    assert!(matches!(
        driver.run(stream.receive()),
        Err(RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID
        })
    ));
    drop(stream);

    let deadline_context = app.invocation_context_after(
        std::time::Duration::from_millis(10),
        lenso_kernel::CancellationToken::new(),
    );
    let stream = driver
        .run(handle.open_with_context(OPERATION, deadline_context, "room".to_owned()))
        .expect("deadline stream open should not fail")
        .expect("deadline stream open should succeed");
    let send = stream.send("deadline".to_owned());
    let advance = async {
        driver.yield_now().await;
        driver.advance(std::time::Duration::from_millis(10));
    };
    let deadline = driver.run(async { futures::future::join(send, advance).await.0 });
    assert!(matches!(
        deadline,
        Err(RuntimeFailure::DeadlineExceeded { .. })
    ));
    assert!(matches!(
        driver.run(stream.receive()),
        Err(RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID
        })
    ));
}

#[test]
fn established_stream_stays_bound_to_its_opening_generation() {
    let (app, driver) = stream_app(0, 2);
    let handle = app
        .stream_handle::<Chat>("consumer")
        .expect("the stream binding should resolve");
    let existing = driver
        .run(handle.open(OPERATION, "room-1".to_owned()))
        .expect("existing stream open should not fail")
        .expect("existing stream should open");
    driver
        .run(existing.send("queued-before-restart".to_owned()))
        .expect("existing generation should queue a message");
    let crashing = driver
        .run(handle.open(OPERATION, "crash-session".to_owned()))
        .expect("crashing stream open should not fail")
        .expect("crashing stream should open");
    assert!(matches!(
        driver.run(crashing.send("crash".to_owned())),
        Err(RuntimeFailure::ModuleFailure { .. })
    ));
    drop(crashing);

    for _ in 0..10 {
        driver.run(driver.yield_now());
        if app.module_generation("provider") == Some(2) {
            break;
        }
    }
    assert_eq!(app.module_generation("provider"), Some(2));
    assert!(matches!(
        driver.run(existing.receive()),
        Err(RuntimeFailure::Unavailable {
            capability: CAPABILITY_ID
        })
    ));
    assert!(
        driver
            .run(handle.open(OPERATION, "room-2".to_owned()))
            .expect("the stable handle should use the new generation")
            .is_ok()
    );
}

#[test]
fn provider_failure_terminates_only_the_nested_stream() {
    let (app, driver) = stream_app(0, 1);
    let handle = app
        .stream_handle::<Chat>("consumer")
        .expect("the stream binding should resolve");
    let caller_cancellation = lenso_kernel::CancellationToken::new();
    let context = app.invocation_context(None, caller_cancellation.clone());
    let stream = driver
        .run(handle.open_with_context(OPERATION, context, "crash-session".to_owned()))
        .expect("crashing stream open should not fail")
        .expect("crashing stream should open");

    assert!(matches!(
        driver.run(stream.send("crash".to_owned())),
        Err(RuntimeFailure::ModuleFailure { .. })
    ));
    assert!(
        !caller_cancellation.is_cancelled(),
        "a nested provider failure must not cancel its caller's shared invocation context"
    );
}
