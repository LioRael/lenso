use super::{
    BTreeMap, BTreeSet, ErasedDomainResult, ErasedValue, ExecutionClassId, InvocationContext,
    LocalBoxFuture, ModuleLifecycle, NativeEventEndpoint, NativeStreamEndpoint, Rc,
    ResolvedAppPlan, RuntimeFailure,
};

/// Type-erased native endpoint used only while Kernel constructs and dispatches the graph.
pub trait NativeRequestEndpoint: std::fmt::Debug {
    /// Stable Capability series identity.
    fn capability_id(&self) -> &'static str;
    /// Exact Descriptor version implemented by this endpoint.
    fn descriptor_version(&self) -> &'static str;
    /// Exact stable Operation names implemented by this endpoint.
    fn operations(&self) -> &'static [&'static str];
    /// Dispatches one operation without serializing its typed Rust payload.
    fn invoke(
        &self,
        operation: &str,
        request: ErasedValue,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<ErasedDomainResult, RuntimeFailure>>;
}

/// One freshly prepared Module Instance generation returned by an Execution Adapter.
#[derive(Debug)]
pub struct PreparedNativeModule {
    pub(super) endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
    pub(super) stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
    pub(super) event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
    pub(super) lifecycle: Rc<dyn ModuleLifecycle>,
}

impl PreparedNativeModule {
    /// Creates one generation from its exact endpoint set and lifecycle Interface.
    pub fn new(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints: Vec::new(),
            event_endpoints: Vec::new(),
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Creates one generation from an already shared lifecycle implementation.
    pub fn with_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: Rc<dyn ModuleLifecycle>,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints: Vec::new(),
            event_endpoints: Vec::new(),
            lifecycle,
        }
    }

    /// Creates one generation with request and bidirectional stream endpoints.
    pub fn with_endpoints(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints,
            event_endpoints: Vec::new(),
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Creates one generation with shared lifecycle and request/stream endpoints.
    pub fn with_endpoints_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: Rc<dyn ModuleLifecycle>,
    ) -> Self {
        Self::with_all_endpoints_lifecycle(endpoints, stream_endpoints, Vec::new(), lifecycle)
    }

    /// Creates one generation with shared lifecycle and every endpoint kind.
    pub fn with_all_endpoints_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: Rc<dyn ModuleLifecycle>,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints,
            event_endpoints,
            lifecycle,
        }
    }

    /// Creates one generation containing only bidirectional stream endpoints.
    pub fn with_stream_endpoints(
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self::with_endpoints(Vec::new(), stream_endpoints, lifecycle)
    }

    /// Creates one generation containing only ephemeral Event endpoints.
    pub fn with_event_endpoints(
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self::with_all_endpoints(Vec::new(), Vec::new(), event_endpoints, lifecycle)
    }

    /// Creates one generation with request, stream, and ephemeral Event endpoints.
    pub fn with_all_endpoints(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: impl ModuleLifecycle,
    ) -> Self {
        Self {
            endpoints,
            stream_endpoints,
            event_endpoints,
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Returns the exact endpoints prepared for this generation.
    pub fn endpoints(&self) -> &[Rc<dyn NativeRequestEndpoint>] {
        &self.endpoints
    }

    /// Returns the exact stream endpoints prepared for this generation.
    pub fn stream_endpoints(&self) -> &[Rc<dyn NativeStreamEndpoint>] {
        &self.stream_endpoints
    }

    /// Returns the exact Event endpoints prepared for this generation.
    pub fn event_endpoints(&self) -> &[Rc<dyn NativeEventEndpoint>] {
        &self.event_endpoints
    }

    /// Returns the lifecycle Interface prepared for this generation.
    pub fn lifecycle(&self) -> Rc<dyn ModuleLifecycle> {
        self.lifecycle.clone()
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Vec<Rc<dyn NativeRequestEndpoint>>,
        Vec<Rc<dyn NativeStreamEndpoint>>,
        Vec<Rc<dyn NativeEventEndpoint>>,
        Rc<dyn ModuleLifecycle>,
    ) {
        (
            self.endpoints,
            self.stream_endpoints,
            self.event_endpoints,
            self.lifecycle,
        )
    }
}

/// One provider-specific binding prepared by an Execution Adapter.
#[derive(Clone, Debug)]
pub struct PreparedBinding {
    pub(super) consumer_instance: String,
    pub(super) provider_instance: String,
    pub(super) endpoint: Rc<dyn NativeRequestEndpoint>,
}

/// One provider-specific bidirectional stream binding prepared by an Adapter.
#[derive(Clone, Debug)]
pub struct PreparedStreamBinding {
    pub(super) consumer_instance: String,
    pub(super) provider_instance: String,
    pub(super) endpoint: Rc<dyn NativeStreamEndpoint>,
}

/// One provider-specific ephemeral Event binding prepared by an Adapter.
#[derive(Clone, Debug)]
pub struct PreparedEventBinding {
    pub(super) consumer_instance: String,
    pub(super) provider_instance: String,
    pub(super) endpoint: Rc<dyn NativeEventEndpoint>,
}

impl PreparedEventBinding {
    /// Binds one consumer to one exact Event endpoint and provider Instance.
    pub fn new(
        consumer_instance: impl Into<String>,
        provider_instance: impl Into<String>,
        endpoint: Rc<dyn NativeEventEndpoint>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Returns the App-local consumer Instance selected by the Plan.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the App-local provider Instance selected by the Plan.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the exact prepared Event endpoint referenced by this binding.
    pub fn endpoint(&self) -> Rc<dyn NativeEventEndpoint> {
        self.endpoint.clone()
    }

    pub(super) fn same_identity(&self, other: &Self) -> bool {
        self.consumer_instance == other.consumer_instance
            && self.provider_instance == other.provider_instance
            && self.endpoint.capability_id() == other.endpoint.capability_id()
    }
}

impl PreparedStreamBinding {
    /// Binds one consumer to one exact stream endpoint and provider Instance.
    pub fn new(
        consumer_instance: impl Into<String>,
        provider_instance: impl Into<String>,
        endpoint: Rc<dyn NativeStreamEndpoint>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Returns the App-local consumer Instance selected by the Plan.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the App-local provider Instance selected by the Plan.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the exact prepared stream endpoint referenced by this binding.
    pub fn endpoint(&self) -> Rc<dyn NativeStreamEndpoint> {
        self.endpoint.clone()
    }

    pub(super) fn same_identity(&self, other: &Self) -> bool {
        self.consumer_instance == other.consumer_instance
            && self.provider_instance == other.provider_instance
            && self.endpoint.capability_id() == other.endpoint.capability_id()
    }
}

impl PreparedBinding {
    /// Binds one consumer to the endpoint prepared for one exact provider Instance.
    pub fn new(
        consumer_instance: impl Into<String>,
        provider_instance: impl Into<String>,
        endpoint: Rc<dyn NativeRequestEndpoint>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Returns the App-local consumer Instance selected by the Plan.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }

    /// Returns the App-local provider Instance selected by the Plan.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }

    /// Returns the exact prepared endpoint referenced by this binding.
    pub fn endpoint(&self) -> Rc<dyn NativeRequestEndpoint> {
        self.endpoint.clone()
    }

    pub(super) fn same_identity(&self, other: &Self) -> bool {
        self.consumer_instance == other.consumer_instance
            && self.provider_instance == other.provider_instance
            && self.endpoint.capability_id() == other.endpoint.capability_id()
    }
}

/// Prepared native bindings returned by an Execution Adapter to Kernel.
#[derive(Debug)]
pub struct PreparedNativeApp {
    pub(super) bindings: Vec<PreparedBinding>,
    pub(super) stream_bindings: Vec<PreparedStreamBinding>,
    pub(super) event_bindings: Vec<PreparedEventBinding>,
    pub(super) generations: BTreeMap<String, PreparedNativeModule>,
}

impl PreparedNativeApp {
    /// Completes Adapter preparation with the full generation and binding tables.
    pub fn new(
        bindings: Vec<PreparedBinding>,
        generations: BTreeMap<String, PreparedNativeModule>,
    ) -> Self {
        Self {
            bindings,
            stream_bindings: Vec::new(),
            event_bindings: Vec::new(),
            generations,
        }
    }

    /// Creates the complete Adapter result for an empty Plan.
    pub fn empty() -> Self {
        Self::new(Vec::new(), BTreeMap::new())
    }

    /// Adds the exact bidirectional stream bindings prepared by an Adapter.
    #[must_use]
    pub fn with_stream_bindings(mut self, stream_bindings: Vec<PreparedStreamBinding>) -> Self {
        self.stream_bindings = stream_bindings;
        self
    }

    /// Adds the exact ephemeral Event bindings prepared by an Adapter.
    #[must_use]
    pub fn with_event_bindings(mut self, event_bindings: Vec<PreparedEventBinding>) -> Self {
        self.event_bindings = event_bindings;
        self
    }

    pub(super) fn merge(&mut self, other: Self) -> Result<(), RuntimeFailure> {
        for binding in other.bindings {
            if self
                .bindings
                .iter()
                .any(|existing| existing.same_identity(&binding))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared binding `{}:{}:{}`",
                        binding.consumer_instance,
                        binding.endpoint.capability_id(),
                        binding.provider_instance
                    ),
                });
            }
            self.bindings.push(binding);
        }
        for binding in other.stream_bindings {
            if self
                .stream_bindings
                .iter()
                .any(|existing| existing.same_identity(&binding))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared stream binding `{}:{}:{}`",
                        binding.consumer_instance,
                        binding.endpoint.capability_id(),
                        binding.provider_instance
                    ),
                });
            }
            self.stream_bindings.push(binding);
        }
        for binding in other.event_bindings {
            if self
                .event_bindings
                .iter()
                .any(|existing| existing.same_identity(&binding))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared Event binding `{}:{}:{}`",
                        binding.consumer_instance,
                        binding.endpoint.capability_id(),
                        binding.provider_instance
                    ),
                });
            }
            self.event_bindings.push(binding);
        }
        for (instance_key, generation) in other.generations {
            if self
                .generations
                .insert(instance_key.clone(), generation)
                .is_some()
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "multiple Execution Adapters prepared Module Instance generation `{instance_key}`"
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Host-specific seam that instantiates Module generations and prepares endpoints.
pub trait ExecutionAdapter: std::fmt::Debug + 'static {
    /// Returns the open execution class implemented by this Adapter package.
    fn execution_class(&self) -> ExecutionClassId;

    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;

    /// Creates a fresh generation for one selected Module Instance.
    ///
    /// Adapters that cannot truthfully recreate a generation retain the default
    /// failure, which lets Kernel apply the selected finite policy without
    /// pretending that an in-process fault boundary is recoverable.
    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        Err(RuntimeFailure::Internal {
            detail: format!("Execution Adapter cannot recreate Module Instance `{instance_key}`"),
        })
    }
}

/// Native Rust Adapter Interface for statically linked Module packages.
///
/// The blanket implementation below contributes every native Adapter to the
/// open catalog under the official native execution-class identity.
pub trait NativeExecutionAdapter: std::fmt::Debug + 'static {
    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;

    /// Creates a fresh generation for one selected native Module Instance.
    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        Err(RuntimeFailure::Internal {
            detail: format!("Execution Adapter cannot recreate Module Instance `{instance_key}`"),
        })
    }
}

impl<T: NativeExecutionAdapter> ExecutionAdapter for T {
    fn execution_class(&self) -> ExecutionClassId {
        ExecutionClassId::native_rust()
    }

    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        NativeExecutionAdapter::prepare(self, plan)
    }

    fn recreate(
        &self,
        plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativeModule, RuntimeFailure> {
        NativeExecutionAdapter::recreate(self, plan, instance_key)
    }
}

/// The execution classes contributed by installed Adapter packages.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionClassSet(BTreeSet<ExecutionClassId>);

impl ExecutionClassSet {
    /// Returns whether an installed Adapter provides this execution class.
    pub fn contains(&self, execution_class: &ExecutionClassId) -> bool {
        self.0.contains(execution_class)
    }

    /// Iterates the execution classes in deterministic identity order.
    pub fn iter(&self) -> impl Iterator<Item = &ExecutionClassId> {
        self.0.iter()
    }
}

/// A Runner could not assemble one unambiguous Adapter catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionAdapterCatalogError {
    /// More than one installed Adapter claimed the same execution class.
    DuplicateExecutionClass { execution_class: String },
}

impl std::fmt::Display for ExecutionAdapterCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateExecutionClass { execution_class } => write!(
                formatter,
                "multiple Execution Adapters provide class `{execution_class}`"
            ),
        }
    }
}

impl std::error::Error for ExecutionAdapterCatalogError {}

/// Immutable Adapter catalog assembled by a Runner before Kernel boot.
#[derive(Debug, Default)]
pub struct ExecutionAdapterCatalog {
    pub(super) adapters: BTreeMap<ExecutionClassId, Rc<dyn ExecutionAdapter>>,
}

impl ExecutionAdapterCatalog {
    /// Creates an empty catalog for an App with no Module Instances.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a catalog containing one Adapter package.
    pub fn single(adapter: impl ExecutionAdapter) -> Self {
        Self::new()
            .with_adapter(adapter)
            .expect("a new catalog cannot contain a duplicate execution class")
    }

    /// Installs one Adapter package under its open execution-class identity.
    pub fn with_adapter(
        self,
        adapter: impl ExecutionAdapter,
    ) -> Result<Self, ExecutionAdapterCatalogError> {
        self.with_shared_adapter(Rc::new(adapter))
    }

    /// Installs an Adapter package discovered as a runtime trait object.
    pub fn with_shared_adapter(
        mut self,
        adapter: Rc<dyn ExecutionAdapter>,
    ) -> Result<Self, ExecutionAdapterCatalogError> {
        let execution_class = adapter.execution_class();
        if self.adapters.contains_key(&execution_class) {
            return Err(ExecutionAdapterCatalogError::DuplicateExecutionClass {
                execution_class: execution_class.to_string(),
            });
        }
        self.adapters.insert(execution_class, adapter);
        Ok(self)
    }

    /// Returns the effective execution classes contributed by installed packages.
    pub fn execution_classes(&self) -> ExecutionClassSet {
        ExecutionClassSet(self.adapters.keys().cloned().collect())
    }

    pub(super) fn adapter(
        &self,
        execution_class: &ExecutionClassId,
    ) -> Option<Rc<dyn ExecutionAdapter>> {
        self.adapters.get(execution_class).cloned()
    }

    pub(super) fn prepare(
        &self,
        plan: &ResolvedAppPlan,
    ) -> Result<PreparedNativeApp, RuntimeFailure> {
        let mut required_classes = BTreeSet::new();
        for instance in plan.module_instances() {
            if !self.adapters.contains_key(instance.execution_class()) {
                return Err(RuntimeFailure::UnavailableExecutionClass {
                    instance_key: instance.instance_key().to_owned(),
                    execution_class: instance.execution_class().to_string(),
                });
            }
            required_classes.insert(instance.execution_class().clone());
        }

        let mut prepared = PreparedNativeApp::empty();
        for execution_class in required_classes {
            let adapter = self
                .adapters
                .get(&execution_class)
                .expect("required execution classes were validated");
            prepared.merge(adapter.prepare(plan)?)?;
        }
        Ok(prepared)
    }
}
