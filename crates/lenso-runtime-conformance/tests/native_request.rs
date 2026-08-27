//! Black-box Kernel request conformance through product-neutral fixtures.

use std::time::Duration;

use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, PlanResolutionError, PluginInstancePlan, ResolvedAppPlan,
    RestartPolicy,
};
use lenso_kernel::{DeterministicDriver, Kernel, RuntimeDriver, RuntimeFailure};
use lenso_runtime_conformance::ConformanceExecutionAdapter;
use lenso_runtime_conformance::{
    ALTERNATE_PROBE_PROVIDER_PACKAGE_ID, AlternateProbeProviderFactory, PROBE_CONSUMER_PACKAGE_ID,
    PROBE_PROVIDER_PACKAGE_ID, ProbeConsumerFactory, ProbeProviderFactory,
};
use lenso_runtime_conformance::{
    PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION, PROBE_OPERATION, Probe, ProbeClient, ProbeError,
    ProbeInvocationError, ProbeRequest,
};

fn probe_composition(provider_package_id: &str) -> AppComposition {
    AppComposition::new(
        vec![
            PluginInstancePlan::new("provider", provider_package_id).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [lenso_runtime_conformance::PROBE_OPERATION],
                ),
            ),
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    CapabilityCardinality::One,
                ),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            PROBE_CAPABILITY_ID,
            PROBE_DESCRIPTOR_VERSION,
            "provider",
        )],
    )
}

fn probe_adapter() -> ConformanceExecutionAdapter {
    ConformanceExecutionAdapter::new()
        .with_factory(ProbeProviderFactory)
        .with_factory(AlternateProbeProviderFactory)
        .with_factory(ProbeConsumerFactory)
}

fn probe_app(provider_package_id: &str) -> (lenso_kernel::NativeApp, DeterministicDriver) {
    let plan = probe_composition(provider_package_id)
        .resolve()
        .expect("the conformance Composition should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), probe_adapter()))
        .expect("the native App should start");
    (app, driver)
}

#[test]
fn typed_client_invokes_a_prepared_provider() {
    let (app, driver) = probe_app(PROBE_PROVIDER_PACKAGE_ID);
    let client = ProbeClient::new(
        app.handle::<Probe>("consumer")
            .expect("binding should resolve"),
    );

    let response = driver.run(client.probe(ProbeRequest {
        value: "Ada".to_owned(),
    }));

    assert_eq!(response.unwrap().value, "Echo: Ada");
}

#[test]
fn typed_client_preserves_domain_errors() {
    let (app, driver) = probe_app(PROBE_PROVIDER_PACKAGE_ID);
    let client = ProbeClient::new(
        app.handle::<Probe>("consumer")
            .expect("binding should resolve"),
    );

    let outcome = driver.run(client.probe(ProbeRequest {
        value: String::new(),
    }));

    assert_eq!(
        outcome,
        Err(ProbeInvocationError::Domain(ProbeError::EmptyValue))
    );
}

#[test]
fn kernel_rejects_an_unknown_operation_as_a_runtime_failure() {
    let (app, driver) = probe_app(PROBE_PROVIDER_PACKAGE_ID);

    let outcome = driver.run(app.invoke::<lenso_runtime_conformance::Probe>(
        "consumer",
        "missing.operation",
        ProbeRequest {
            value: "Ada".to_owned(),
        },
    ));

    assert_eq!(
        outcome,
        Err(RuntimeFailure::UnknownOperation {
            capability: PROBE_CAPABILITY_ID,
            operation: "missing.operation".to_owned(),
        })
    );
}

#[test]
fn typed_client_reports_a_missing_binding_as_a_runtime_failure() {
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            ResolvedAppPlan::new(vec![], vec![]),
            driver.clone(),
            ConformanceExecutionAdapter::new(),
        ))
        .expect("an App without providers can start");

    let error = app.handle::<Probe>("consumer").unwrap_err();

    assert_eq!(
        error,
        RuntimeFailure::Unavailable {
            capability: PROBE_CAPABILITY_ID,
        }
    );
}

#[test]
fn kernel_rejects_a_planned_plugin_without_a_linked_factory() {
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::start_native(
        ResolvedAppPlan::new(
            vec![PluginInstancePlan::new(
                "consumer",
                PROBE_CONSUMER_PACKAGE_ID,
            )],
            vec![],
        ),
        driver.clone(),
        ConformanceExecutionAdapter::new(),
    ));

    assert_eq!(
        outcome.unwrap_err(),
        RuntimeFailure::MissingPluginFactory {
            instance: "consumer".to_owned(),
            package_id: PROBE_CONSUMER_PACKAGE_ID.to_owned(),
        }
    );
}

#[test]
fn kernel_rejects_a_native_plan_with_an_unsupported_schema() {
    let driver = DeterministicDriver::new();

    let outcome = driver.run(Kernel::start_native(
        ResolvedAppPlan::with_schema_version(0),
        driver.clone(),
        ConformanceExecutionAdapter::new(),
    ));

    assert!(matches!(
        outcome,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("unsupported Plan schema version 0")
    ));
}

#[test]
fn conformance_adapter_rejects_an_operation_table_mismatch_during_preparation() {
    let driver = DeterministicDriver::new();
    let plan = ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new("provider", PROBE_PROVIDER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    ["undeclared.operation"],
                ),
            ),
        ],
        vec![],
    );

    let outcome = driver.run(Kernel::start_native(
        plan,
        driver.clone(),
        ConformanceExecutionAdapter::new().with_factory(ProbeProviderFactory),
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
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    CapabilityCardinality::Many,
                ),
            ),
            PluginInstancePlan::new("provider-z", PROBE_PROVIDER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [lenso_runtime_conformance::PROBE_OPERATION],
                ),
            ),
            PluginInstancePlan::new("provider-a", PROBE_PROVIDER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [lenso_runtime_conformance::PROBE_OPERATION],
                ),
            ),
        ],
        vec![
            CapabilityBinding::new(
                "consumer",
                PROBE_CAPABILITY_ID,
                PROBE_DESCRIPTOR_VERSION,
                "provider-z",
            ),
            CapabilityBinding::new(
                "consumer",
                PROBE_CAPABILITY_ID,
                PROBE_DESCRIPTOR_VERSION,
                "provider-a",
            ),
        ],
    );

    let plan = composition.resolve().expect("many binding should resolve");

    assert_eq!(
        plan.plugin_instances()
            .iter()
            .map(PluginInstancePlan::instance_key)
            .collect::<Vec<_>>(),
        ["consumer", "provider-a", "provider-z"]
    );
    let consumer = &plan.plugin_instances()[0];
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
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
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
            capability_id: PROBE_CAPABILITY_ID.to_owned(),
        })
    );
}

#[test]
fn missing_one_binding_is_rejected_before_the_execution_adapter_runs() {
    let driver = DeterministicDriver::new();
    let plan = ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::one(PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION),
            ),
        ],
        vec![],
    );

    let outcome = driver.run(Kernel::start_native(
        plan,
        driver.clone(),
        ConformanceExecutionAdapter::new(),
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
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::optional(PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION),
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
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::many(PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION),
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
            ConformanceExecutionAdapter::new().with_factory(ProbeConsumerFactory),
        ))
        .expect("the consumer should start without many providers");

    let handle = app
        .many_handle::<lenso_runtime_conformance::Probe>("consumer")
        .expect("an empty many handle should be materialized");
    assert_eq!(handle.binding_count(), 0);
    let outcomes = driver
        .run(handle.invoke_many(
            lenso_runtime_conformance::PROBE_OPERATION,
            ProbeRequest {
                value: "Ada".to_owned(),
            },
        ))
        .expect("an empty many fan-out should succeed");
    assert!(outcomes.is_empty());
}

#[test]
fn a_singular_client_does_not_fallback_to_the_first_many_provider() {
    let composition = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::many(PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION),
            ),
            PluginInstancePlan::new("provider-z", PROBE_PROVIDER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [lenso_runtime_conformance::PROBE_OPERATION],
                ),
            ),
            PluginInstancePlan::new("provider-a", PROBE_PROVIDER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [lenso_runtime_conformance::PROBE_OPERATION],
                ),
            ),
        ],
        vec![
            CapabilityBinding::new(
                "consumer",
                PROBE_CAPABILITY_ID,
                PROBE_DESCRIPTOR_VERSION,
                "provider-z",
            ),
            CapabilityBinding::new(
                "consumer",
                PROBE_CAPABILITY_ID,
                PROBE_DESCRIPTOR_VERSION,
                "provider-a",
            ),
        ],
    );
    let plan = composition.resolve().expect("many binding should resolve");
    let registry = ConformanceExecutionAdapter::new()
        .with_factory(ProbeProviderFactory)
        .with_factory(ProbeConsumerFactory);
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), registry))
        .expect("the App should start with both providers");
    assert_eq!(
        app.binding_count::<lenso_runtime_conformance::Probe>("consumer"),
        2
    );
    let client = ProbeClient::new(
        app.handle::<Probe>("consumer")
            .expect("many binding should be present"),
    );

    let outcome = driver.run(client.probe(ProbeRequest {
        value: "Ada".to_owned(),
    }));

    assert_eq!(
        outcome,
        Err(ProbeInvocationError::Runtime(
            RuntimeFailure::AmbiguousBinding {
                capability: PROBE_CAPABILITY_ID,
                providers: 2,
            },
        ))
    );

    let handle = app
        .many_handle::<lenso_runtime_conformance::Probe>("consumer")
        .expect("the many handle should be materialized");
    let outcomes = driver
        .run(handle.invoke_many(
            lenso_runtime_conformance::PROBE_OPERATION,
            ProbeRequest {
                value: "Ada".to_owned(),
            },
        ))
        .expect("both providers should receive the typed request");
    assert_eq!(
        outcomes
            .into_iter()
            .map(|outcome| outcome.unwrap().value)
            .collect::<Vec<_>>(),
        ["Echo: Ada", "Echo: Ada"]
    );
}

#[test]
fn ambiguous_one_binding_is_rejected() {
    let composition = probe_composition(PROBE_PROVIDER_PACKAGE_ID);
    let composition = AppComposition::new(
        composition.plugin_instances().to_vec(),
        vec![
            CapabilityBinding::new(
                "consumer",
                PROBE_CAPABILITY_ID,
                PROBE_DESCRIPTOR_VERSION,
                "provider",
            ),
            CapabilityBinding::new(
                "consumer",
                PROBE_CAPABILITY_ID,
                PROBE_DESCRIPTOR_VERSION,
                "provider",
            ),
        ],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::AmbiguousOneBinding {
            consumer_instance: "consumer".to_owned(),
            capability_id: PROBE_CAPABILITY_ID.to_owned(),
            providers: 2,
        })
    );
}

#[test]
fn required_one_bindings_cannot_form_an_activation_cycle() {
    let endpoint = || {
        CapabilityEndpointPlan::new(
            PROBE_CAPABILITY_ID,
            PROBE_DESCRIPTOR_VERSION,
            [lenso_runtime_conformance::PROBE_OPERATION],
        )
    };
    let requirement =
        || CapabilityRequirementPlan::one(PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION);
    let composition = AppComposition::new(
        vec![
            PluginInstancePlan::new("a", PROBE_PROVIDER_PACKAGE_ID)
                .with_capability(endpoint())
                .with_requirement(requirement()),
            PluginInstancePlan::new("b", PROBE_PROVIDER_PACKAGE_ID)
                .with_capability(endpoint())
                .with_requirement(requirement()),
        ],
        vec![
            CapabilityBinding::new("a", PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION, "b"),
            CapabilityBinding::new("b", PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION, "a"),
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
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    CapabilityCardinality::One,
                ),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            PROBE_CAPABILITY_ID,
            PROBE_DESCRIPTOR_VERSION,
            "missing-provider",
        )],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::InvalidProviderReference {
            consumer_instance: "consumer".to_owned(),
            capability_id: PROBE_CAPABILITY_ID.to_owned(),
            provider_instance: "missing-provider".to_owned(),
        })
    );
}

#[test]
fn incompatible_capability_versions_are_rejected() {
    let composition = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::new(
                    PROBE_CAPABILITY_ID,
                    "2.0.0",
                    CapabilityCardinality::One,
                ),
            ),
            PluginInstancePlan::new("provider", PROBE_PROVIDER_PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [lenso_runtime_conformance::PROBE_OPERATION],
                ),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            PROBE_CAPABILITY_ID,
            "2.0.0",
            "provider",
        )],
    );

    assert_eq!(
        composition.resolve(),
        Err(PlanResolutionError::IncompatibleCapabilityVersion {
            consumer_instance: "consumer".to_owned(),
            capability_id: PROBE_CAPABILITY_ID.to_owned(),
            required: "2.0.0".to_owned(),
            provided: PROBE_DESCRIPTOR_VERSION.to_owned(),
            provider_instance: "provider".to_owned(),
        })
    );
}

#[test]
fn two_providers_pass_the_same_typed_client_contract() {
    assert_provider_uses_the_typed_contract(PROBE_PROVIDER_PACKAGE_ID, "Echo: Ada");
    assert_provider_uses_the_typed_contract(ALTERNATE_PROBE_PROVIDER_PACKAGE_ID, "Alternate: Ada");
}

#[test]
fn replacing_the_provider_changes_composition_and_plan_but_not_the_consumer_binding() {
    let first_plan = probe_composition(PROBE_PROVIDER_PACKAGE_ID)
        .resolve()
        .expect("the first provider should resolve");
    let second_plan = probe_composition(ALTERNATE_PROBE_PROVIDER_PACKAGE_ID)
        .resolve()
        .expect("the replacement provider should resolve");

    let first_driver = DeterministicDriver::new();
    let first_app = first_driver
        .run(Kernel::start_native(
            first_plan.clone(),
            first_driver.clone(),
            probe_adapter(),
        ))
        .expect("the first selected provider should start");
    let second_driver = DeterministicDriver::new();
    let second_app = second_driver
        .run(Kernel::start_native(
            second_plan.clone(),
            second_driver.clone(),
            probe_adapter(),
        ))
        .expect("the replacement provider should start");

    let first_response = first_driver.run(
        ProbeClient::new(
            first_app
                .handle::<Probe>("consumer")
                .expect("first binding should resolve"),
        )
        .probe(ProbeRequest {
            value: "Ada".to_owned(),
        }),
    );
    let second_response = second_driver.run(
        ProbeClient::new(
            second_app
                .handle::<Probe>("consumer")
                .expect("replacement binding should resolve"),
        )
        .probe(ProbeRequest {
            value: "Ada".to_owned(),
        }),
    );

    assert_eq!(first_response.unwrap().value, "Echo: Ada");
    assert_eq!(second_response.unwrap().value, "Alternate: Ada");
    assert_ne!(first_plan, second_plan);
    assert_eq!(
        first_plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == "consumer"),
        second_plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == "consumer")
    );
    assert_eq!(
        first_plan.capability_bindings(),
        second_plan.capability_bindings()
    );
}

fn assert_provider_uses_the_typed_contract(package_id: &str, expected_message: &str) {
    let (app, driver) = probe_app(package_id);
    let client = ProbeClient::new(
        app.handle::<Probe>("consumer")
            .expect("binding should resolve"),
    );

    let response = driver.run(client.probe(ProbeRequest {
        value: "Ada".to_owned(),
    }));

    assert_eq!(response.unwrap().value, expected_message);

    let domain_error = driver.run(client.probe(ProbeRequest {
        value: String::new(),
    }));
    assert_eq!(
        domain_error,
        Err(ProbeInvocationError::Domain(ProbeError::EmptyValue))
    );
}

#[test]
fn conformance_adapter_recreates_a_generation_through_the_supervision_seam() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", PROBE_CONSUMER_PACKAGE_ID).with_requirement(
                CapabilityRequirementPlan::one(PROBE_CAPABILITY_ID, PROBE_DESCRIPTOR_VERSION),
            ),
            PluginInstancePlan::new("provider", PROBE_PROVIDER_PACKAGE_ID)
                .with_restart_policy(RestartPolicy::on_failure(
                    1,
                    Duration::from_secs(30),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::from_secs(1),
                ))
                .with_capability(CapabilityEndpointPlan::new(
                    PROBE_CAPABILITY_ID,
                    PROBE_DESCRIPTOR_VERSION,
                    [PROBE_OPERATION],
                )),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            PROBE_CAPABILITY_ID,
            PROBE_DESCRIPTOR_VERSION,
            "provider",
        )],
    )
    .resolve()
    .expect("the supervised native plan should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), probe_adapter()))
        .expect("the conformance Adapter App should start");
    let client = ProbeClient::new(
        app.handle::<Probe>("consumer")
            .expect("the stable handle should resolve"),
    );

    app.report_plugin_failure("provider")
        .expect("the conformance Adapter should schedule recreation");
    driver.run(async {
        for _ in 0..6 {
            driver.yield_now().await;
        }
    });

    assert_eq!(app.plugin_generation("provider"), Some(2));
    assert_eq!(
        driver
            .run(client.probe(ProbeRequest {
                value: "Registry".to_owned(),
            }))
            .expect("the stable typed client should use the replacement generation")
            .value,
        "Echo: Registry"
    );
}
