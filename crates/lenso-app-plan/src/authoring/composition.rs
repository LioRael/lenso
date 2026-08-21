use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_configuration() -> Value {
    Value::Object(serde_json::Map::new())
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CapabilityEndpoint {
    capability_id: String,
    descriptor_version: String,
    operations: Vec<String>,
    #[serde(default)]
    operation_kinds: BTreeMap<String, InteractionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admission: Option<RequestAdmission>,
    #[serde(default)]
    operation_admissions: BTreeMap<String, RequestAdmission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_capacity: Option<usize>,
}

impl CapabilityEndpoint {
    /// Declares request Operations for one Capability endpoint.
    pub fn request(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            operations: operations.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Declares one endpoint and marks all supplied Operations as streams.
    pub fn stream(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut endpoint = Self::request(capability_id, descriptor_version, operations);
        for operation in endpoint.operations.clone() {
            endpoint
                .operation_kinds
                .insert(operation, InteractionKind::Stream);
        }
        endpoint
    }

    /// Declares one endpoint and marks all supplied Operations as Events.
    pub fn event(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut endpoint = Self::request(capability_id, descriptor_version, operations);
        for operation in endpoint.operations.clone() {
            endpoint
                .operation_kinds
                .insert(operation, InteractionKind::Event);
        }
        endpoint
    }

    /// Sets one Operation's interaction kind.
    #[must_use]
    pub fn with_operation_kind(
        mut self,
        operation: impl Into<String>,
        kind: InteractionKind,
    ) -> Self {
        self.operation_kinds.insert(operation.into(), kind);
        self
    }

    /// Applies one bounded request policy to every request Operation.
    #[must_use]
    pub fn with_admission(mut self, admission: RequestAdmission) -> Self {
        self.admission = Some(admission);
        self
    }

    /// Applies one bounded Event mailbox capacity.
    #[must_use]
    pub fn with_event_capacity(mut self, capacity: usize) -> Self {
        self.event_capacity = Some(capacity);
        self
    }

    /// Returns the Capability identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the declared Operations.
    pub fn operations(&self) -> &[String] {
        &self.operations
    }
    /// Returns the authored interaction kinds.
    pub fn operation_kinds(&self) -> &BTreeMap<String, InteractionKind> {
        &self.operation_kinds
    }
    /// Returns the endpoint-wide request policy.
    pub const fn admission(&self) -> Option<RequestAdmission> {
        self.admission
    }
    /// Returns Operation-specific request policies.
    pub fn operation_admissions(&self) -> &BTreeMap<String, RequestAdmission> {
        &self.operation_admissions
    }
    /// Returns the Event mailbox capacity.
    pub const fn event_capacity(&self) -> Option<usize> {
        self.event_capacity
    }
}

/// Transport-independent interaction kind in authoring data.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractionKind {
    /// One request produces one response.
    #[default]
    Request,
    /// One open establishes a bidirectional stream.
    Stream,
    /// One publication is delivered to subscribers.
    Event,
}

/// Bounded request queue and concurrency policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestAdmission {
    queue_capacity: usize,
    max_concurrency: usize,
}

impl RequestAdmission {
    /// Creates one request admission policy.
    pub const fn new(queue_capacity: usize, max_concurrency: usize) -> Self {
        Self {
            queue_capacity,
            max_concurrency,
        }
    }
    /// Returns the queue capacity.
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }
    /// Returns the concurrency limit.
    pub const fn max_concurrency(self) -> usize {
        self.max_concurrency
    }
}

/// One Capability requirement declared by a Module Instance.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapabilityRequirement {
    capability_id: String,
    descriptor_version: String,
    cardinality: Cardinality,
}

impl CapabilityRequirement {
    /// Declares an exactly-one requirement.
    pub fn one(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(capability_id, descriptor_version, Cardinality::One)
    }
    /// Declares an optional requirement.
    pub fn optional(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
    ) -> Self {
        Self::new(capability_id, descriptor_version, Cardinality::Optional)
    }
    /// Declares a deterministic many-provider requirement.
    pub fn many(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(capability_id, descriptor_version, Cardinality::Many)
    }
    /// Declares a requirement with an explicit cardinality.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            cardinality,
        }
    }
    /// Returns the Capability identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the requirement cardinality.
    pub const fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}

/// Binding cardinality in App Composition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cardinality {
    One,
    Optional,
    Many,
}

/// One consumer-to-provider binding selected before Kernel boot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Binding {
    consumer: String,
    capability_id: String,
    descriptor_version: String,
    provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admission: Option<RequestAdmission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_capacity: Option<usize>,
}

impl Binding {
    /// Binds a consumer requirement to one explicit provider Instance.
    pub fn new(
        consumer: impl Into<String>,
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            consumer: consumer.into(),
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            provider: provider.into(),
            admission: None,
            event_capacity: None,
        }
    }
    /// Overrides request admission for this binding.
    #[must_use]
    pub fn with_admission(mut self, admission: RequestAdmission) -> Self {
        self.admission = Some(admission);
        self
    }
    /// Overrides Event mailbox capacity for this binding.
    #[must_use]
    pub fn with_event_capacity(mut self, capacity: usize) -> Self {
        self.event_capacity = Some(capacity);
        self
    }
    /// Returns the consumer Instance key.
    pub fn consumer(&self) -> &str {
        &self.consumer
    }
    /// Returns the Capability identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the provider Instance key.
    pub fn provider(&self) -> &str {
        &self.provider
    }
    /// Returns an explicit request policy.
    pub const fn admission(&self) -> Option<RequestAdmission> {
        self.admission
    }
    /// Returns an explicit Event capacity.
    pub const fn event_capacity(&self) -> Option<usize> {
        self.event_capacity
    }
}

/// One Module Instance in App Composition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Module {
    key: String,
    package: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default = "default_configuration")]
    configuration: Value,
    #[serde(default)]
    provides: Vec<CapabilityEndpoint>,
    #[serde(default)]
    requires: Vec<CapabilityRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<ModuleRole>,
}

/// Target-owned Web UI role selected by an authoring profile.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRole {
    WebShell,
    BrowserAdapter,
    UiContribution,
}

fn default_entrypoint() -> String {
    "default".to_owned()
}

impl Module {
    /// Selects one package under an App-local Instance key.
    pub fn new(key: impl Into<String>, package: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            package: package.into(),
            entrypoint: default_entrypoint(),
            configuration: default_configuration(),
            provides: Vec::new(),
            requires: Vec::new(),
            execution_class: None,
            configuration_schema: None,
            role: None,
        }
    }
    /// Selects an explicit package entrypoint.
    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }
    /// Supplies opaque non-secret Module configuration.
    #[must_use]
    pub fn with_configuration(mut self, configuration: Value) -> Self {
        self.configuration = configuration;
        self
    }
    /// Replaces Module configuration in an existing authoring document.
    pub fn set_configuration(&mut self, configuration: Value) {
        self.configuration = configuration;
    }
    /// Associates a JSON Schema used by `check` for configuration shape.
    #[must_use]
    pub fn with_configuration_schema(mut self, path: impl Into<String>) -> Self {
        self.configuration_schema = Some(path.into());
        self
    }
    /// Declares an explicit target-owned Web UI role.
    #[must_use]
    pub const fn with_role(mut self, role: ModuleRole) -> Self {
        self.role = Some(role);
        self
    }
    /// Declares one provided Capability endpoint.
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpoint) -> Self {
        self.provides.push(capability);
        self
    }
    /// Declares one required Capability.
    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirement) -> Self {
        self.requires.push(requirement);
        self
    }
    /// Selects an explicit Execution Adapter class.
    pub fn set_execution_class(&mut self, execution_class: impl Into<String>) {
        self.execution_class = Some(execution_class.into());
    }
    /// Returns the Instance key.
    pub fn key(&self) -> &str {
        &self.key
    }
    /// Returns the package identity.
    pub fn package(&self) -> &str {
        &self.package
    }
    /// Returns the entrypoint.
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }
    /// Returns Module configuration.
    pub fn configuration(&self) -> &Value {
        &self.configuration
    }
    /// Returns provided Capability endpoints.
    pub fn provides(&self) -> &[CapabilityEndpoint] {
        &self.provides
    }
    /// Returns required Capabilities.
    pub fn requires(&self) -> &[CapabilityRequirement] {
        &self.requires
    }
    /// Returns the selected Execution Adapter class, when explicit.
    pub fn execution_class(&self) -> Option<&str> {
        self.execution_class.as_deref()
    }
    /// Returns the optional configuration schema path.
    pub fn configuration_schema(&self) -> Option<&str> {
        self.configuration_schema.as_deref()
    }
    pub const fn role(&self) -> Option<ModuleRole> {
        self.role
    }
}
