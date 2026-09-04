use std::{any::Any, cell::RefCell, collections::BTreeMap, rc::Rc};

use futures::future::{LocalBoxFuture, ready};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    EventAdmissionPlan, PluginInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    DeterministicDriver, DiagnosticEvent, EventAdmission, EventCapability, InvocationContext,
    NativeEventEndpoint, NativeExecutionAdapter, NoopPluginLifecycle, PreparedEventBinding,
    PreparedNativeApp, PreparedNativePlugin, RuntimeDriver, RuntimeFailure,
};

const CAPABILITY_ID: &str = "example.notifications@1";
const DESCRIPTOR_VERSION: &str = "1.0.0";
const OPERATION: &str = "notify";
const SECOND_OPERATION: &str = "audit";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Notification {
    sequence: u64,
}

#[derive(Debug)]
struct Notifications;

impl EventCapability for Notifications {
    type Event = Notification;

    const ID: &'static str = CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = DESCRIPTOR_VERSION;
}

#[derive(Debug)]
struct RecordingEndpoint {
    seen: Rc<RefCell<Vec<u64>>>,
    exhausted_sequence: Option<u64>,
}

#[derive(Debug)]
struct MultiOperationEndpoint;

impl NativeEventEndpoint for MultiOperationEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION, SECOND_OPERATION]
    }

    fn publish(
        &self,
        _operation: &str,
        _event: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(ready(Ok(())))
    }
}

#[derive(Debug)]
struct AcknowledgedAdmissionEndpoint {
    driver: DeterministicDriver,
    seen: Rc<RefCell<Vec<u64>>>,
}

#[derive(Debug)]
struct BlockingAdmissionEndpoint;

impl NativeEventEndpoint for BlockingAdmissionEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn owns_event_admission(&self) -> bool {
        true
    }

    fn publish(
        &self,
        _operation: &str,
        _event: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(futures::future::pending())
    }
}

impl NativeEventEndpoint for AcknowledgedAdmissionEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn owns_event_admission(&self) -> bool {
        true
    }

    fn publish(
        &self,
        _operation: &str,
        event: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        let event = event
            .downcast::<Notification>()
            .expect("the typed event should reach adapter admission");
        self.seen.borrow_mut().push(event.sequence);
        let driver = self.driver.clone();
        let acknowledgement = driver.now() + std::time::Duration::from_millis(20);
        Box::pin(async move {
            driver.sleep_until(acknowledgement).await;
            Ok(())
        })
    }
}

impl NativeEventEndpoint for RecordingEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn publish(
        &self,
        operation: &str,
        event: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        if operation != OPERATION {
            return Box::pin(ready(Err(RuntimeFailure::UnknownOperation {
                capability: CAPABILITY_ID,
                operation: operation.to_owned(),
            })));
        }
        let event = event
            .downcast::<Notification>()
            .expect("the typed event should cross the native endpoint seam");
        if self.exhausted_sequence == Some(event.sequence) {
            return Box::pin(ready(Err(RuntimeFailure::ResourceExhausted {
                capability: CAPABILITY_ID,
                operation: OPERATION.to_owned(),
            })));
        }
        self.seen.borrow_mut().push(event.sequence);
        Box::pin(ready(Ok(())))
    }
}

#[derive(Debug)]
struct EventAdapter {
    endpoints: BTreeMap<String, Rc<dyn NativeEventEndpoint>>,
}

impl NativeExecutionAdapter for EventAdapter {
    fn supports_runtime_profile(&self, version: u32, profile: &str) -> bool {
        version == 1 || (version == 2 && profile == "lenso.native-authoring@2")
    }

    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let mut generations = BTreeMap::new();
        let mut bindings = Vec::new();
        for (provider, endpoint) in &self.endpoints {
            generations.insert(
                provider.clone(),
                PreparedNativePlugin::with_event_endpoints(
                    vec![endpoint.clone()],
                    NoopPluginLifecycle,
                ),
            );
            let requirement_id = plan
                .capability_bindings()
                .iter()
                .find(|binding| {
                    binding.consumer_instance() == "consumer"
                        && binding.provider_instance() == provider
                        && binding.capability_id() == CAPABILITY_ID
                })
                .map(lenso_app_plan::CapabilityBinding::requirement_id)
                .expect("the prepared Event endpoint should have one Plan binding");
            bindings.push(
                PreparedEventBinding::new("consumer", provider, endpoint.clone())
                    .with_requirement_id(requirement_id),
            );
        }
        generations.insert(
            "consumer".to_owned(),
            PreparedNativePlugin::new(Vec::new(), NoopPluginLifecycle),
        );
        Ok(PreparedNativeApp::new(Vec::new(), generations).with_event_bindings(bindings))
    }
}

fn many_event_plan(provider_count: usize) -> ResolvedAppPlan {
    let mut instances = vec![
        PluginInstancePlan::new("consumer", "package.consumer").with_requirement(
            CapabilityRequirementPlan::many(CAPABILITY_ID, DESCRIPTOR_VERSION),
        ),
    ];
    let mut bindings = Vec::new();
    for index in 0..provider_count {
        let provider = format!("provider-{index}");
        let queue_capacity = if index == 0 { 1 } else { 2 };
        instances.push(
            PluginInstancePlan::new(&provider, "package.provider").with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, [OPERATION])
                    .with_event_operation(OPERATION)
                    .with_event_capacity(queue_capacity),
            ),
        );
        bindings.push(CapabilityBinding::new(
            "consumer",
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            provider,
        ));
    }
    AppComposition::new(instances, bindings)
        .resolve()
        .expect("the Event Composition should resolve")
}

fn authoring_v2_event_plan() -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer")
                .with_authoring(2, "lenso.native-authoring@2")
                .with_requirement(
                    CapabilityRequirementPlan::many(CAPABILITY_ID, DESCRIPTOR_VERSION)
                        .with_requirement_id("notifications"),
                ),
            PluginInstancePlan::new("provider", "package.provider")
                .with_authoring(2, "lenso.native-authoring@2")
                .with_capability(
                    CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, [OPERATION])
                        .with_event_operation(OPERATION)
                        .with_event_capacity(1),
                ),
        ],
        vec![
            CapabilityBinding::new("consumer", CAPABILITY_ID, DESCRIPTOR_VERSION, "provider")
                .with_requirement_id("notifications"),
        ],
    )
    .resolve()
    .expect("the authoring-v2 Event Composition should resolve")
}

#[test]
fn one_binding_shares_one_event_mailbox_across_operations() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::many(CAPABILITY_ID, DESCRIPTOR_VERSION),
            ),
            PluginInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new(
                    CAPABILITY_ID,
                    DESCRIPTOR_VERSION,
                    [OPERATION, SECOND_OPERATION],
                )
                .with_event_operation(OPERATION)
                .with_event_operation(SECOND_OPERATION)
                .with_event_admission(EventAdmissionPlan::new(1)),
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
    .expect("the multi-operation Event Composition should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(lenso_kernel::Kernel::start_native(
            plan,
            driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::from([(
                    "provider".to_owned(),
                    Rc::new(MultiOperationEndpoint) as Rc<dyn NativeEventEndpoint>,
                )]),
            },
        ))
        .expect("the multi-operation Event App should start");
    let handle = app
        .many_event_handle::<Notifications>("consumer")
        .expect("the Event handle should materialize");
    let observer = app
        .diagnostics()
        .subscribe_all(16)
        .expect("the diagnostics observer should be bounded");

    let first = driver.run(handle.publish(OPERATION, Notification { sequence: 1 }));
    let second = driver.run(handle.publish(SECOND_OPERATION, Notification { sequence: 2 }));

    assert_eq!(first[0].admission(), EventAdmission::Accepted);
    assert_eq!(second[0].admission(), EventAdmission::Exhausted);
    let records = std::iter::from_fn(|| observer.try_recv()).collect::<Vec<_>>();
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::EventAdmission {
                ref publisher_instance,
                ref subscriber_instance,
                outcome: lenso_kernel::DiagnosticAdmission::Accepted,
                ..
            } if publisher_instance == "consumer" && subscriber_instance == "provider"
        )
    }));
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::EventAdmission {
                ref publisher_instance,
                ref subscriber_instance,
                outcome: lenso_kernel::DiagnosticAdmission::Exhausted,
                ..
            } if publisher_instance == "consumer" && subscriber_instance == "provider"
        )
    }));
}

#[test]
fn zero_capacity_event_binding_never_accepts_a_publication() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::many(CAPABILITY_ID, DESCRIPTOR_VERSION),
            ),
            PluginInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, [OPERATION])
                    .with_event_operation(OPERATION)
                    .with_event_capacity(0),
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
    .expect("zero-capacity Event Composition should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(lenso_kernel::Kernel::start_native(
            plan,
            driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::from([(
                    "provider".to_owned(),
                    Rc::new(RecordingEndpoint {
                        seen: Rc::new(RefCell::new(Vec::new())),
                        exhausted_sequence: None,
                    }) as Rc<dyn NativeEventEndpoint>,
                )]),
            },
        ))
        .expect("zero-capacity Event App should start");
    let handle = app
        .many_event_handle::<Notifications>("consumer")
        .expect("the Event handle should materialize");

    let outcome = driver.run(handle.publish(OPERATION, Notification { sequence: 1 }));

    assert_eq!(outcome[0].admission(), EventAdmission::Exhausted);
}

#[test]
fn adapter_owned_admission_reports_the_acknowledged_commit_even_if_the_deadline_elapses() {
    let plan = many_event_plan(1);
    let driver = DeterministicDriver::new();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let app = driver
        .run(lenso_kernel::Kernel::start_native(
            plan,
            driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::from([(
                    "provider-0".to_owned(),
                    Rc::new(AcknowledgedAdmissionEndpoint {
                        driver: driver.clone(),
                        seen: seen.clone(),
                    }) as Rc<dyn NativeEventEndpoint>,
                )]),
            },
        ))
        .expect("the Adapter-owned Event App should start");
    let handle = app
        .many_event_handle::<Notifications>("consumer")
        .expect("the Event handle should materialize");
    let context = app.invocation_context_after(
        std::time::Duration::from_millis(10),
        lenso_kernel::CancellationToken::new(),
    );
    let advance = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            advance.yield_now().await;
            advance.advance(std::time::Duration::from_millis(10));
            advance.yield_now().await;
            advance.advance(std::time::Duration::from_millis(10));
        }))
        .expect("the deterministic Driver should accept the clock task");

    let outcome =
        driver.run(handle.publish_with_context(OPERATION, context, Notification { sequence: 1 }));

    assert_eq!(outcome[0].admission(), EventAdmission::Accepted);
    assert_eq!(&*seen.borrow(), &[1]);
}

#[test]
fn event_fan_out_attempts_every_binding_in_deterministic_order_and_preserves_fifo() {
    let first_seen = Rc::new(RefCell::new(Vec::new()));
    let second_seen = Rc::new(RefCell::new(Vec::new()));
    let app_driver = DeterministicDriver::new();
    let app = app_driver
        .run(lenso_kernel::Kernel::start_native(
            many_event_plan(2),
            app_driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::from([
                    (
                        "provider-0".to_owned(),
                        Rc::new(RecordingEndpoint {
                            seen: first_seen.clone(),
                            exhausted_sequence: Some(2),
                        }) as Rc<dyn NativeEventEndpoint>,
                    ),
                    (
                        "provider-1".to_owned(),
                        Rc::new(RecordingEndpoint {
                            seen: second_seen.clone(),
                            exhausted_sequence: None,
                        }) as Rc<dyn NativeEventEndpoint>,
                    ),
                ]),
            },
        ))
        .expect("the Event App should start");
    let handle = app
        .many_event_handle::<Notifications>("consumer")
        .expect("many Event handles should allow explicit bindings");
    assert_eq!(handle.binding_count(), 2);

    let first = app_driver.run(handle.publish(OPERATION, Notification { sequence: 1 }));
    assert_eq!(
        first
            .iter()
            .map(|result| (result.subscriber_instance(), result.admission()))
            .collect::<Vec<_>>(),
        vec![
            ("provider-0", EventAdmission::Accepted),
            ("provider-1", EventAdmission::Accepted),
        ]
    );
    assert!(first_seen.borrow().is_empty());

    let partial = app_driver.run(handle.publish(OPERATION, Notification { sequence: 2 }));
    assert_eq!(
        partial
            .iter()
            .map(|result| (result.subscriber_instance(), result.admission()))
            .collect::<Vec<_>>(),
        vec![
            ("provider-0", EventAdmission::Exhausted),
            ("provider-1", EventAdmission::Accepted),
        ]
    );

    app_driver.run(app_driver.yield_now());
    let third = app_driver.run(handle.publish(OPERATION, Notification { sequence: 3 }));
    assert!(
        third
            .iter()
            .all(|result| result.admission() == EventAdmission::Accepted)
    );
    app_driver.run(app_driver.yield_now());
    assert_eq!(&*first_seen.borrow(), &[1, 3]);
    assert_eq!(&*second_seen.borrow(), &[1, 2, 3]);
}

#[test]
fn many_event_handle_with_no_bindings_is_an_empty_success() {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(lenso_kernel::Kernel::start_native(
            many_event_plan(0),
            driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::new(),
            },
        ))
        .expect("an Event App without providers should start");
    let handle = app
        .many_event_handle::<Notifications>("consumer")
        .expect("many Event handles should allow zero bindings");
    assert_eq!(handle.binding_count(), 0);
    assert!(
        app.optional_event_handle::<Notifications>("consumer")
            .is_none()
    );
    assert!(
        driver
            .run(handle.publish(OPERATION, Notification { sequence: 1 },))
            .is_empty()
    );
}

#[test]
fn one_event_handle_materializes_one_explicit_subscriber() {
    let endpoint = Rc::new(RecordingEndpoint {
        seen: Rc::new(RefCell::new(Vec::new())),
        exhausted_sequence: None,
    }) as Rc<dyn NativeEventEndpoint>;
    let driver = DeterministicDriver::new();
    let app = driver
        .run(lenso_kernel::Kernel::start_native(
            many_event_plan(1),
            driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::from([("provider-0".to_owned(), endpoint)]),
            },
        ))
        .expect("the single-subscriber Event App should start");

    assert_eq!(
        app.event_handle::<Notifications>("consumer")
            .expect("one Event binding should materialize")
            .binding_count(),
        1
    );
    assert!(
        app.optional_event_handle::<Notifications>("consumer")
            .is_some_and(|handle| handle.binding_count() == 1)
    );
}

#[test]
fn dropped_authoring_v2_event_admission_remains_owned_until_settlement() {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(lenso_kernel::Kernel::start_native(
            authoring_v2_event_plan(),
            driver.clone(),
            EventAdapter {
                endpoints: BTreeMap::from([(
                    "provider".to_owned(),
                    Rc::new(BlockingAdmissionEndpoint) as Rc<dyn NativeEventEndpoint>,
                )]),
            },
        ))
        .expect("the authoring-v2 Event App should start");
    let handle = app
        .many_event_handle::<Notifications>("consumer")
        .expect("the named Event binding should resolve");
    let mut abandoned = Box::pin(handle.publish(OPERATION, Notification { sequence: 1 }));
    assert!(futures::FutureExt::now_or_never(abandoned.as_mut()).is_none());
    drop(abandoned);
    let advance = driver.clone();
    driver
        .spawn_local(Box::pin(async move {
            advance.yield_now().await;
            advance.advance(std::time::Duration::from_millis(10));
        }))
        .expect("the deterministic Driver should accept the clock task");

    assert_eq!(
        driver.run(app.shutdown(std::time::Duration::from_millis(10))),
        lenso_kernel::ShutdownOutcome::Timeout
    );
}
