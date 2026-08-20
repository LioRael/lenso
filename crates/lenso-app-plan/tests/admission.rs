use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ModuleInstancePlan, RequestAdmissionPlan,
};

#[test]
fn resolved_bindings_materialize_bounded_request_admission() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::one("capability.greeting", "1.0.0"),
            ),
            ModuleInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new("capability.greeting", "1.0.0", ["greet"])
                    .with_operation_admission("greet", RequestAdmissionPlan::new(3, 2)),
            ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            "capability.greeting",
            "1.0.0",
            "provider",
        )],
    )
    .resolve()
    .expect("the admission configuration should resolve");

    let binding = &plan.capability_bindings()[0];
    let admission = plan.request_admission_for(binding, "greet");

    assert_eq!(admission.queue_capacity(), 3);
    assert_eq!(admission.max_concurrency(), 2);
}

#[test]
fn a_binding_can_override_the_provider_operation_admission() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::new(
                    "capability.greeting",
                    "1.0.0",
                    CapabilityCardinality::One,
                ),
            ),
            ModuleInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new("capability.greeting", "1.0.0", ["greet"])
                    .with_admission(RequestAdmissionPlan::new(3, 2)),
            ),
        ],
        vec![
            CapabilityBinding::new("consumer", "capability.greeting", "1.0.0", "provider")
                .with_admission(RequestAdmissionPlan::new(0, 1)),
        ],
    )
    .resolve()
    .expect("the binding admission override should resolve");

    let binding = &plan.capability_bindings()[0];
    let admission = plan.request_admission_for(binding, "greet");

    assert_eq!(admission.queue_capacity(), 0);
    assert_eq!(admission.max_concurrency(), 1);
}

#[test]
fn zero_concurrency_is_rejected_before_boot() {
    let result = AppComposition::new(
        vec![
            ModuleInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new("capability.greeting", "1.0.0", ["greet"])
                    .with_admission(RequestAdmissionPlan::new(1, 0)),
            ),
        ],
        vec![],
    )
    .resolve();

    assert!(matches!(
        result,
        Err(lenso_app_plan::PlanResolutionError::InvalidRequestAdmission {
            capability_id,
            operation,
            ..
        }) if capability_id == "capability.greeting" && operation == "greet"
    ));
}

#[test]
fn zero_binding_concurrency_is_rejected_before_boot() {
    let result = AppComposition::new(
        vec![
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::one("capability.greeting", "1.0.0"),
            ),
            ModuleInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new("capability.greeting", "1.0.0", ["greet"]),
            ),
        ],
        vec![
            CapabilityBinding::new("consumer", "capability.greeting", "1.0.0", "provider")
                .with_admission(RequestAdmissionPlan::new(1, 0)),
        ],
    )
    .resolve();

    assert!(matches!(
        result,
        Err(lenso_app_plan::PlanResolutionError::InvalidRequestAdmission {
            capability_id,
            operation,
            ..
        }) if capability_id == "capability.greeting" && operation == "greet"
    ));
}
