use std::{any::Any, cell::Cell, collections::BTreeMap, rc::Rc};

use futures::FutureExt;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ExecutionClassId, ModuleInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    DeterministicDriver, ExecutionAdapter, ExecutionAdapterCatalog, ExecutionAdapterCatalogError,
    InvocationContext, Kernel, NativeRequestEndpoint, PreparedBinding, PreparedNativeApp,
    RuntimeFailure,
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

impl ExecutionAdapter for RecordingAdapter {
    fn execution_class(&self) -> ExecutionClassId {
        self.execution_class.clone()
    }

    fn prepare(&self, _plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        self.prepared.set(true);
        Ok(PreparedNativeApp::with_modules(
            self.bindings.clone(),
            BTreeMap::new(),
        ))
    }
}

fn plan_with_classes(classes: &[&str]) -> ResolvedAppPlan {
    AppComposition::new(
        classes
            .iter()
            .enumerate()
            .map(|(index, execution_class)| {
                ModuleInstancePlan::new(format!("module-{index}"), format!("package-{index}"))
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
        }) if instance_key == "module-0" && execution_class == "community.python-process@1"
    ));
    assert!(!native_prepared.get());
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
            ModuleInstancePlan::new("consumer", "package.consumer").with_requirement(
                CapabilityRequirementPlan::new(
                    CAPABILITY_ID,
                    DESCRIPTOR_VERSION,
                    CapabilityCardinality::Many,
                ),
            ),
            ModuleInstancePlan::new("native-provider", "package.native").with_capability(
                CapabilityEndpointPlan::new(CAPABILITY_ID, DESCRIPTOR_VERSION, ["echo"]),
            ),
            ModuleInstancePlan::new("bun-provider", "package.bun")
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
