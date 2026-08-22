//! Kernel-owned fixtures for testing Runtime Drivers and Execution Adapters.
//!
//! The types in this crate deliberately carry no product semantics. They make
//! the Kernel Interface executable without depending on a concrete Adapter,
//! an example Capability package, or an example App.

use std::{collections::BTreeMap, fmt, rc::Rc};

use futures::future::LocalBoxFuture;
use lenso_app_plan::{ExecutionClassId, ModuleInstancePlan, ResolvedAppPlan};
use lenso_kernel::{
    ActivateContext, InvocationContext, ModuleLifecycle, NativeRequestEndpoint,
    NativeRequestHandle, NoopModuleLifecycle, PreparedBinding, PreparedNativeApp,
    PreparedNativeModule, RequestCapability, RuntimeFailure,
};

/// Stable identity used only by the runtime conformance suite.
pub const PROBE_CAPABILITY_ID: &str = "lenso.runtime.conformance.probe@1";
/// Exact conformance Descriptor version.
pub const PROBE_DESCRIPTOR_VERSION: &str = "1.0.0";
/// The request Operation exercised by the conformance suite.
pub const PROBE_OPERATION: &str = "probe";

/// Default provider package used by the conformance suite.
pub const PROBE_PROVIDER_PACKAGE_ID: &str = "lenso.runtime.conformance.probe-provider";
/// Replaceable provider package used to prove binding stability.
pub const ALTERNATE_PROBE_PROVIDER_PACKAGE_ID: &str =
    "lenso.runtime.conformance.alternate-probe-provider";
/// Consumer package used by the conformance suite.
pub const PROBE_CONSUMER_PACKAGE_ID: &str = "lenso.runtime.conformance.probe-consumer";

/// Request value transferred through the runtime seam.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeRequest {
    pub value: String,
}

/// Success value transferred through the runtime seam.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbeResponse {
    pub value: String,
}

/// Domain outcome used to prove that runtime and domain failures stay distinct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProbeError {
    EmptyValue,
}

/// Typed conformance Capability.
#[derive(Debug)]
pub struct Probe;

impl RequestCapability for Probe {
    type Request = ProbeRequest;
    type Response = ProbeResponse;
    type DomainError = ProbeError;

    const ID: &'static str = PROBE_CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = PROBE_DESCRIPTOR_VERSION;
}

/// Provider-side Interface for the conformance Capability.
pub trait ProbeProvider: fmt::Debug + 'static {
    fn probe(
        &self,
        context: InvocationContext,
        request: ProbeRequest,
    ) -> LocalBoxFuture<'static, Result<ProbeResponse, ProbeInvocationError>>;
}

/// Typed endpoint backed by one conformance provider.
#[derive(Debug)]
pub struct ProbeEndpoint<P> {
    provider: Rc<P>,
}

impl<P: ProbeProvider> ProbeEndpoint<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider: Rc::new(provider),
        }
    }
}

impl<P: ProbeProvider> NativeRequestEndpoint for ProbeEndpoint<P> {
    fn capability_id(&self) -> &'static str {
        PROBE_CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        PROBE_DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[PROBE_OPERATION]
    }

    fn invoke(
        &self,
        operation: &str,
        request: Box<dyn std::any::Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<
        'static,
        Result<Result<Box<dyn std::any::Any>, Box<dyn std::any::Any>>, RuntimeFailure>,
    > {
        if operation != PROBE_OPERATION {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::UnknownOperation {
                    capability: PROBE_CAPABILITY_ID,
                    operation: operation.to_owned(),
                },
            )));
        }
        let Ok(request) = request.downcast::<ProbeRequest>() else {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::ProtocolViolation {
                    capability: PROBE_CAPABILITY_ID,
                },
            )));
        };
        let provider = Rc::clone(&self.provider);
        Box::pin(async move {
            match provider.probe(context, *request).await {
                Ok(value) => Ok(Ok(Box::new(value) as Box<dyn std::any::Any>)),
                Err(ProbeInvocationError::Domain(error)) => {
                    Ok(Err(Box::new(error) as Box<dyn std::any::Any>))
                }
                Err(ProbeInvocationError::Runtime(error)) => Err(error),
            }
        })
    }
}

/// Consumer wrapper used by Driver and Adapter conformance tests.
#[derive(Debug)]
pub struct ProbeClient {
    handle: NativeRequestHandle<Probe>,
}

impl ProbeClient {
    pub fn new(handle: NativeRequestHandle<Probe>) -> Self {
        Self { handle }
    }

    pub fn from_dependencies(
        dependencies: &lenso_kernel::ModuleDependencies,
    ) -> Result<Self, RuntimeFailure> {
        Ok(Self::new(dependencies.one::<Probe>()?))
    }

    pub async fn probe(
        &self,
        request: ProbeRequest,
    ) -> Result<ProbeResponse, ProbeInvocationError> {
        self.handle
            .invoke(PROBE_OPERATION, request)
            .await
            .map_err(ProbeInvocationError::Runtime)?
            .map_err(ProbeInvocationError::Domain)
    }
}

/// Keeps typed domain outcomes separate from runtime failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProbeInvocationError {
    Domain(ProbeError),
    Runtime(RuntimeFailure),
}

/// One generation returned by the conformance Adapter.
#[derive(Debug)]
pub struct ConformanceModule {
    endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
    lifecycle: Rc<dyn ModuleLifecycle>,
}

impl ConformanceModule {
    pub fn new(endpoints: Vec<Rc<dyn NativeRequestEndpoint>>) -> Self {
        Self::with_lifecycle(endpoints, NoopModuleLifecycle)
    }

    pub fn with_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            lifecycle: Rc::new(lifecycle),
        }
    }

    fn prepared(&self) -> PreparedNativeModule {
        PreparedNativeModule::with_lifecycle(self.endpoints.clone(), self.lifecycle.clone())
    }
}

impl Default for ConformanceModule {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

/// Adapter-specific factory used only by the runtime conformance suite.
pub trait ConformanceModuleFactory: fmt::Debug + 'static {
    fn package_id(&self) -> &'static str;

    fn package_version(&self) -> &'static str {
        ""
    }

    fn instantiate(
        &self,
        instance: &ModuleInstancePlan,
    ) -> Result<ConformanceModule, RuntimeFailure>;
}

/// Request-only Execution Adapter used to test the Kernel Interface directly.
#[derive(Debug, Default)]
pub struct ConformanceExecutionAdapter {
    factories: Vec<Rc<dyn ConformanceModuleFactory>>,
}

impl ConformanceExecutionAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_factory(mut self, factory: impl ConformanceModuleFactory) -> Self {
        self.factories.push(Rc::new(factory));
        self
    }

    fn instantiate(
        &self,
        instance: &ModuleInstancePlan,
    ) -> Result<ConformanceModule, RuntimeFailure> {
        let matches = self
            .factories
            .iter()
            .filter(|factory| {
                factory.package_id() == instance.package_id()
                    && (instance.package_revision().is_empty()
                        || factory.package_version() == instance.package_revision())
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Err(RuntimeFailure::MissingModuleFactory {
                instance: instance.instance_key().to_owned(),
                package_id: instance.package_id().to_owned(),
            }),
            [factory] => factory.instantiate(instance),
            _ => Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "multiple conformance factories declare package `{}`",
                    instance.package_id()
                ),
            }),
        }
    }
}

impl lenso_kernel::NativeExecutionAdapter for ConformanceExecutionAdapter {
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        plan.validate()
            .map_err(|error| RuntimeFailure::InvalidResolvedPlan {
                detail: error.to_string(),
            })?;

        let mut modules = BTreeMap::new();
        let mut generations = BTreeMap::new();
        for instance in plan
            .module_instances()
            .iter()
            .filter(|instance| instance.execution_class() == &ExecutionClassId::native_rust())
        {
            let module = self.instantiate(instance)?;
            generations.insert(instance.instance_key().to_owned(), module.prepared());
            modules.insert(instance.instance_key().to_owned(), module);
        }

        let mut bindings = Vec::new();
        for binding in plan.capability_bindings() {
            let Some(module) = modules.get(binding.provider_instance()) else {
                continue;
            };
            let endpoint = module
                .endpoints
                .iter()
                .find(|endpoint| {
                    endpoint.capability_id() == binding.capability_id()
                        && endpoint.descriptor_version() == binding.descriptor_version()
                })
                .cloned()
                .ok_or_else(|| RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Capability `{}` version `{}` has no request endpoint on provider `{}`",
                        binding.capability_id(),
                        binding.descriptor_version(),
                        binding.provider_instance()
                    ),
                })?;
            bindings.push(PreparedBinding::new(
                binding.consumer_instance(),
                binding.provider_instance(),
                endpoint,
            ));
        }

        Ok(PreparedNativeApp::new(bindings, generations))
    }

    fn recreate(
        &self,
        plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        let instance = plan.module_instance(instance_key).ok_or_else(|| {
            RuntimeFailure::InvalidResolvedPlan {
                detail: format!("unknown Module Instance `{instance_key}`"),
            }
        })?;
        Ok(self.instantiate(instance)?.prepared())
    }
}

/// Default consumer factory. It verifies its singular dependency during activation.
#[derive(Debug)]
pub struct ProbeConsumerFactory;

impl ConformanceModuleFactory for ProbeConsumerFactory {
    fn package_id(&self) -> &'static str {
        PROBE_CONSUMER_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn instantiate(
        &self,
        _instance: &ModuleInstancePlan,
    ) -> Result<ConformanceModule, RuntimeFailure> {
        Ok(ConformanceModule::with_lifecycle(
            Vec::new(),
            ProbeConsumerLifecycle,
        ))
    }
}

#[derive(Debug)]
struct ProbeConsumerLifecycle;

impl ModuleLifecycle for ProbeConsumerLifecycle {
    fn activate(&self, context: ActivateContext) -> lenso_kernel::ModuleFuture {
        let client = (context.dependencies().len() == 1)
            .then(|| ProbeClient::from_dependencies(context.dependencies()));
        Box::pin(async move {
            let Some(client) = client else {
                return Ok(());
            };
            match client?
                .probe(ProbeRequest {
                    value: "activation".to_owned(),
                })
                .await
            {
                Ok(_) => Ok(()),
                Err(ProbeInvocationError::Runtime(error)) => Err(error),
                Err(ProbeInvocationError::Domain(error)) => Err(RuntimeFailure::ModuleFailure {
                    detail: format!("probe activation dependency returned {error:?}"),
                }),
            }
        })
    }
}

/// Default provider used by the conformance suite.
#[derive(Debug)]
pub struct ProbeProviderFactory;

impl ConformanceModuleFactory for ProbeProviderFactory {
    fn package_id(&self) -> &'static str {
        PROBE_PROVIDER_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn instantiate(
        &self,
        _instance: &ModuleInstancePlan,
    ) -> Result<ConformanceModule, RuntimeFailure> {
        Ok(ConformanceModule::new(vec![Rc::new(ProbeEndpoint::new(
            EchoProbe("Echo"),
        ))]))
    }
}

/// Alternate implementation used to prove that bindings do not name implementations.
#[derive(Debug)]
pub struct AlternateProbeProviderFactory;

impl ConformanceModuleFactory for AlternateProbeProviderFactory {
    fn package_id(&self) -> &'static str {
        ALTERNATE_PROBE_PROVIDER_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn instantiate(
        &self,
        _instance: &ModuleInstancePlan,
    ) -> Result<ConformanceModule, RuntimeFailure> {
        Ok(ConformanceModule::new(vec![Rc::new(ProbeEndpoint::new(
            EchoProbe("Alternate"),
        ))]))
    }
}

#[derive(Debug)]
struct EchoProbe(&'static str);

impl ProbeProvider for EchoProbe {
    fn probe(
        &self,
        _context: InvocationContext,
        request: ProbeRequest,
    ) -> LocalBoxFuture<'static, Result<ProbeResponse, ProbeInvocationError>> {
        let prefix = self.0;
        Box::pin(async move {
            if request.value.is_empty() {
                Err(ProbeInvocationError::Domain(ProbeError::EmptyValue))
            } else {
                Ok(ProbeResponse {
                    value: format!("{prefix}: {}", request.value),
                })
            }
        })
    }
}
