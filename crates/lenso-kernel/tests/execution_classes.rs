use std::{any::Any, cell::Cell, rc::Rc};

use futures::FutureExt;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ExecutionClassId, PluginInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    DeterministicDriver, ExecutionAdapter, ExecutionAdapterCatalog, ExecutionAdapterCatalogError,
    InvocationContext, Kernel, NativeRequestEndpoint, NoopPluginLifecycle, PreparedBinding,
    PreparedNativeApp, PreparedNativePlugin, RuntimeFailure,
};

const CAPABILITY_ID: &str = "test.echo";
const DESCRIPTOR_VERSION: &str = "1.0.0";

#[derive(Debug)]
struct EchoEndpoint;

impl NativeRequestEndpoint for EchoEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &["echo"]
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

#[derive(Debug)]
struct RecordingAdapter {
    execution_class: ExecutionClassId,
    prepared: Rc<Cell<bool>>,
    bindings: Vec<PreparedBinding>,
}

#[derive(Debug)]
struct IncompleteAdapter;

impl ExecutionAdapter for IncompleteAdapter {
    fn execution_class(&self) -> ExecutionClassId {
        ExecutionClassId::native_rust()
    }

    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        Ok(PreparedNativeApp::empty())
    }
}

#[derive(Debug)]
struct MissingBindingAdapter;

impl ExecutionAdapter for MissingBindingAdapter {
    fn execution_class(&self) -> ExecutionClassId {
        ExecutionClassId::native_rust()
    }

    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let endpoint: Rc<dyn NativeRequestEndpoint> = Rc::new(EchoEndpoint);
        Ok(PreparedNativeApp::new(
            Vec::new(),
            [
                (
                    "consumer".to_owned(),
                    PreparedNativePlugin::new(Vec::new(), NoopPluginLifecycle),
                ),
                (
                    "provider".to_owned(),
                    PreparedNativePlugin::new(vec![endpoint], NoopPluginLifecycle),
                ),
            ]
            .into_iter()
            .collect(),
        ))
    }
}

impl ExecutionAdapter for RecordingAdapter {
    fn execution_class(&self) -> ExecutionClassId {
        self.execution_class.clone()
    }

    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        self.prepared.set(true);
        let generations = plan
            .plugin_instances()
            .iter()
            .filter(|instance| instance.execution_class() == &self.execution_class)
            .map(|instance| {
                let endpoints = self
                    .bindings
                    .iter()
                    .filter(|binding| binding.provider_instance() == instance.instance_key())
                    .map(PreparedBinding::endpoint)
                    .collect();
                (
                    instance.instance_key().to_owned(),
                    PreparedNativePlugin::new(endpoints, NoopPluginLifecycle),
                )
            })
            .collect();
        Ok(PreparedNativeApp::new(self.bindings.clone(), generations))
    }
}

fn plan_with_classes(classes: &[&str]) -> ResolvedAppPlan {
    AppComposition::new(
        classes
            .iter()
            .enumerate()
            .map(|(index, execution_class)| {
                PluginInstancePlan::new(format!("plugin-{index}"), format!("package-{index}"))
                    .with_execution_class(ExecutionClassId::new(*execution_class))
            })
            .collect(),
        vec![],
    )
    .resolve()
    .expect("open execution class IDs are valid authoring data")
}

#[test]
fn kernel_rejects_a_missing_execution_class_before_any_adapter_prepares() {
    let plan = plan_with_classes(&["community.python-process@1"]);
    let native_prepared = Rc::new(Cell::new(false));
    let adapters = ExecutionAdapterCatalog::new()
        .with_adapter(RecordingAdapter {
            execution_class: ExecutionClassId::native_rust(),
            prepared: native_prepared.clone(),
            bindings: Vec::new(),
        })
        .expect("the native execution class is unique");
    let driver = DeterministicDriver::new();

    let result = driver.run(Kernel::start(plan, driver.clone(), adapters));

    assert!(matches!(
        result,
        Err(RuntimeFailure::UnavailableExecutionClass {
            instance_key,
            execution_class,
        }) if instance_key == "plugin-0" && execution_class == "community.python-process@1"
    ));
    assert!(!native_prepared.get());
}

#[test]
fn kernel_rejects_an_incomplete_adapter_result_before_lifecycle() {
    let plan = plan_with_classes(&["lenso.native-rust@1"]);
    let driver = DeterministicDriver::new();

    let result = driver.run(Kernel::start(
        plan,
        driver.clone(),
        ExecutionAdapterCatalog::single(IncompleteAdapter),
    ));

    assert!(matches!(
        result,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("prepared 0 Plugin generations; expected 1")
    ));
}

#[test]
fn kernel_rejects_a_missing_prepared_binding_before_lifecycle() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::one(CAPABILITY_ID, DESCRIPTOR_VERSION),
            ),
            PluginInstancePlan::new("provider", "package.provider").with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, ["echo"]),
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
    .expect("the binding plan should resolve");
    let driver = DeterministicDriver::new();

    let result = driver.run(Kernel::start(
        plan,
        driver.clone(),
        ExecutionAdapterCatalog::single(MissingBindingAdapter),
    ));

    assert!(matches!(
        result,
        Err(RuntimeFailure::InvalidResolvedPlan { detail })
            if detail.contains("prepared 0 bindings; expected 1")
    ));
}

#[test]
fn runner_catalog_composes_independent_execution_adapter_plugins() {
    let plan = plan_with_classes(&["lenso.native-rust@1", "lenso.bun-process@1"]);
    let native_prepared = Rc::new(Cell::new(false));
    let bun_prepared = Rc::new(Cell::new(false));
    let adapters = ExecutionAdapterCatalog::new()
        .with_adapter(RecordingAdapter {
            execution_class: ExecutionClassId::native_rust(),
            prepared: native_prepared.clone(),
            bindings: Vec::new(),
        })
        .expect("the native execution class is unique")
        .with_shared_adapter(Rc::new(RecordingAdapter {
            execution_class: ExecutionClassId::bun_child_process(),
            prepared: bun_prepared.clone(),
            bindings: Vec::new(),
        }))
        .expect("the Bun execution class is unique");
    let classes = adapters.execution_classes();

    assert!(classes.contains(&ExecutionClassId::native_rust()));
    assert!(classes.contains(&ExecutionClassId::bun_child_process()));

    let driver = DeterministicDriver::new();
    let result = driver.run(Kernel::start(plan, driver.clone(), adapters));

    assert!(result.is_ok());
    assert!(native_prepared.get());
    assert!(bun_prepared.get());
}

#[test]
fn runner_catalog_rejects_duplicate_execution_class_plugins() {
    let first_prepared = Rc::new(Cell::new(false));
    let second_prepared = Rc::new(Cell::new(false));
    let catalog = ExecutionAdapterCatalog::new()
        .with_adapter(RecordingAdapter {
            execution_class: ExecutionClassId::bun_child_process(),
            prepared: first_prepared,
            bindings: Vec::new(),
        })
        .expect("the first Bun Adapter owns the class");

    let result = catalog.with_adapter(RecordingAdapter {
        execution_class: ExecutionClassId::bun_child_process(),
        prepared: second_prepared,
        bindings: Vec::new(),
    });

    assert!(matches!(
        result,
        Err(ExecutionAdapterCatalogError::DuplicateExecutionClass { execution_class })
            if execution_class == "lenso.bun-process@1"
    ));
}

#[test]
fn catalog_composes_many_bindings_prepared_by_different_execution_classes() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::new(
                    CAPABILITY_ID,
                    DESCRIPTOR_VERSION,
                    CapabilityCardinality::Many,
                ),
            ),
            PluginInstancePlan::new("native-provider", "package.native").with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, ["echo"]),
            ),
            PluginInstancePlan::new("bun-provider", "package.bun")
                .with_execution_class(ExecutionClassId::bun_child_process())
                .with_capability(CapabilityEndpointPlan::new(
                    CAPABILITY_ID,
                    DESCRIPTOR_VERSION,
                    ["echo"],
                )),
        ],
        vec![
            CapabilityBinding::new(
                "consumer",
                CAPABILITY_ID,
                DESCRIPTOR_VERSION,
                "native-provider",
            ),
            CapabilityBinding::new(
                "consumer",
                CAPABILITY_ID,
                DESCRIPTOR_VERSION,
                "bun-provider",
            ),
        ],
    )
    .resolve()
    .expect("the cross-class many binding should resolve");
    let native_prepared = Rc::new(Cell::new(false));
    let bun_prepared = Rc::new(Cell::new(false));
    let adapters = ExecutionAdapterCatalog::new()
        .with_adapter(RecordingAdapter {
            execution_class: ExecutionClassId::native_rust(),
            prepared: native_prepared.clone(),
            bindings: vec![PreparedBinding::new(
                "consumer",
                "native-provider",
                Rc::new(EchoEndpoint),
            )],
        })
        .expect("the native execution class is unique")
        .with_adapter(RecordingAdapter {
            execution_class: ExecutionClassId::bun_child_process(),
            prepared: bun_prepared.clone(),
            bindings: vec![PreparedBinding::new(
                "consumer",
                "bun-provider",
                Rc::new(EchoEndpoint),
            )],
        })
        .expect("the Bun execution class is unique");
    let driver = DeterministicDriver::new();

    let result = driver.run(Kernel::start(plan, driver.clone(), adapters));

    assert!(result.is_ok());
    assert!(native_prepared.get());
    assert!(bun_prepared.get());
}
