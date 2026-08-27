use lenso_app_plan::{AppComposition, ExecutionClassId, PluginInstancePlan};

#[test]
fn resolved_plan_preserves_an_open_execution_class_id() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("python", "package.python")
                .with_execution_class(ExecutionClassId::new("community.python-process@1")),
        ],
        vec![],
    )
    .resolve()
    .expect("the execution class ID is valid authoring data");

    assert_eq!(
        plan.plugin_instance("python")
            .expect("the instance is materialized")
            .execution_class()
            .as_str(),
        "community.python-process@1"
    );
}

#[test]
fn native_rust_is_the_default_execution_class() {
    let plan = AppComposition::new(
        vec![PluginInstancePlan::new("native", "package.native")],
        vec![],
    )
    .resolve()
    .expect("the default execution class is valid authoring data");

    assert_eq!(
        plan.plugin_instance("native")
            .expect("the instance is materialized")
            .execution_class(),
        &ExecutionClassId::native_rust()
    );
}
