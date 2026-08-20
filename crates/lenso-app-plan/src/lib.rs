//! Typed execution input for the Lenso vNext Kernel.

/// The Resolved App Plan schema understood by this Kernel version.
pub const PLAN_SCHEMA_VERSION: u32 = 1;

/// Exact, immutable execution input supplied to the Kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAppPlan {
    schema_version: u32,
    module_instances: Vec<ModuleInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
}

/// One exact App-local Module Instance selected by the resolved Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleInstancePlan {
    instance_key: String,
    package_id: String,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
}

/// Exact Capability endpoint table expected from one Module Instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityEndpointPlan {
    capability_id: String,
    descriptor_version: String,
    operations: Vec<String>,
}

impl CapabilityEndpointPlan {
    /// Declares one exact Capability Descriptor and its stable Operation table.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            operations: operations.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the Capability series identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the exact stable Operation table.
    pub fn operations(&self) -> &[String] {
        &self.operations
    }
}

impl ModuleInstancePlan {
    /// Selects one statically linked package under an App-local Instance key.
    pub fn new(instance_key: impl Into<String>, package_id: impl Into<String>) -> Self {
        Self {
            instance_key: instance_key.into(),
            package_id: package_id.into(),
            provided_capabilities: Vec::new(),
        }
    }

    /// Declares one exact endpoint this Instance must prepare.
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpointPlan) -> Self {
        self.provided_capabilities.push(capability);
        self
    }

    /// Returns the App-local Instance key.
    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the selected package identity.
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Returns the exact endpoint set this Instance must prepare.
    pub fn provided_capabilities(&self) -> &[CapabilityEndpointPlan] {
        &self.provided_capabilities
    }
}

/// One exact consumer-to-provider Capability binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityBinding {
    consumer_instance: String,
    capability_id: String,
    descriptor_version: String,
    provider_instance: String,
}

impl CapabilityBinding {
    /// Binds one consumer to one provider at an exact Descriptor version.
    pub fn new(
        consumer_instance: impl Into<String>,
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        provider_instance: impl Into<String>,
    ) -> Self {
        Self {
            consumer_instance: consumer_instance.into(),
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            provider_instance: provider_instance.into(),
        }
    }

    /// Returns the consumer Instance key.
    pub fn consumer_instance(&self) -> &str {
        &self.consumer_instance
    }
    /// Returns the Capability series identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the provider Instance key.
    pub fn provider_instance(&self) -> &str {
        &self.provider_instance
    }
}

impl ResolvedAppPlan {
    /// Creates a valid Plan containing no Module Instances.
    pub const fn empty() -> Self {
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            module_instances: Vec::new(),
            capability_bindings: Vec::new(),
        }
    }

    /// Creates a resolved Plan with exact Module Instances and bindings.
    pub fn new(
        module_instances: Vec<ModuleInstancePlan>,
        capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
            module_instances,
            capability_bindings,
        }
    }

    /// Creates a Plan with an explicit schema version.
    ///
    /// This is primarily useful to decode authoring-tool output before validation.
    pub const fn with_schema_version(schema_version: u32) -> Self {
        Self {
            schema_version,
            module_instances: Vec::new(),
            capability_bindings: Vec::new(),
        }
    }

    /// Returns the Plan schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the exact Module Instances in deterministic Plan order.
    pub fn module_instances(&self) -> &[ModuleInstancePlan] {
        &self.module_instances
    }

    /// Returns the exact Capability bindings in deterministic Plan order.
    pub fn capability_bindings(&self) -> &[CapabilityBinding] {
        &self.capability_bindings
    }
}
