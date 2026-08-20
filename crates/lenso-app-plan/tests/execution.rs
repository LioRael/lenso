use lenso_app_plan::{
    AppComposition, ExecutionClass, ExecutionClassSet, ModuleInstancePlan, PlanResolutionError,
};

#[test]
fn host_validation_rejects_a_child_process_module_without_host_support() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("bun", "package.bun")
                .with_execution_class(ExecutionClass::BunChildProcess),
        ],
        vec![],
    )
    .resolve()
    .expect("the execution class is valid authoring data");

    assert!(matches!(
        plan.validate_for(ExecutionClassSet::native_rust()),
        Err(PlanResolutionError::UnsupportedExecutionClass {
            instance_key,
            execution_class: ExecutionClass::BunChildProcess,
        }) if instance_key == "bun"
    ));
}

#[test]
fn host_validation_accepts_each_class_explicitly_admitted_by_the_host() {
    let plan = AppComposition::new(
        vec![
            ModuleInstancePlan::new("native", "package.native"),
            ModuleInstancePlan::new("bun", "package.bun")
                .with_execution_class(ExecutionClass::BunChildProcess),
        ],
        vec![],
    )
    .resolve()
    .expect("the execution classes are valid authoring data");

    let host = ExecutionClassSet::native_rust().with(ExecutionClass::BunChildProcess);
    plan.validate_for(host)
        .expect("the host explicitly provides both execution classes");
}
