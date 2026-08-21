use std::time::Duration;

use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ModuleInstancePlan, ResolvedAppPlan,
};
use lenso_capability_greeting::{
    GREET_OPERATION, GREETING_CAPABILITY_ID, GREETING_DESCRIPTOR_VERSION, GreetRequest, Greeting,
};
use lenso_kernel::{
    DeterministicDriver, DiagnosticEvent, DiagnosticFilter, DiagnosticOutcome, DiagnosticSource,
    DiagnosticSubscribeError, ExecutionAdapterCatalog, Kernel, RuntimeDiagnostics, RuntimeDriver,
    RuntimeFailureKind, ShutdownOutcome,
};
use lenso_native_adapter::NativeModuleRegistry;
use lenso_native_greeter::{
    CONSUMER_PACKAGE_ID, ConsumerFactory, GREETER_PACKAGE_ID, GreeterFactory,
};

fn greeting_plan() -> ResolvedAppPlan {
    AppComposition::new(
        vec![
            ModuleInstancePlan::new("greeter", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [GREET_OPERATION],
                ),
            ),
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    CapabilityCardinality::One,
                ),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            GREETING_CAPABILITY_ID,
            GREETING_DESCRIPTOR_VERSION,
            "greeter",
        )],
    )
    .resolve()
    .expect("the greeting Plan should resolve")
}

#[test]
fn diagnostics_filter_sources_and_drop_overflow_without_affecting_shutdown() {
    let driver = DeterministicDriver::new();
    let diagnostics = RuntimeDiagnostics::new();
    let lifecycle = diagnostics
        .subscribe(DiagnosticFilter::only(DiagnosticSource::Lifecycle), 1)
        .expect("a positive observer capacity should be accepted");
    let invocation = diagnostics
        .subscribe(DiagnosticFilter::only(DiagnosticSource::Invocation), 1)
        .expect("a positive observer capacity should be accepted");

    let app = driver
        .run(Kernel::start_with_diagnostics(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
            diagnostics,
        ))
        .expect("the empty App should start");

    assert!(app.is_ready());
    assert!(lifecycle.try_recv().is_some());
    assert!(lifecycle.dropped_count() > 0);
    assert!(invocation.try_recv().is_none());
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn observer_can_await_the_next_record() {
    let driver = DeterministicDriver::new();
    let diagnostics = RuntimeDiagnostics::new();
    let mut observer = diagnostics
        .subscribe_all(8)
        .expect("the diagnostics observer should be bounded");

    let start_driver = driver.clone();
    let start_diagnostics = diagnostics.clone();
    let (record, app) = driver.run(async {
        futures::future::join(observer.recv(), async move {
            start_driver.yield_now().await;
            Kernel::start_with_diagnostics(
                ResolvedAppPlan::empty(),
                start_driver.clone(),
                ExecutionAdapterCatalog::new(),
                start_diagnostics,
            )
            .await
            .expect("the empty App should start")
        })
        .await
    });

    assert!(record.is_some());
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn zero_observers_do_not_change_empty_app_behavior() {
    let driver = DeterministicDriver::new();
    let diagnostics = RuntimeDiagnostics::new();

    let app = driver
        .run(Kernel::start_with_diagnostics(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
            diagnostics.clone(),
        ))
        .expect("the empty App should start without observers");

    assert_eq!(diagnostics.observer_count(), 0);
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn observer_disconnect_and_zero_capacity_are_non_fatal() {
    let diagnostics = RuntimeDiagnostics::new();
    assert!(matches!(
        diagnostics.subscribe_all(0),
        Err(DiagnosticSubscribeError::ZeroCapacity)
    ));

    let observer = diagnostics
        .subscribe_all(2)
        .expect("a positive observer capacity is required");
    assert_eq!(diagnostics.observer_count(), 1);
    drop(observer);
    assert_eq!(diagnostics.observer_count(), 0);

    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_with_diagnostics(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
            diagnostics,
        ))
        .expect("an observer disconnect must not affect App startup");

    assert!(app.is_ready());
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn shutdown_records_the_actual_admission_and_cleanup_boundaries() {
    let driver = DeterministicDriver::new();
    let diagnostics = RuntimeDiagnostics::new();
    let observer = diagnostics
        .subscribe_all(32)
        .expect("the diagnostics observer should be bounded");
    let app = driver
        .run(Kernel::start_with_diagnostics(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
            diagnostics,
        ))
        .expect("the empty App should start");

    app.request_shutdown();
    driver.advance(Duration::from_millis(10));
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );

    let records = std::iter::from_fn(|| observer.try_recv()).collect::<Vec<_>>();
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record.event, DiagnosticEvent::ShutdownAdmissionClosed))
            .count(),
        1
    );
    assert!(records.iter().any(|record| {
        record.timestamp == Duration::from_millis(10)
            && matches!(
                record.event,
                DiagnosticEvent::ShutdownCompleted {
                    elapsed: Duration::ZERO,
                    ..
                }
            )
    }));
    assert!(records.iter().any(|record| {
        record.timestamp == Duration::ZERO
            && matches!(record.event, DiagnosticEvent::ShutdownAdmissionClosed)
    }));
    assert!(records.iter().any(|record| {
        record.timestamp == Duration::from_millis(10)
            && matches!(record.event, DiagnosticEvent::ShutdownCleanupStarted { .. })
    }));
    assert_eq!(
        records
            .iter()
            .filter(|record| matches!(record.event, DiagnosticEvent::ShutdownCompleted { .. }))
            .count(),
        1
    );
}

#[test]
fn diagnostics_do_not_treat_unresolved_caller_text_as_structural_identity() {
    let driver = DeterministicDriver::new();
    let diagnostics = RuntimeDiagnostics::new();
    let observer = diagnostics
        .subscribe_all(32)
        .expect("the diagnostics observer should be bounded");
    let app = driver
        .run(Kernel::start_with_diagnostics(
            ResolvedAppPlan::empty(),
            driver.clone(),
            ExecutionAdapterCatalog::new(),
            diagnostics,
        ))
        .expect("the empty App should start");

    let caller_text = "not-in-the-plan: secret-value";
    assert!(app.ensure_binding::<Greeting>(caller_text).is_err());
    let result = driver.run(
        app.many_handle::<Greeting>(caller_text)
            .expect("many requirements may have no providers")
            .invoke_many(
                GREET_OPERATION,
                GreetRequest {
                    name: "Ada".to_owned(),
                },
            ),
    );
    assert!(matches!(result, Ok(ref responses) if responses.is_empty()));

    let records = std::iter::from_fn(|| observer.try_recv()).collect::<Vec<_>>();
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::RuntimeFailure { instance: None, .. }
        )
    }));
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::InvocationStarted {
                caller_instance: None,
                ..
            }
        )
    }));
    assert!(records.iter().all(|record| match &record.event {
        DiagnosticEvent::RuntimeFailure { instance, .. } =>
            instance.as_deref() != Some(caller_text),
        DiagnosticEvent::InvocationStarted {
            caller_instance, ..
        }
        | DiagnosticEvent::InvocationCompleted {
            caller_instance, ..
        } => caller_instance.as_deref() != Some(caller_text),
        _ => true,
    }));

    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[test]
fn request_diagnostics_expose_timing_and_failure_categories_without_domain_bodies() {
    let driver = DeterministicDriver::new();
    let diagnostics = RuntimeDiagnostics::new();
    let observer = diagnostics
        .subscribe_all(64)
        .expect("observer capacity is positive");
    let app = driver
        .run(Kernel::start_native_with_diagnostics(
            greeting_plan(),
            driver.clone(),
            NativeModuleRegistry::new()
                .with_factory(GreeterFactory)
                .with_factory(ConsumerFactory),
            diagnostics,
        ))
        .expect("the greeting App should start");

    let success = driver.run(app.invoke::<Greeting>(
        "consumer",
        GREET_OPERATION,
        GreetRequest {
            name: "Ada".to_owned(),
        },
    ));
    assert!(success.is_ok());

    let domain_error = driver.run(app.invoke::<Greeting>(
        "consumer",
        GREET_OPERATION,
        GreetRequest {
            name: String::new(),
        },
    ));
    assert!(matches!(domain_error, Ok(Err(_))));

    let unknown = driver.run(app.invoke::<Greeting>(
        "consumer",
        "unknown.operation",
        GreetRequest {
            name: "Ada".to_owned(),
        },
    ));
    assert!(unknown.is_err());

    let records = std::iter::from_fn(|| observer.try_recv()).collect::<Vec<_>>();
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::InvocationCompleted {
                outcome: DiagnosticOutcome::Succeeded,
                elapsed,
                ..
            } if elapsed == Duration::ZERO
        )
    }));
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::InvocationCompleted {
                outcome: DiagnosticOutcome::DomainError,
                ..
            }
        )
    }));
    assert!(records.iter().any(|record| {
        matches!(
            record.event,
            DiagnosticEvent::RuntimeFailure {
                kind: RuntimeFailureKind::UnknownOperation,
                ..
            }
        )
    }));

    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}
