use std::time::Duration;

use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ModuleInstancePlan, PlanResolutionError, ResolvedAppPlan,
    RestartPolicy,
};
use lenso_capability_greeting::{
    GREETING_CAPABILITY_ID, GREETING_DESCRIPTOR_VERSION, GreetError, GreetRequest, Greeting,
    GreetingClient, GreetingInvocationError,
};
use lenso_kernel::{DeterministicDriver, Kernel, RuntimeDriver, RuntimeFailure};
use lenso_native_adapter::NativeModuleRegistry;
use lenso_native_greeter::{
    ALTERNATE_GREETER_PACKAGE_ID, AlternateGreeterFactory, CONSUMER_PACKAGE_ID, ConsumerFactory,
    GREETER_PACKAGE_ID, GreeterFactory,
};

fn greeting_composition(provider_package_id: &str) -> AppComposition {
    AppComposition::new(
        vec![
            ModuleInstancePlan::new("greeter", provider_package_id).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
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
}

fn greeting_registry() -> NativeModuleRegistry {
    NativeModuleRegistry::new()
        .with_factory(GreeterFactory)
        .with_factory(AlternateGreeterFactory)
        .with_factory(ConsumerFactory)
}

fn greeting_app(provider_package_id: &str) -> (lenso_kernel::NativeApp, DeterministicDriver) {
    let plan = greeting_composition(provider_package_id)
        .resolve()
        .expect("the greeting Composition should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            greeting_registry(),
        ))
        .expect("the native App should start");
    (app, driver)
}

#[test]
fn generated_client_invokes_a_statically_linked_provider() {
    let (app, driver) = greeting_app(GREETER_PACKAGE_ID);
    let client = GreetingClient::new(
        app.handle::<Greeting>("consumer")
            .expect("binding should resolve"),
    );

    let response = driver.run(client.greet(GreetRequest {
        name: "Ada".to_owned(),
    }));

    assert_eq!(response.unwrap().message, "Hello, Ada!");
}

#[test]
fn generated_client_preserves_domain_errors() {
    let (app, driver) = greeting_app(GREETER_PACKAGE_ID);
    let client = GreetingClient::new(
        app.handle::<Greeting>("consumer")
            .expect("binding should resolve"),
    );

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
    let (app, driver) = greeting_app(GREETER_PACKAGE_ID);

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

    let error = app.handle::<Greeting>("consumer").unwrap_err();

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

#[test]
fn composition_materializes_keyed_instances_requirements_and_deterministic_many_bindings() {
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    CapabilityCardinality::Many,
                ),
            ),
            ModuleInstancePlan::new("provider-z", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
                ),
            ),
            ModuleInstancePlan::new("provider-a", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
                ),
            ),
        ],
        vec![
            CapabilityBinding::new(
                "consumer",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "provider-z",
            ),
            CapabilityBinding::new(
                "consumer",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "provider-a",
            ),
        ],
    );

    let plan = composition.resolve().expect("many binding should resolve");

    assert_eq!(
        plan.module_instances()
            .iter()
            .map(ModuleInstancePlan::instance_key)
            .collect::<Vec<_>>(),
        ["consumer", "provider-a", "provider-z"]
    );
    let consumer = &plan.module_instances()[0];
    assert_eq!(
        consumer.required_capabilities()[0].cardinality(),
        CapabilityCardinality::Many
    );
    assert_eq!(
        plan.capability_bindings()
            .iter()
            .map(CapabilityBinding::provider_instance)
            .collect::<Vec<_>>(),
        ["provider-a", "provider-z"]
    );
}

#[test]
fn missing_one_binding_is_rejected_before_native_boot() {
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    CapabilityCardinality::One,
                ),
            ),
        ],
        vec![],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::MissingOneBinding {
            consumer_instance: "consumer".to_owned(),
            capability_id: GREETING_CAPABILITY_ID.to_owned(),
        })
    );
}

#[test]
fn missing_one_binding_is_rejected_before_the_native_adapter_runs() {
    let driver = DeterministicDriver::new();
    let plan = ResolvedAppPlan::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::one(GREETING_CAPABILITY_ID, GREETING_DESCRIPTOR_VERSION),
            ),
        ],
        vec![],
    );

    let outcome = driver.run(Kernel::start_native(
        plan,
        driver.clone(),
        NativeModuleRegistry::new(),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("missing one binding")
    ));
}

#[test]
fn optional_requirement_may_be_unbound() {
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::optional(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                ),
            ),
        ],
        vec![],
    );

    let plan = composition
        .resolve()
        .expect("an absent optional binding should resolve");

    assert!(plan.capability_bindings().is_empty());
}

#[test]
fn many_requirement_may_be_unbound_and_fan_out_to_nothing() {
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::many(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                ),
            ),
        ],
        vec![],
    );
    let plan = composition
        .resolve()
        .expect("an absent many binding should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            NativeModuleRegistry::new().with_factory(ConsumerFactory),
        ))
        .expect("the consumer should start without many providers");

    let handle = app
        .many_handle::<lenso_capability_greeting::Greeting>("consumer")
        .expect("an empty many handle should be materialized");
    assert_eq!(handle.binding_count(), 0);
    let outcomes = driver
        .run(handle.invoke_many(
            lenso_capability_greeting::GREET_OPERATION,
            GreetRequest {
                name: "Ada".to_owned(),
            },
        ))
        .expect("an empty many fan-out should succeed");
    assert!(outcomes.is_empty());
}

#[test]
fn a_singular_client_does_not_fallback_to_the_first_many_provider() {
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::many(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                ),
            ),
            ModuleInstancePlan::new("provider-z", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
                ),
            ),
            ModuleInstancePlan::new("provider-a", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
                ),
            ),
        ],
        vec![
            CapabilityBinding::new(
                "consumer",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "provider-z",
            ),
            CapabilityBinding::new(
                "consumer",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "provider-a",
            ),
        ],
    );
    let plan = composition.resolve().expect("many binding should resolve");
    let registry = NativeModuleRegistry::new()
        .with_factory(GreeterFactory)
        .with_factory(ConsumerFactory);
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), registry))
        .expect("the App should start with both providers");
    assert_eq!(
        app.binding_count::<lenso_capability_greeting::Greeting>("consumer"),
        2
    );
    let client = GreetingClient::new(
        app.handle::<Greeting>("consumer")
            .expect("many binding should be present"),
    );

    let outcome = driver.run(client.greet(GreetRequest {
        name: "Ada".to_owned(),
    }));

    assert_eq!(
        outcome,
        Err(GreetingInvocationError::Runtime(
            RuntimeFailure::AmbiguousBinding {
                capability: GREETING_CAPABILITY_ID,
                providers: 2,
            },
        ))
    );

    let handle = app
        .many_handle::<lenso_capability_greeting::Greeting>("consumer")
        .expect("the many handle should be materialized");
    let outcomes = driver
        .run(handle.invoke_many(
            lenso_capability_greeting::GREET_OPERATION,
            GreetRequest {
                name: "Ada".to_owned(),
            },
        ))
        .expect("both providers should receive the typed request");
    assert_eq!(
        outcomes
            .into_iter()
            .map(|outcome| outcome.unwrap().message)
            .collect::<Vec<_>>(),
        ["Hello, Ada!", "Hello, Ada!"]
    );
}

#[test]
fn ambiguous_one_binding_is_rejected() {
    let composition = greeting_composition(GREETER_PACKAGE_ID);
    let composition = AppComposition::new(
        composition.module_instances().to_vec(),
        vec![
            CapabilityBinding::new(
                "consumer",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "greeter",
            ),
            CapabilityBinding::new(
                "consumer",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "greeter",
            ),
        ],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::AmbiguousOneBinding {
            consumer_instance: "consumer".to_owned(),
            capability_id: GREETING_CAPABILITY_ID.to_owned(),
            providers: 2,
        })
    );
}

#[test]
fn required_one_bindings_cannot_form_an_activation_cycle() {
    let endpoint = || {
        CapabilityEndpointPlan::new(
            GREETING_CAPABILITY_ID,
            GREETING_DESCRIPTOR_VERSION,
            [lenso_capability_greeting::GREET_OPERATION],
        )
    };
    let requirement =
        || CapabilityRequirementPlan::one(GREETING_CAPABILITY_ID, GREETING_DESCRIPTOR_VERSION);
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("a", GREETER_PACKAGE_ID)
                .with_capability(endpoint())
                .with_requirement(requirement()),
            ModuleInstancePlan::new("b", GREETER_PACKAGE_ID)
                .with_capability(endpoint())
                .with_requirement(requirement()),
        ],
        vec![
            CapabilityBinding::new(
                "a",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "b",
            ),
            CapabilityBinding::new(
                "b",
                GREETING_CAPABILITY_ID,
                GREETING_DESCRIPTOR_VERSION,
                "a",
            ),
        ],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::ActivationCycle {
            instances: vec!["a".to_owned(), "b".to_owned()]
        })
    );
}

#[test]
fn invalid_provider_reference_is_rejected() {
    let composition = AppComposition::new(
        vec![
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
            "missing-provider",
        )],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::InvalidProviderReference {
            consumer_instance: "consumer".to_owned(),
            capability_id: GREETING_CAPABILITY_ID.to_owned(),
            provider_instance: "missing-provider".to_owned(),
        })
    );
}

#[test]
fn incompatible_capability_versions_are_rejected() {
    let composition = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    GREETING_CAPABILITY_ID,
                    "2.0.0",
                    CapabilityCardinality::One,
                ),
            ),
            ModuleInstancePlan::new("greeter", GREETER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    [lenso_capability_greeting::GREET_OPERATION],
                ),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            GREETING_CAPABILITY_ID,
            "2.0.0",
            "greeter",
        )],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::IncompatibleCapabilityVersion {
            consumer_instance: "consumer".to_owned(),
            capability_id: GREETING_CAPABILITY_ID.to_owned(),
            required: "2.0.0".to_owned(),
            provided: GREETING_DESCRIPTOR_VERSION.to_owned(),
            provider_instance: "greeter".to_owned(),
        })
    );
}

#[test]
fn two_static_providers_pass_the_same_generated_client_contract() {
    assert_provider_uses_the_generated_contract(GREETER_PACKAGE_ID, "Hello, Ada!");
    assert_provider_uses_the_generated_contract(ALTERNATE_GREETER_PACKAGE_ID, "Ahoy, Ada!");
}

#[test]
fn replacing_the_provider_changes_composition_and_plan_but_not_the_consumer_binding() {
    let first_plan = greeting_composition(GREETER_PACKAGE_ID)
        .resolve()
        .expect("the first provider should resolve");
    let second_plan = greeting_composition(ALTERNATE_GREETER_PACKAGE_ID)
        .resolve()
        .expect("the replacement provider should resolve");

    let first_driver = DeterministicDriver::new();
    let first_app = first_driver
        .run(Kernel::start_native(
            first_plan.clone(),
            first_driver.clone(),
            greeting_registry(),
        ))
        .expect("the first selected provider should start");
    let second_driver = DeterministicDriver::new();
    let second_app = second_driver
        .run(Kernel::start_native(
            second_plan.clone(),
            second_driver.clone(),
            greeting_registry(),
        ))
        .expect("the replacement provider should start");

    let first_response = first_driver.run(
        GreetingClient::new(
            first_app
                .handle::<Greeting>("consumer")
                .expect("first binding should resolve"),
        )
        .greet(GreetRequest {
            name: "Ada".to_owned(),
        }),
    );
    let second_response = second_driver.run(
        GreetingClient::new(
            second_app
                .handle::<Greeting>("consumer")
                .expect("replacement binding should resolve"),
        )
        .greet(GreetRequest {
            name: "Ada".to_owned(),
        }),
    );

    assert_eq!(first_response.unwrap().message, "Hello, Ada!");
    assert_eq!(second_response.unwrap().message, "Ahoy, Ada!");
    assert_ne!(first_plan, second_plan);
    assert_eq!(
        first_plan
            .module_instances()
            .iter()
            .find(|instance| instance.instance_key() == "consumer"),
        second_plan
            .module_instances()
            .iter()
            .find(|instance| instance.instance_key() == "consumer")
    );
    assert_eq!(
        first_plan.capability_bindings(),
        second_plan.capability_bindings()
    );
}

fn assert_provider_uses_the_generated_contract(package_id: &str, expected_message: &str) {
    let (app, driver) = greeting_app(package_id);
    let client = GreetingClient::new(
        app.handle::<Greeting>("consumer")
            .expect("binding should resolve"),
    );

    let response = driver.run(client.greet(GreetRequest {
        name: "Ada".to_owned(),
    }));

    assert_eq!(response.unwrap().message, expected_message);

    let domain_error = driver.run(client.greet(GreetRequest {
        name: String::new(),
    }));
    assert_eq!(
        domain_error,
        Err(GreetingInvocationError::Domain(GreetError::EmptyName))
    );
}

#[test]
fn native_registry_recreates_a_generation_through_the_supervision_seam() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::one(GREETING_CAPABILITY_ID, GREETING_DESCRIPTOR_VERSION),
            ),
            ModuleInstancePlan::new("greeter", GREETER_PACKAGE_ID)
                .with_restart_policy(RestartPolicy::on_failure(
                    1,
                    Duration::from_secs(30),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::from_secs(1),
                ))
                .with_capability(CapabilityEndpointPlan::new(
                    GREETING_CAPABILITY_ID,
                    GREETING_DESCRIPTOR_VERSION,
                    ["greet"],
                )),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            GREETING_CAPABILITY_ID,
            GREETING_DESCRIPTOR_VERSION,
            "greeter",
        )],
    )
    .resolve()
    .expect("the supervised native plan should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            greeting_registry(),
        ))
        .expect("the native registry App should start");
    let client = GreetingClient::new(
        app.handle::<Greeting>("consumer")
            .expect("the stable handle should resolve"),
    );

    app.report_module_failure("greeter")
        .expect("the production registry should schedule recreation");
    driver.run(async {
        for _ in 0..6 {
            driver.yield_now().await;
        }
    });

    assert_eq!(app.module_generation("greeter"), Some(2));
    assert_eq!(
        driver
            .run(client.greet(GreetRequest {
                name: "Registry".to_owned(),
            }))
            .expect("the stable generated client should use the replacement generation")
            .message,
        "Hello, Registry!"
    );
}
