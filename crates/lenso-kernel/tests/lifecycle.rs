use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ModuleInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    ActivateContext, DeactivateContext, DeactivationReason, DeterministicDriver, Kernel,
    ModuleLifecycle, ModuleLifecyclePhase, NativeExecutionAdapter, PrepareContext,
    PreparedNativeApp, RuntimeFailure,
};

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

impl ModuleLifecycle for RecordingLifecycle {
    fn prepare(&self, context: PrepareContext) -> lenso_kernel::ModuleFuture {
        assert_eq!(context.instance_key(), self.instance_key);
        assert_eq!(context.phase(), ModuleLifecyclePhase::Prepare);
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

    fn activate(&self, context: ActivateContext) -> lenso_kernel::ModuleFuture {
        assert_eq!(context.instance_key(), self.instance_key);
        assert_eq!(context.phase(), ModuleLifecyclePhase::Activate);
        assert!(!context.ready_gate().is_open());
        assert_eq!(context.readiness().phase(), ModuleLifecyclePhase::Ready);

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
                .expect("the deterministic Driver should accept Module work");
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

    fn deactivate(&self, context: DeactivateContext) -> lenso_kernel::ModuleFuture {
        assert_eq!(context.instance_key(), self.instance_key);
        assert_eq!(context.phase(), ModuleLifecyclePhase::Deactivate);
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
    modules: BTreeMap<String, Rc<dyn ModuleLifecycle>>,
}

impl NativeExecutionAdapter for RecordingAdapter {
    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        Ok(PreparedNativeApp::with_modules(
            BTreeMap::new(),
            self.modules.clone(),
        ))
    }
}

fn lifecycle_plan() -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            ModuleInstancePlan::new("z-provider", "package.z").with_capability(
                CapabilityEndpointPlan::new("capability.alpha", "1.0.0", ["alpha.call"]),
            ),
            ModuleInstancePlan::new("m-provider", "package.m")
                .with_capability(CapabilityEndpointPlan::new(
                    "capability.beta",
                    "1.0.0",
                    ["beta.call"],
                ))
                .with_requirement(CapabilityRequirementPlan::one("capability.alpha", "1.0.0")),
            ModuleInstancePlan::new("a-consumer", "package.a")
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
    let modules = ["z-provider", "m-provider", "a-consumer"]
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
                }) as Rc<dyn ModuleLifecycle>,
            )
        })
        .collect();
    RecordingAdapter { modules }
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
        .expect("all Modules should start");

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
