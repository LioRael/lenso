#![allow(dead_code)]

use std::cell::Cell;

use lenso::prelude::*;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan,
};
use lenso_kernel::{DeterministicDriver, Kernel, NativeExecutionAdapter, RuntimeDriver};
use lenso_native_adapter::{
    CompleteObjectLifecycle, NativePluginFactory, NativePluginFactoryContext, NativePluginInstance,
    NativePluginRegistry, PluginObject,
};

#[derive(Debug)]
struct ConsumerFactory;

#[derive(Debug)]
struct BoundConsumer {
    health: lenso_kernel::NativeRequestHandle<health::Health>,
}

#[derive(Debug)]
struct BoundConsumerFactory {
    object: PluginObject<BoundConsumer>,
}

#[derive(Debug)]
struct PreparingConsumerFactory {
    fail: bool,
}

#[derive(Debug)]
struct PreparingConsumer {
    fail: bool,
}

impl lenso_kernel::PluginLifecycle for PreparingConsumer {
    fn prepare(&self, context: lenso_kernel::PrepareContext) -> lenso_kernel::PluginFuture {
        let fail = self.fail;
        Box::pin(async move {
            assert!(context.admission().is_closed());
            let health = context.dependencies().one::<health::Health>()?;
            assert_eq!(health.invoke("check", ()).await?, Ok(1));
            if fail {
                return Err(lenso_kernel::RuntimeFailure::PluginFailure {
                    detail: "dependent preparation failed after reading its provider".into(),
                });
            }
            Ok(())
        })
    }
}

impl NativePluginFactory for PreparingConsumerFactory {
    fn package_id(&self) -> &'static str {
        "fixture.preparing-consumer"
    }

    fn instantiate(
        &self,
        _context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, lenso_kernel::RuntimeFailure> {
        Ok(NativePluginInstance::with_lifecycle(
            vec![],
            PreparingConsumer { fail: self.fail },
        ))
    }
}

impl NativePluginFactory for BoundConsumerFactory {
    fn package_id(&self) -> &'static str {
        "fixture.bound-consumer"
    }

    fn runtime_profile(&self) -> &'static str {
        "lenso.native-authoring@2"
    }

    fn instantiate(
        &self,
        context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, lenso_kernel::RuntimeFailure> {
        let lifecycle =
            CompleteObjectLifecycle::new(self.object.clone(), context.configuration(), |context| {
                Box::pin(async move {
                    let health = context.dependencies().requirement("health")?.one()?;
                    Ok(std::rc::Rc::new(BoundConsumer { health }))
                })
            });
        Ok(NativePluginInstance::with_lifecycle(vec![], lifecycle))
    }
}

impl NativePluginFactory for ConsumerFactory {
    fn package_id(&self) -> &'static str {
        "fixture.consumer"
    }

    fn runtime_profile(&self) -> &'static str {
        "lenso.native-authoring@2"
    }

    fn instantiate(
        &self,
        _context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, lenso_kernel::RuntimeFailure> {
        Ok(NativePluginInstance::default())
    }
}

#[derive(Clone, Debug, PluginConfig)]
struct PluginSettings {
    enabled: bool,
}

#[doc(hidden)]
pub mod __lenso_native_support {
    pub use lenso::__private::{
        NativeEventEndpoint, NativePluginInstance, NativeRequestEndpoint, NativeStreamEndpoint,
    };
}

mod health {
    use std::any::Any;

    use lenso::__private::{
        InvocationContext, LocalBoxFuture, NativeRequestEndpoint, NativeRequestFuture,
        RuntimeFailure,
    };
    use lenso_kernel::RequestCapability;

    #[derive(Debug)]
    pub struct HealthEndpoint {
        provider: std::rc::Rc<dyn HealthProvider>,
    }

    impl HealthEndpoint {
        pub(crate) fn new(provider: impl HealthProvider) -> Self {
            Self {
                provider: std::rc::Rc::new(provider),
            }
        }
    }

    impl NativeRequestEndpoint for HealthEndpoint {
        fn capability_id(&self) -> &'static str {
            "example.health@1"
        }

        fn descriptor_version(&self) -> &'static str {
            "1.0.0"
        }

        fn operations(&self) -> &'static [&'static str] {
            &["check"]
        }

        fn invoke(
            &self,
            _operation: &str,
            request: Box<dyn Any>,
            context: InvocationContext,
        ) -> LocalBoxFuture<'static, Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>>
        {
            let Ok(request) = request.downcast::<()>() else {
                return Box::pin(async {
                    Err(RuntimeFailure::ProtocolViolation {
                        capability: "example.health@1",
                    })
                });
            };
            let _: () = *request;
            let invocation = self.provider.check(context, ());
            Box::pin(async move {
                invocation.await.map(|result| {
                    result
                        .map(|value| Box::new(value) as Box<dyn Any>)
                        .map_err(|error| Box::new(error) as Box<dyn Any>)
                })
            })
        }
    }

    #[macro_export]
    macro_rules! __test_lenso_provided_health {
        () => {
            r#"{"capability_id":"example.health@1","descriptor_version":"1.0.0","operations":["check"],"operation_kinds":{},"default_admission":{"queue_capacity":0,"max_concurrency":1},"operation_admissions":{},"event_admission":null,"cross_lane_transfer":false}"#
        };
    }
    pub use crate::__test_lenso_provided_health as __lenso_provided_health;

    #[macro_export]
    macro_rules! __test_lenso_native_endpoints_health {
        ($provider:expr, $support:path) => {{
            use $support as __LensoNativeSupport;
            (
                vec![
                    std::rc::Rc::new($crate::health::HealthEndpoint::new($provider))
                        as std::rc::Rc<dyn __LensoNativeSupport::NativeRequestEndpoint>,
                ],
                Vec::<std::rc::Rc<dyn __LensoNativeSupport::NativeStreamEndpoint>>::new(),
                Vec::<std::rc::Rc<dyn __LensoNativeSupport::NativeEventEndpoint>>::new(),
            )
        }};
    }
    pub use crate::__test_lenso_native_endpoints_health as __lenso_native_endpoints_health;

    #[macro_export]
    macro_rules! __test_lenso_native_lower_health {
        ($plugin:ty, $support:path) => {
            use $support as __LensoNativeSupportHealth;
            impl $crate::health::HealthProvider for $plugin {
                fn check(
                    &self,
                    context: __LensoNativeSupportHealth::InvocationContext,
                    request: (),
                ) -> __LensoNativeSupportHealth::NativeRequestFuture<$crate::health::Health> {
                    let plugin = self.clone();
                    Box::pin(async move { Ok(<$plugin>::check(&plugin, context, request).await) })
                }
            }
        };
    }
    pub use crate::__test_lenso_native_lower_health as __lenso_native_lower_health;

    #[macro_export]
    macro_rules! __test_lenso_native_lower_object_health {
        ($object:ty, $plugin:ty, $support:path) => {
            use $support as __LensoNativeObjectSupportHealth;
            impl $crate::health::HealthProvider for $object {
                fn check(
                    &self,
                    context: __LensoNativeObjectSupportHealth::InvocationContext,
                    request: (),
                ) -> __LensoNativeObjectSupportHealth::NativeRequestFuture<$crate::health::Health>
                {
                    let object = self.clone();
                    Box::pin(async move {
                        let plugin = object.get()?;
                        Ok(<$plugin>::check(plugin.as_ref(), context, request).await)
                    })
                }
            }
        };
    }
    pub use crate::__test_lenso_native_lower_object_health as __lenso_native_lower_object_health;

    #[derive(Debug)]
    pub struct Health;

    impl RequestCapability for Health {
        type Request = ();
        type Response = usize;
        type DomainError = ();

        const ID: &'static str = "example.health@1";
        const DESCRIPTOR_VERSION: &'static str = "1.0.0";
    }

    pub trait HealthProvider: std::fmt::Debug + 'static {
        fn check(&self, context: InvocationContext, request: ()) -> NativeRequestFuture<Health>;
    }
}

#[plugin]
#[derive(Clone, Debug, Default)]
struct StatelessHealthPlugin {
    calls: Cell<usize>,
    stops: std::rc::Rc<Cell<usize>>,
}

#[lenso::plugin_impl]
impl StatelessHealthPlugin {
    #[stop]
    fn stop(&self) {
        self.stops.set(self.stops.get() + 1);
    }
}

#[provides(health::Health)]
impl StatelessHealthPlugin {
    async fn check(
        &self,
        _context: lenso::__private::InvocationContext,
        (): (),
    ) -> Result<usize, ()> {
        let calls = self.calls.get() + 1;
        self.calls.set(calls);
        Ok(calls)
    }
}

#[test]
fn stateless_plugin_derives_empty_configuration() {
    let descriptor: lenso::__private::serde_json::Value =
        lenso::__private::serde_json::from_str(PLUGIN_DESCRIPTOR_JSON)
            .expect("generated Descriptor should be valid JSON");
    assert_eq!(
        descriptor["configuration_schema"],
        lenso::__private::serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "required": [],
            "properties": {},
        })
    );
    assert_eq!(descriptor["authoring_version"], 2);
    assert_eq!(descriptor["runtime_profile"], "lenso.native-authoring@2");

    let provider = PluginInstancePlan::new("health", "lenso")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_configuration("{}")
        .with_capability(CapabilityEndpointPlan::new(
            "example.health@1",
            "1.0.0",
            ["check"],
        ));
    let plan = AppComposition::new(vec![provider], vec![])
        .resolve()
        .expect("stateless plan should resolve");
    NativeExecutionAdapter::prepare(&NativePluginRegistry::new().with_linked_factories(), &plan)
        .expect("stateless Plugin should accept empty configuration");
}

#[test]
fn authoring_v2_provider_invokes_the_one_constructed_object() {
    let provider = PluginInstancePlan::new("health", "lenso")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_configuration("{}")
        .with_capability(CapabilityEndpointPlan::new(
            "example.health@1",
            "1.0.0",
            ["check"],
        ));
    let consumer = PluginInstancePlan::new("consumer", "fixture.consumer")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_requirement(
            CapabilityRequirementPlan::one("example.health@1", "1.0.0")
                .with_requirement_id("health"),
        );
    let plan = AppComposition::new(
        vec![consumer, provider],
        vec![
            CapabilityBinding::new("consumer", "example.health@1", "1.0.0", "health")
                .with_requirement_id("health"),
        ],
    )
    .resolve()
    .expect("complete-object provider plan should resolve");
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            NativePluginRegistry::new()
                .with_linked_factories()
                .with_factory(ConsumerFactory),
        ))
        .expect("authoring v2 provider should construct");

    assert_eq!(
        driver
            .run(app.invoke::<health::Health>("consumer", "check", ()))
            .expect("first request should reach the provider"),
        Ok(1)
    );
    assert_eq!(
        driver
            .run(app.invoke::<health::Health>("consumer", "check", ()))
            .expect("second request should reach the provider"),
        Ok(2)
    );
}

#[test]
fn complete_object_consumer_materializes_its_named_provider_during_construction() {
    let provider = PluginInstancePlan::new("health", "lenso")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_capability(CapabilityEndpointPlan::new(
            "example.health@1",
            "1.0.0",
            ["check"],
        ));
    let consumer = PluginInstancePlan::new("consumer", "fixture.bound-consumer")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_requirement(
            CapabilityRequirementPlan::one("example.health@1", "1.0.0")
                .with_requirement_id("health"),
        );
    let plan = AppComposition::new(
        vec![consumer, provider],
        vec![
            CapabilityBinding::new("consumer", "example.health@1", "1.0.0", "health")
                .with_requirement_id("health"),
        ],
    )
    .resolve()
    .expect("named complete-object dependency should resolve");
    let object = PluginObject::empty();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            NativePluginRegistry::new()
                .with_linked_factories()
                .with_factory(BoundConsumerFactory {
                    object: object.clone(),
                }),
        ))
        .expect("constructor should materialize a dependency while public admission is closed");
    let consumer = object.get().expect("consumer should construct once");
    assert_eq!(
        driver.run(consumer.health.invoke("check", ())).unwrap(),
        Ok(1)
    );
    assert_eq!(
        driver.run(app.shutdown(std::time::Duration::from_secs(1))),
        lenso_kernel::ShutdownOutcome::Clean
    );
}

#[test]
fn legacy_prepare_calls_its_constructed_complete_object_provider() {
    let plan = mixed_authoring_plan();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            NativePluginRegistry::new()
                .with_linked_factories()
                .with_factory(PreparingConsumerFactory { fail: false }),
        ))
        .expect("legacy prepare must be able to use its already-constructed v2 dependency");
    assert_eq!(
        driver
            .run(app.invoke::<health::Health>("consumer", "check", ()))
            .unwrap(),
        Ok(2)
    );
    assert_eq!(
        driver.run(app.shutdown(std::time::Duration::from_secs(1))),
        lenso_kernel::ShutdownOutcome::Clean
    );
}

fn mixed_authoring_plan() -> lenso_app_plan::ResolvedAppPlan {
    let provider = PluginInstancePlan::new("health", "lenso")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_capability(CapabilityEndpointPlan::new(
            "example.health@1",
            "1.0.0",
            ["check"],
        ));
    let consumer = PluginInstancePlan::new("consumer", "fixture.preparing-consumer")
        .with_requirement(CapabilityRequirementPlan::one("example.health@1", "1.0.0"));
    AppComposition::new(
        vec![consumer, provider],
        vec![CapabilityBinding::new(
            "consumer",
            "example.health@1",
            "1.0.0",
            "health",
        )],
    )
    .resolve()
    .expect("mixed-authoring dependency should resolve")
}

#[test]
fn later_prepare_failure_stops_the_constructed_provider_once_without_ready() {
    let stops = std::rc::Rc::new(Cell::new(0));
    let recorded_stops = stops.clone();
    let registry = NativePluginRegistry::new()
        .with_linked_factories()
        .with_factory_override(lenso_native_adapter::ConfiguredPluginFactory::<
            StatelessHealthPlugin,
            _,
        >::new(move |plugin| {
            plugin.stops = recorded_stops.clone();
            Ok(())
        }))
        .unwrap()
        .with_factory(PreparingConsumerFactory { fail: true });
    let driver = DeterministicDriver::new();
    let diagnostics = lenso_kernel::RuntimeDiagnostics::new();
    let observer = diagnostics.subscribe_all(64).unwrap();
    let result = driver.run(Kernel::start_native_with_diagnostics(
        mixed_authoring_plan(),
        driver.clone(),
        registry,
        diagnostics,
    ));
    assert!(
        matches!(result, Err(lenso_kernel::RuntimeFailure::PluginFailure { detail }) if detail == "dependent preparation failed after reading its provider")
    );
    assert_eq!(stops.get(), 1);
    while let Some(record) = observer.try_recv() {
        assert!(!matches!(
            record.event,
            lenso_kernel::DiagnosticEvent::AppReady
        ));
    }
    driver.run(driver.yield_now());
    assert_eq!(stops.get(), 1);
}

#[test]
fn stateless_plugin_rejects_non_empty_configuration() {
    let provider = PluginInstancePlan::new("health", "lenso")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_configuration(r#"{"unexpected":true}"#)
        .with_capability(CapabilityEndpointPlan::new(
            "example.health@1",
            "1.0.0",
            ["check"],
        ));
    let plan = AppComposition::new(vec![provider], vec![])
        .resolve()
        .expect("composition resolution defers Plugin-owned configuration validation");
    let driver = DeterministicDriver::new();
    let error = driver
        .run(Kernel::start_native(
            plan,
            driver.clone(),
            NativePluginRegistry::new().with_linked_factories(),
        ))
        .expect_err("stateless Plugin must reject non-empty configuration");
    assert!(format!("{error:?}").contains("does not accept configuration"));
}
