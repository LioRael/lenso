use std::{any::Any, cell::RefCell, collections::BTreeMap, rc::Rc};

use futures::FutureExt;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    ActivateContext, DeactivateContext, DeactivationReason, DeterministicDriver, InvocationContext,
    Kernel, NativeExecutionAdapter, NativeRequestEndpoint, PluginLifecycle, PluginLifecyclePhase,
    PrepareContext, PreparedBinding, PreparedNativeApp, PreparedNativePlugin, RuntimeFailure,
};

#[derive(Debug)]
struct LifecycleEndpoint {
    capability: &'static str,
    operation: &'static [&'static str],
}

impl NativeRequestEndpoint for LifecycleEndpoint {
    fn capability_id(&self) -> &'static str {
        self.capability
    }

    fn descriptor_version(&self) -> &'static str {
        "1.0.0"
    }

    fn operations(&self) -> &'static [&'static str] {
        self.operation
    }

    fn invoke(
        &self,
        _operation: &str,
        _request: Box<dyn Any>,
        _context: InvocationContext,
    ) -> futures::future::LocalBoxFuture<
        'static,
        Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>,
    > {
        futures::future::ready(Ok(Ok(Box::new(()) as Box<dyn Any>))).boxed_local()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Prepare(String),
    IngressBound(String),
    Activate(String),
    IngressAccepted(String),
    BackgroundClaimed(String),
    Deactivate(String, DeactivationReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExternalWorkKind {
    None,
    Ingress,
    Background,
}

#[derive(Debug)]
struct RecordingLifecycle {
    instance_key: String,
    events: Rc<RefCell<Vec<Event>>>,
    external_observations: Rc<RefCell<Vec<bool>>>,
    fail_prepare: bool,
    fail_activate: bool,
    external_work: ExternalWorkKind,
}

impl PluginLifecycle for RecordingLifecycle {
    fn prepare(&self, context: PrepareContext) -> lenso_kernel::PluginFuture {
        assert_eq!(context.instance_key(), self.instance_key);
        assert_eq!(context.phase(), PluginLifecyclePhase::Prepare);
        assert_eq!(context.entrypoint(), format!("{}.entry", self.instance_key));
        assert_eq!(
            context.configuration(),
            format!("config:{}", self.instance_key)
        );
        let events = self.events.clone();
        let instance_key = self.instance_key.clone();
        let fail = self.fail_prepare;
        let ingress = self.external_work == ExternalWorkKind::Ingress;
        Box::pin(async move {
            events
                .borrow_mut()
                .push(Event::Prepare(instance_key.clone()));
            if ingress {
                events
                    .borrow_mut()
                    .push(Event::IngressBound(instance_key.clone()));
            }
            if fail {
                Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!("prepare failure for `{instance_key}`"),
                })
            } else {
                Ok(())
            }
        })
    }

    fn activate(&self, context: ActivateContext) -> lenso_kernel::PluginFuture {
        assert_eq!(context.instance_key(), self.instance_key);
        assert_eq!(context.phase(), PluginLifecyclePhase::Activate);
        assert!(!context.ready_gate().is_open());
        assert_eq!(context.readiness().phase(), PluginLifecyclePhase::Ready);

        if self.external_work != ExternalWorkKind::None {
            let readiness = context.readiness();
            let events = self.events.clone();
            let observations = self.external_observations.clone();
            let instance_key = self.instance_key.clone();
            let event = match self.external_work {
                ExternalWorkKind::Ingress => Event::IngressAccepted(instance_key.clone()),
                ExternalWorkKind::Background => Event::BackgroundClaimed(instance_key.clone()),
                ExternalWorkKind::None => unreachable!("None was filtered above"),
            };
            context
                .tasks()
                .spawn_local(Box::pin(async move {
                    readiness.wait().await;
                    observations.borrow_mut().push(readiness.is_open());
                    events.borrow_mut().push(event);
                }))
                .expect("the deterministic Driver should accept Plugin work");
        }

        let events = self.events.clone();
        let instance_key = self.instance_key.clone();
        let fail = self.fail_activate;
        Box::pin(async move {
            events
                .borrow_mut()
                .push(Event::Activate(instance_key.clone()));
            if fail {
                Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!("activate failure for `{instance_key}`"),
                })
            } else {
                Ok(())
            }
        })
    }

    fn deactivate(&self, context: DeactivateContext) -> lenso_kernel::PluginFuture {
        assert_eq!(context.instance_key(), self.instance_key);
        assert_eq!(context.phase(), PluginLifecyclePhase::Deactivate);
        assert_eq!(
            context.tasks().task_count(),
            0,
            "startup rollback must terminate generation tasks before deactivation"
        );
        let events = self.events.clone();
        let instance_key = self.instance_key.clone();
        let reason = context.reason();
        Box::pin(async move {
            events
                .borrow_mut()
                .push(Event::Deactivate(instance_key, reason));
            Ok(())
        })
    }
}

#[derive(Debug)]
struct RecordingAdapter {
    plugins: BTreeMap<String, Rc<dyn PluginLifecycle>>,
}

impl NativeExecutionAdapter for RecordingAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let alpha: Rc<dyn NativeRequestEndpoint> = Rc::new(LifecycleEndpoint {
            capability: "capability.alpha",
            operation: &["alpha.call"],
        });
        let beta: Rc<dyn NativeRequestEndpoint> = Rc::new(LifecycleEndpoint {
            capability: "capability.beta",
            operation: &["beta.call"],
        });
        let generations = self
            .plugins
            .iter()
            .map(|(instance_key, lifecycle)| {
                let endpoints = match instance_key.as_str() {
                    "z-provider" => vec![alpha.clone()],
                    "m-provider" => vec![beta.clone()],
                    _ => Vec::new(),
                };
                (
                    instance_key.clone(),
                    PreparedNativePlugin::with_lifecycle(endpoints, lifecycle.clone()),
                )
            })
            .collect();
        Ok(PreparedNativeApp::new(
            vec![
                PreparedBinding::new("m-provider", "z-provider", alpha),
                PreparedBinding::new("a-consumer", "m-provider", beta),
            ],
            generations,
        ))
    }
}

fn lifecycle_plan() -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            PluginInstancePlan::new("z-provider", "package.z")
                .with_entrypoint("z-provider.entry")
                .with_configuration("config:z-provider")
                .with_capability(CapabilityEndpointPlan::new(
                    "capability.alpha",
                    "1.0.0",
                    ["alpha.call"],
                )),
            PluginInstancePlan::new("m-provider", "package.m")
                .with_entrypoint("m-provider.entry")
                .with_configuration("config:m-provider")
                .with_capability(CapabilityEndpointPlan::new(
                    "capability.beta",
                    "1.0.0",
                    ["beta.call"],
                ))
                .with_requirement(CapabilityRequirementPlan::one("capability.alpha", "1.0.0")),
            PluginInstancePlan::new("a-consumer", "package.a")
                .with_entrypoint("a-consumer.entry")
                .with_configuration("config:a-consumer")
                .with_requirement(CapabilityRequirementPlan::one("capability.beta", "1.0.0")),
        ],
        vec![
            CapabilityBinding::new("m-provider", "capability.alpha", "1.0.0", "z-provider"),
            CapabilityBinding::new("a-consumer", "capability.beta", "1.0.0", "m-provider"),
        ],
    )
    .resolve()
    .expect("the lifecycle graph should resolve")
}

fn recording_adapter(
    events: &Rc<RefCell<Vec<Event>>>,
    external_observations: &Rc<RefCell<Vec<bool>>>,
    fail_prepare: Option<&str>,
    fail_activate: Option<&str>,
    ingress_instance: Option<&str>,
    background_instance: Option<&str>,
) -> RecordingAdapter {
    let plugins = ["z-provider", "m-provider", "a-consumer"]
        .into_iter()
        .map(|instance_key| {
            (
                instance_key.to_owned(),
                Rc::new(RecordingLifecycle {
                    instance_key: instance_key.to_owned(),
                    events: events.clone(),
                    external_observations: external_observations.clone(),
                    fail_prepare: fail_prepare == Some(instance_key),
                    fail_activate: fail_activate == Some(instance_key),
                    external_work: if ingress_instance == Some(instance_key) {
                        ExternalWorkKind::Ingress
                    } else if background_instance == Some(instance_key) {
                        ExternalWorkKind::Background
                    } else {
                        ExternalWorkKind::None
                    },
                }) as Rc<dyn PluginLifecycle>,
            )
        })
        .collect();
    RecordingAdapter { plugins }
}

#[test]
fn successful_startup_prepares_and_activates_in_dependency_order_and_opens_one_gate() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let external_observations = Rc::new(RefCell::new(Vec::new()));
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            lifecycle_plan(),
            driver.clone(),
            recording_adapter(
                &events,
                &external_observations,
                None,
                None,
                Some("z-provider"),
                Some("m-provider"),
            ),
        ))
        .expect("all Plugins should start");

    assert!(app.is_ready());
    assert!(app.ready_gate().is_open());
    assert_eq!(
        *events.borrow(),
        vec![
            Event::Prepare("z-provider".to_owned()),
            Event::IngressBound("z-provider".to_owned()),
            Event::Prepare("m-provider".to_owned()),
            Event::Prepare("a-consumer".to_owned()),
            Event::Activate("z-provider".to_owned()),
            Event::Activate("m-provider".to_owned()),
            Event::Activate("a-consumer".to_owned()),
            Event::IngressAccepted("z-provider".to_owned()),
            Event::BackgroundClaimed("m-provider".to_owned()),
        ]
    );
    assert_eq!(*external_observations.borrow(), vec![true, true]);
}

#[test]
fn prepare_failure_keeps_the_app_not_ready_and_rolls_back_prepared_instances_once() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let external_observations = Rc::new(RefCell::new(Vec::new()));
    let driver = DeterministicDriver::new();
    let outcome = driver.run(Kernel::start_native(
        lifecycle_plan(),
        driver.clone(),
        recording_adapter(
            &events,
            &external_observations,
            Some("m-provider"),
            None,
            Some("z-provider"),
            Some("m-provider"),
        ),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("prepare failure for `m-provider`")
    ));
    assert_eq!(
        *events.borrow(),
        vec![
            Event::Prepare("z-provider".to_owned()),
            Event::IngressBound("z-provider".to_owned()),
            Event::Prepare("m-provider".to_owned()),
            Event::Deactivate("m-provider".to_owned(), DeactivationReason::StartupRollback),
            Event::Deactivate("z-provider".to_owned(), DeactivationReason::StartupRollback),
        ]
    );
    assert!(external_observations.borrow().is_empty());
}

#[test]
fn activation_failure_never_opens_readiness_and_unwinds_all_prepared_instances_in_reverse_order() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let external_observations = Rc::new(RefCell::new(Vec::new()));
    let driver = DeterministicDriver::new();
    let outcome = driver.run(Kernel::start_native(
        lifecycle_plan(),
        driver.clone(),
        recording_adapter(
            &events,
            &external_observations,
            None,
            Some("m-provider"),
            Some("z-provider"),
            Some("m-provider"),
        ),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("activate failure for `m-provider`")
    ));
    assert_eq!(
        *events.borrow(),
        vec![
            Event::Prepare("z-provider".to_owned()),
            Event::IngressBound("z-provider".to_owned()),
            Event::Prepare("m-provider".to_owned()),
            Event::Prepare("a-consumer".to_owned()),
            Event::Activate("z-provider".to_owned()),
            Event::Activate("m-provider".to_owned()),
            Event::Deactivate("a-consumer".to_owned(), DeactivationReason::StartupRollback),
            Event::Deactivate("m-provider".to_owned(), DeactivationReason::StartupRollback),
            Event::Deactivate("z-provider".to_owned(), DeactivationReason::StartupRollback),
        ]
    );
    assert!(external_observations.borrow().is_empty());
}
