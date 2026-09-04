use std::any::Any;

use super::{
    BTreeMap, BTreeSet, ErasedDomainResult, ErasedValue, ExecutionClassId, InvocationContext,
    LocalBoxFuture, NativeEventEndpoint, NativeStreamEndpoint, PluginLifecycle, Rc,
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
    /// Exposes a generated endpoint to its matching typed Capability binding.
    ///
    /// Hand-written and older generated endpoints use the default erased path.
    #[doc(hidden)]
    fn typed_endpoint(&self) -> Option<&dyn Any> {
        None
    }
    /// Dispatches one operation without serializing its typed Rust payload.
    fn invoke(
        &self,
        operation: &str,
        request: ErasedValue,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<ErasedDomainResult, RuntimeFailure>>;
}

/// The complete native endpoint set owned by one Plugin generation.
#[derive(Clone, Debug, Default)]
pub struct NativeEndpointSet {
    request: Vec<Rc<dyn NativeRequestEndpoint>>,
    stream: Vec<Rc<dyn NativeStreamEndpoint>>,
    event: Vec<Rc<dyn NativeEventEndpoint>>,
}

impl NativeEndpointSet {
    /// Creates an endpoint set containing every native interaction kind.
    pub fn new(
        request: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream: Vec<Rc<dyn NativeStreamEndpoint>>,
        event: Vec<Rc<dyn NativeEventEndpoint>>,
    ) -> Self {
        Self {
            request,
            stream,
            event,
        }
    }

    /// Returns the request endpoints in this generation.
    pub fn request(&self) -> &[Rc<dyn NativeRequestEndpoint>] {
        &self.request
    }

    /// Returns the stream endpoints in this generation.
    pub fn stream(&self) -> &[Rc<dyn NativeStreamEndpoint>] {
        &self.stream
    }

    /// Returns the Event endpoints in this generation.
    pub fn event(&self) -> &[Rc<dyn NativeEventEndpoint>] {
        &self.event
    }
}

/// One freshly prepared Plugin Instance generation returned by an Execution Adapter.
#[derive(Debug)]
pub struct PreparedNativePlugin {
    pub(super) endpoints: NativeEndpointSet,
    pub(super) lifecycle: Rc<dyn PluginLifecycle>,
}

impl PreparedNativePlugin {
    /// Creates one generation from its exact endpoint set and lifecycle Interface.
    pub fn new(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: impl PluginLifecycle,
    ) -> Self {
        Self {
            endpoints: NativeEndpointSet::new(endpoints, Vec::new(), Vec::new()),
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Creates one generation from an already shared lifecycle implementation.
    pub fn with_lifecycle(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        lifecycle: Rc<dyn PluginLifecycle>,
    ) -> Self {
        Self {
            endpoints: NativeEndpointSet::new(endpoints, Vec::new(), Vec::new()),
            lifecycle,
        }
    }

    /// Creates one generation with request and bidirectional stream endpoints.
    pub fn with_endpoints(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: impl PluginLifecycle,
    ) -> Self {
        Self {
            endpoints: NativeEndpointSet::new(endpoints, stream_endpoints, Vec::new()),
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Creates a generation from one complete endpoint set and shared lifecycle.
    pub fn with_endpoint_set_lifecycle(
        endpoints: NativeEndpointSet,
        lifecycle: Rc<dyn PluginLifecycle>,
    ) -> Self {
        Self {
            endpoints,
            lifecycle,
        }
    }

    /// Creates one generation containing only bidirectional stream endpoints.
    pub fn with_stream_endpoints(
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        lifecycle: impl PluginLifecycle,
    ) -> Self {
        Self::with_endpoints(Vec::new(), stream_endpoints, lifecycle)
    }

    /// Creates one generation containing only ephemeral Event endpoints.
    pub fn with_event_endpoints(
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: impl PluginLifecycle,
    ) -> Self {
        Self::with_all_endpoints(Vec::new(), Vec::new(), event_endpoints, lifecycle)
    }

    /// Creates one generation with request, stream, and ephemeral Event endpoints.
    pub fn with_all_endpoints(
        endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
        stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
        event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
        lifecycle: impl PluginLifecycle,
    ) -> Self {
        Self {
            endpoints: NativeEndpointSet::new(endpoints, stream_endpoints, event_endpoints),
            lifecycle: Rc::new(lifecycle),
        }
    }

    /// Returns the exact endpoints prepared for this generation.
    pub fn endpoints(&self) -> &[Rc<dyn NativeRequestEndpoint>] {
        self.endpoints.request()
    }

    /// Returns the exact stream endpoints prepared for this generation.
    pub fn stream_endpoints(&self) -> &[Rc<dyn NativeStreamEndpoint>] {
        self.endpoints.stream()
    }

    /// Returns the exact Event endpoints prepared for this generation.
    pub fn event_endpoints(&self) -> &[Rc<dyn NativeEventEndpoint>] {
        self.endpoints.event()
    }

    /// Returns the lifecycle Interface prepared for this generation.
    pub fn lifecycle(&self) -> Rc<dyn PluginLifecycle> {
        self.lifecycle.clone()
    }

    pub(super) fn into_parts(self) -> (NativeEndpointSet, Rc<dyn PluginLifecycle>) {
        (self.endpoints, self.lifecycle)
    }
}

/// One provider-specific binding prepared by an Execution Adapter.
#[derive(Clone, Debug)]
pub struct PreparedBinding {
    pub(super) requirement_id: String,
    pub(super) consumer_instance: String,
    pub(super) provider_instance: String,
    pub(super) endpoint: Rc<dyn NativeRequestEndpoint>,
}

/// One provider-specific bidirectional stream binding prepared by an Adapter.
#[derive(Clone, Debug)]
pub struct PreparedStreamBinding {
    pub(super) requirement_id: String,
    pub(super) consumer_instance: String,
    pub(super) provider_instance: String,
    pub(super) endpoint: Rc<dyn NativeStreamEndpoint>,
}

/// One provider-specific ephemeral Event binding prepared by an Adapter.
#[derive(Clone, Debug)]
pub struct PreparedEventBinding {
    pub(super) requirement_id: String,
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
            requirement_id: format!("~{}", endpoint.capability_id()),
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Selects the consumer-local requirement prepared by this Adapter.
    #[must_use]
    pub fn with_requirement_id(mut self, id: impl Into<String>) -> Self {
        self.requirement_id = id.into();
        self
    }

    /// Returns the exact consumer-local requirement identity.
    pub fn requirement_id(&self) -> &str {
        &self.requirement_id
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
        self.requirement_id == other.requirement_id
            && self.consumer_instance == other.consumer_instance
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
            requirement_id: format!("~{}", endpoint.capability_id()),
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Selects the consumer-local requirement prepared by this Adapter.
    #[must_use]
    pub fn with_requirement_id(mut self, id: impl Into<String>) -> Self {
        self.requirement_id = id.into();
        self
    }

    /// Returns the exact consumer-local requirement identity.
    pub fn requirement_id(&self) -> &str {
        &self.requirement_id
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
        self.requirement_id == other.requirement_id
            && self.consumer_instance == other.consumer_instance
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
            requirement_id: format!("~{}", endpoint.capability_id()),
            consumer_instance: consumer_instance.into(),
            provider_instance: provider_instance.into(),
            endpoint,
        }
    }

    /// Selects the consumer-local requirement prepared by this Adapter.
    #[must_use]
    pub fn with_requirement_id(mut self, id: impl Into<String>) -> Self {
        self.requirement_id = id.into();
        self
    }

    /// Returns the exact consumer-local requirement identity.
    pub fn requirement_id(&self) -> &str {
        &self.requirement_id
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
        self.requirement_id == other.requirement_id
            && self.consumer_instance == other.consumer_instance
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
    pub(super) generations: BTreeMap<String, PreparedNativePlugin>,
}

impl PreparedNativeApp {
    /// Completes Adapter preparation with the full generation and binding tables.
    pub fn new(
        bindings: Vec<PreparedBinding>,
        generations: BTreeMap<String, PreparedNativePlugin>,
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
                        "multiple Execution Adapters prepared Plugin Instance generation `{instance_key}`"
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Host-specific seam that instantiates Plugin generations and prepares endpoints.
pub trait ExecutionAdapter: std::fmt::Debug + 'static {
    /// Confirms a versioned authoring/profile pair before any Plugin is prepared.
    fn supports_runtime_profile(&self, authoring_version: u32, profile: &str) -> bool {
        authoring_version == 1
            && (profile == self.execution_class().as_str()
                || (self.execution_class() == ExecutionClassId::native_rust()
                    && profile == "lenso.native-authoring@1"))
    }
    /// Returns the open execution class implemented by this Adapter package.
    fn execution_class(&self) -> ExecutionClassId;

    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;

    /// Creates a fresh generation for one selected Plugin Instance.
    ///
    /// Adapters that cannot truthfully recreate a generation retain the default
    /// failure, which lets Kernel apply the selected finite policy without
    /// pretending that an in-process fault boundary is recoverable.
    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativePlugin, RuntimeFailure> {
        Err(RuntimeFailure::Internal {
            detail: format!("Execution Adapter cannot recreate Plugin Instance `{instance_key}`"),
        })
    }
}

/// Native Rust Adapter Interface for statically linked Plugin Releases.
///
/// The blanket implementation below contributes every native Adapter to the
/// open catalog under the official native execution-class identity.
pub trait NativeExecutionAdapter: std::fmt::Debug + 'static {
    /// Old native Adapters must opt in explicitly before receiving version 2 contracts.
    fn supports_runtime_profile(&self, authoring_version: u32, profile: &str) -> bool {
        authoring_version == 1
            && matches!(profile, "lenso.native-authoring@1" | "lenso.native-rust@1")
    }
    /// Instantiates the exact Plan and confirms its endpoint and binding tables.
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure>;

    /// Creates a fresh generation for one selected native Plugin Instance.
    fn recreate(
        &self,
        _plan: &ResolvedAppPlan,
        instance_key: &str,
    ) -> Result<PreparedNativePlugin, RuntimeFailure> {
        Err(RuntimeFailure::Internal {
            detail: format!("Execution Adapter cannot recreate Plugin Instance `{instance_key}`"),
        })
    }
}

impl<T: NativeExecutionAdapter> ExecutionAdapter for T {
    fn supports_runtime_profile(&self, authoring_version: u32, profile: &str) -> bool {
        NativeExecutionAdapter::supports_runtime_profile(self, authoring_version, profile)
    }
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
    ) -> Result<PreparedNativePlugin, RuntimeFailure> {
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
    /// Creates an empty catalog for an App with no Plugin Instances.
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
        for instance in plan.plugin_instances() {
            if !self.adapters.contains_key(instance.execution_class()) {
                return Err(RuntimeFailure::UnavailableExecutionClass {
                    instance_key: instance.instance_key().to_owned(),
                    execution_class: instance.execution_class().to_string(),
                });
            }
            required_classes.insert(instance.execution_class().clone());
            if !self.adapters[instance.execution_class()]
                .supports_runtime_profile(instance.authoring_version(), instance.runtime_profile())
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Adapter `{}` does not support authoring {} profile `{}` for `{}`",
                        instance.execution_class(),
                        instance.authoring_version(),
                        instance.runtime_profile(),
                        instance.instance_key()
                    ),
                });
            }
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
