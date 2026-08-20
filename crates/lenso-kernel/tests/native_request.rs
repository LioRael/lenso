use lenso_app_plan::{
    CapabilityBinding, CapabilityEndpointPlan, ModuleInstancePlan, ResolvedAppPlan,
};
use lenso_capability_greeting::{
    GREETING_CAPABILITY_ID, GREETING_DESCRIPTOR_VERSION, GreetError, GreetRequest, GreetingClient,
    GreetingInvocationError,
};
use lenso_kernel::{DeterministicDriver, Kernel, RuntimeFailure};
use lenso_native_adapter::NativeModuleRegistry;
use lenso_native_greeter::{
    CONSUMER_PACKAGE_ID, ConsumerFactory, GREETER_PACKAGE_ID, GreeterFactory,
};

fn greeting_app() -> (lenso_kernel::NativeApp, DeterministicDriver) {
    let plan = ResolvedAppPlan::new(
        vec![
            ModuleInstancePlan::new("greeter", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
                ),
            ),
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            GREETING_CAPABILITY_ID,
            GREETING_DESCRIPTOR_VERSION,
            "greeter",
        )],
    );
    let registry = NativeModuleRegistry::new()
        .with_factory(GreeterFactory)
        .with_factory(ConsumerFactory);
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), registry))
        .expect("the native App should start");
    (app, driver)
}

#[test]
fn generated_client_invokes_a_statically_linked_provider() {
    let (app, driver) = greeting_app();
    let client = GreetingClient::new(&app, "consumer").expect("binding should resolve");

    let response = driver.run(client.greet(GreetRequest {
        name: "Ada".to_owned(),
    }));

    assert_eq!(response.unwrap().message, "Hello, Ada!");
}

#[test]
fn generated_client_preserves_domain_errors() {
    let (app, driver) = greeting_app();
    let client = GreetingClient::new(&app, "consumer").expect("binding should resolve");

    let outcome = driver.run(client.greet(GreetRequest {
        name: String::new(),
    }));

    assert_eq!(
        outcome,
        Err(GreetingInvocationError::Domain(GreetError::EmptyName))
    );
}

#[test]
fn kernel_rejects_an_unknown_operation_as_a_runtime_failure() {
    let (app, driver) = greeting_app();

    let outcome = driver.run(app.invoke::<lenso_capability_greeting::Greeting>(
        "consumer",
        "missing.operation",
        GreetRequest {
            name: "Ada".to_owned(),
        },
    ));

    assert_eq!(
        outcome,
        Err(RuntimeFailure::UnknownOperation {
            capability: GREETING_CAPABILITY_ID,
            operation: "missing.operation".to_owned(),
        })
    );
}

#[test]
fn generated_client_reports_a_missing_binding_as_a_runtime_failure() {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            ResolvedAppPlan::new(vec![], vec![]),
            driver.clone(),
            NativeModuleRegistry::new(),
        ))
        .expect("an App without providers can start");

    let error = GreetingClient::new(&app, "consumer").unwrap_err();

    assert_eq!(
        error,
        RuntimeFailure::Unavailable {
            capability: GREETING_CAPABILITY_ID,
        }
    );
}

#[test]
fn kernel_rejects_a_planned_module_without_a_linked_factory() {
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::start_native(
        ResolvedAppPlan::new(
            vec![ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID)],
            vec![],
        ),
        driver.clone(),
        NativeModuleRegistry::new(),
    ));

    assert_eq!(
        outcome.unwrap_err(),
        RuntimeFailure::MissingModuleFactory {
            instance: "consumer".to_owned(),
            package_id: CONSUMER_PACKAGE_ID.to_owned(),
        }
    );
}

#[test]
fn kernel_rejects_a_native_plan_with_an_unsupported_schema() {
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::start_native(
        ResolvedAppPlan::with_schema_version(0),
        driver.clone(),
        NativeModuleRegistry::new(),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("unsupported Plan schema version 0")
    ));
}

#[test]
fn native_adapter_rejects_an_operation_table_mismatch_during_preparation() {
    let driver = DeterministicDriver::new();
    let plan = ResolvedAppPlan::new(
        vec![
            ModuleInstancePlan::new("greeter", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    ["undeclared.operation"],
                ),
            ),
        ],
        vec![],
    );

    let outcome = driver.run(Kernel::start_native(
        plan,
        driver.clone(),
        NativeModuleRegistry::new().with_factory(GreeterFactory),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("differs from its resolved Descriptor")
    ));
}
