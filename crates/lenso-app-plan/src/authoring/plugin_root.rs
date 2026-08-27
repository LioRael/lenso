use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::configuration::{ConfigurationError, resolve_configuration_layers};
use crate::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ExecutionClassId, ExecutionLaneId, ExecutionLanePlan,
    PluginCriticality, PluginInstancePlan, RequestAdmissionPlan, ResolvedAppPlan, RestartPolicy,
};

fn empty_configuration() -> Value {
    Value::Object(serde_json::Map::new())
}

fn is_empty_configuration(configuration: &Value) -> bool {
    configuration == &empty_configuration()
}

fn default_entrypoint() -> String {
    "default".to_owned()
}

/// Stable App-local identity of one Plugin Instance.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInstanceId {
    plugin_id: String,
    instance_key: String,
}

impl PluginInstanceId {
    pub fn new(plugin_id: impl Into<String>, instance_key: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            instance_key: instance_key.into(),
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    /// Returns the unambiguous private key lowered into the current Plan schema.
    pub fn plan_key(&self) -> String {
        format!("{}/{}", self.plugin_id, self.instance_key)
    }
}

impl fmt::Display for PluginInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.plugin_id, self.instance_key)
    }
}

/// Generated facts for one executable Plugin Release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginDescriptor {
    plugin_id: String,
    release_version: String,
    root_slot: String,
    runtime_package_id: String,
    runtime_package_revision: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration_schema: Option<Value>,
    #[serde(
        default = "empty_configuration",
        skip_serializing_if = "is_empty_configuration"
    )]
    configuration_defaults: Value,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
    required_capabilities: Vec<CapabilityRequirementPlan>,
    execution_class: ExecutionClassId,
    restart_policy: RestartPolicy,
    criticality: PluginCriticality,
}

impl PluginDescriptor {
    pub fn new(
        plugin_id: impl Into<String>,
        release_version: impl Into<String>,
        root_slot: impl Into<String>,
    ) -> Self {
        let plugin_id = plugin_id.into();
        let release_version = release_version.into();
        Self {
            runtime_package_id: plugin_id.clone(),
            runtime_package_revision: release_version.clone(),
            plugin_id,
            release_version,
            root_slot: root_slot.into(),
            entrypoint: default_entrypoint(),
            configuration_schema: None,
            configuration_defaults: empty_configuration(),
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
            execution_class: ExecutionClassId::native_rust(),
            restart_policy: RestartPolicy::default(),
            criticality: PluginCriticality::default(),
        }
    }

    #[must_use]
    pub fn with_runtime_package(
        mut self,
        package_id: impl Into<String>,
        package_revision: impl Into<String>,
    ) -> Self {
        self.runtime_package_id = package_id.into();
        self.runtime_package_revision = package_revision.into();
        self
    }

    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }

    #[must_use]
    pub fn with_configuration_schema(mut self, schema: Value) -> Self {
        self.configuration_schema = Some(schema);
        self
    }

    #[must_use]
    pub fn with_configuration_defaults(mut self, defaults: Value) -> Self {
        self.configuration_defaults = defaults;
        self
    }

    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpointPlan) -> Self {
        self.provided_capabilities.push(capability);
        self
    }

    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirementPlan) -> Self {
        self.required_capabilities.push(requirement);
        self
    }

    #[must_use]
    pub fn with_execution_class(mut self, execution_class: ExecutionClassId) -> Self {
        self.execution_class = execution_class;
        self
    }

    #[must_use]
    pub fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    #[must_use]
    pub fn with_criticality(mut self, criticality: PluginCriticality) -> Self {
        self.criticality = criticality;
        self
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn release_version(&self) -> &str {
        &self.release_version
    }

    pub fn root_slot(&self) -> &str {
        &self.root_slot
    }

    pub fn runtime_package_id(&self) -> &str {
        &self.runtime_package_id
    }

    pub fn runtime_package_revision(&self) -> &str {
        &self.runtime_package_revision
    }

    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }

    pub const fn configuration_schema(&self) -> Option<&Value> {
        self.configuration_schema.as_ref()
    }

    pub const fn configuration_defaults(&self) -> &Value {
        &self.configuration_defaults
    }

    pub fn provided_capabilities(&self) -> &[CapabilityEndpointPlan] {
        &self.provided_capabilities
    }

    pub fn required_capabilities(&self) -> &[CapabilityRequirementPlan] {
        &self.required_capabilities
    }

    pub fn execution_class(&self) -> &ExecutionClassId {
        &self.execution_class
    }

    pub const fn restart_policy(&self) -> RestartPolicy {
        self.restart_policy
    }

    pub const fn criticality(&self) -> PluginCriticality {
        self.criticality
    }
}

/// Provider cardinality owned by one Host root Slot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSlotCardinality {
    One,
    Optional,
    Many,
}

/// One Host-owned root attachment point.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostSlot {
    id: String,
    cardinality: HostSlotCardinality,
    #[serde(default)]
    replaceable: bool,
    #[serde(default = "default_execution_lane")]
    execution_lane: String,
}

fn default_execution_lane() -> String {
    "main".to_owned()
}

impl HostSlot {
    pub fn one(id: impl Into<String>) -> Self {
        Self::new(id, HostSlotCardinality::One)
    }

    pub fn optional(id: impl Into<String>) -> Self {
        Self::new(id, HostSlotCardinality::Optional)
    }

    pub fn many(id: impl Into<String>) -> Self {
        Self::new(id, HostSlotCardinality::Many)
    }

    fn new(id: impl Into<String>, cardinality: HostSlotCardinality) -> Self {
        Self {
            id: id.into(),
            cardinality,
            replaceable: false,
            execution_lane: default_execution_lane(),
        }
    }

    #[must_use]
    pub const fn replaceable(mut self) -> Self {
        self.replaceable = true;
        self
    }

    #[must_use]
    pub fn with_execution_lane(mut self, execution_lane: impl Into<String>) -> Self {
        self.execution_lane = execution_lane.into();
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn cardinality(&self) -> HostSlotCardinality {
        self.cardinality
    }

    pub const fn is_replaceable(&self) -> bool {
        self.replaceable
    }

    pub fn execution_lane(&self) -> &str {
        &self.execution_lane
    }
}

/// One exact Plugin Release available to a Host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostPluginRelease {
    descriptor: PluginDescriptor,
    #[serde(default)]
    allow_root_override: bool,
}

impl HostPluginRelease {
    pub const fn new(descriptor: PluginDescriptor) -> Self {
        Self {
            descriptor,
            allow_root_override: false,
        }
    }

    #[must_use]
    pub const fn allow_root_override(mut self) -> Self {
        self.allow_root_override = true;
        self
    }

    pub const fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub const fn root_override_allowed(&self) -> bool {
        self.allow_root_override
    }
}

/// One Plugin Instance supplied by Host defaults.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostDefaultPlugin {
    id: PluginInstanceId,
    #[serde(default = "empty_configuration")]
    configuration: Value,
    #[serde(default)]
    disableable: bool,
}

/// Host-owned configuration for an Instance that becomes active only when the
/// App owner adds the matching Plugin Root entry.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostPluginConfiguration {
    id: PluginInstanceId,
    #[serde(default = "empty_configuration")]
    configuration: Value,
}

impl HostPluginConfiguration {
    pub fn new(
        plugin_id: impl Into<String>,
        instance_key: impl Into<String>,
        configuration: Value,
    ) -> Self {
        Self {
            id: PluginInstanceId::new(plugin_id, instance_key),
            configuration,
        }
    }

    pub const fn id(&self) -> &PluginInstanceId {
        &self.id
    }

    pub const fn configuration(&self) -> &Value {
        &self.configuration
    }
}

/// One Host-private attachment from a default Plugin Instance to a provider Slot.
///
/// This resolves repeated Capability providers without exposing binding decisions
/// in the user-authored Plugin Root.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostBinding {
    consumer: PluginInstanceId,
    capability_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_slot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_instance: Option<PluginInstanceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admission: Option<RequestAdmissionPlan>,
}

impl HostBinding {
    pub fn new(
        consumer: PluginInstanceId,
        capability_id: impl Into<String>,
        provider_slot: impl Into<String>,
    ) -> Self {
        Self {
            consumer,
            capability_id: capability_id.into(),
            provider_slot: Some(provider_slot.into()),
            provider_instance: None,
            admission: None,
        }
    }

    pub fn to_instance(
        consumer: PluginInstanceId,
        capability_id: impl Into<String>,
        provider: PluginInstanceId,
    ) -> Self {
        Self {
            consumer,
            capability_id: capability_id.into(),
            provider_slot: None,
            provider_instance: Some(provider),
            admission: None,
        }
    }

    #[must_use]
    pub const fn with_admission(mut self, admission: RequestAdmissionPlan) -> Self {
        self.admission = Some(admission);
        self
    }

    pub const fn consumer(&self) -> &PluginInstanceId {
        &self.consumer
    }

    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    pub fn provider_slot(&self) -> Option<&str> {
        self.provider_slot.as_deref()
    }

    pub const fn provider_instance(&self) -> Option<&PluginInstanceId> {
        self.provider_instance.as_ref()
    }

    pub const fn admission(&self) -> Option<RequestAdmissionPlan> {
        self.admission
    }
}

impl HostDefaultPlugin {
    pub fn new(plugin_id: impl Into<String>, instance_key: impl Into<String>) -> Self {
        Self {
            id: PluginInstanceId::new(plugin_id, instance_key),
            configuration: empty_configuration(),
            disableable: false,
        }
    }

    #[must_use]
    pub fn with_configuration(mut self, configuration: Value) -> Self {
        self.configuration = configuration;
        self
    }

    #[must_use]
    pub const fn disableable(mut self) -> Self {
        self.disableable = true;
        self
    }

    pub const fn id(&self) -> &PluginInstanceId {
        &self.id
    }

    pub const fn configuration(&self) -> &Value {
        &self.configuration
    }

    pub const fn is_disableable(&self) -> bool {
        self.disableable
    }
}

/// Immutable Host input used to resolve one App.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostCatalog {
    #[serde(default)]
    slots: Vec<HostSlot>,
    #[serde(default)]
    plugins: Vec<HostPluginRelease>,
    #[serde(default)]
    defaults: Vec<HostDefaultPlugin>,
    #[serde(default)]
    configurations: Vec<HostPluginConfiguration>,
    #[serde(default)]
    bindings: Vec<HostBinding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

fn default_execution_lanes() -> Vec<ExecutionLanePlan> {
    vec![ExecutionLanePlan::new("main")]
}

impl HostCatalog {
    pub fn new(
        slots: impl IntoIterator<Item = HostSlot>,
        plugins: impl IntoIterator<Item = HostPluginRelease>,
        defaults: impl IntoIterator<Item = HostDefaultPlugin>,
    ) -> Self {
        Self {
            slots: slots.into_iter().collect(),
            plugins: plugins.into_iter().collect(),
            defaults: defaults.into_iter().collect(),
            configurations: Vec::new(),
            bindings: Vec::new(),
            execution_lanes: default_execution_lanes(),
        }
    }

    #[must_use]
    pub fn with_execution_lanes(mut self, lanes: Vec<ExecutionLanePlan>) -> Self {
        self.execution_lanes = lanes;
        self
    }

    #[must_use]
    pub fn with_bindings(mut self, bindings: impl IntoIterator<Item = HostBinding>) -> Self {
        self.bindings = bindings.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_configurations(
        mut self,
        configurations: impl IntoIterator<Item = HostPluginConfiguration>,
    ) -> Self {
        self.configurations = configurations.into_iter().collect();
        self
    }

    pub fn slots(&self) -> &[HostSlot] {
        &self.slots
    }

    pub fn plugins(&self) -> &[HostPluginRelease] {
        &self.plugins
    }

    pub fn defaults(&self) -> &[HostDefaultPlugin] {
        &self.defaults
    }

    pub fn configurations(&self) -> &[HostPluginConfiguration] {
        &self.configurations
    }

    pub fn bindings(&self) -> &[HostBinding] {
        &self.bindings
    }

    pub fn execution_lanes(&self) -> &[ExecutionLanePlan] {
        &self.execution_lanes
    }
}

/// Direct configuration of one Plugin Root Instance.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRootInstance {
    id: PluginInstanceId,
    #[serde(default = "empty_configuration")]
    configuration: Value,
}

impl PluginRootInstance {
    pub fn new(plugin_id: impl Into<String>, instance_key: impl Into<String>) -> Self {
        Self {
            id: PluginInstanceId::new(plugin_id, instance_key),
            configuration: empty_configuration(),
        }
    }

    #[must_use]
    pub fn with_configuration(mut self, configuration: Value) -> Self {
        self.configuration = configuration;
        self
    }

    pub const fn id(&self) -> &PluginInstanceId {
        &self.id
    }

    pub const fn configuration(&self) -> &Value {
        &self.configuration
    }
}

/// Filesystem-independent snapshot of one `plugins/` directory.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRootSnapshot {
    #[serde(default)]
    releases: Vec<PluginDescriptor>,
    #[serde(default)]
    instances: Vec<PluginRootInstance>,
    #[serde(default)]
    disabled: Vec<PluginInstanceId>,
}

impl PluginRootSnapshot {
    pub fn new(
        releases: impl IntoIterator<Item = PluginDescriptor>,
        instances: impl IntoIterator<Item = PluginRootInstance>,
        disabled: impl IntoIterator<Item = PluginInstanceId>,
    ) -> Self {
        Self {
            releases: releases.into_iter().collect(),
            instances: instances.into_iter().collect(),
            disabled: disabled.into_iter().collect(),
        }
    }

    pub fn releases(&self) -> &[PluginDescriptor] {
        &self.releases
    }

    pub fn instances(&self) -> &[PluginRootInstance] {
        &self.instances
    }

    pub fn disabled(&self) -> &[PluginInstanceId] {
        &self.disabled
    }
}

/// Provenance of one enabled Plugin Instance in a resolved App.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginInstanceSource {
    HostDefault,
    HostDefaultConfiguredByRoot,
    PluginRoot,
}

/// One enabled Plugin Instance and its private Plan key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPluginInstance {
    id: PluginInstanceId,
    plan_key: String,
    source: PluginInstanceSource,
}

impl ResolvedPluginInstance {
    pub const fn id(&self) -> &PluginInstanceId {
        &self.id
    }

    pub fn plan_key(&self) -> &str {
        &self.plan_key
    }

    pub const fn source(&self) -> PluginInstanceSource {
        self.source
    }
}

/// Complete ready App derived from a Host Catalog and Plugin Root snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedApp {
    plan: ResolvedAppPlan,
    instances: Vec<ResolvedPluginInstance>,
}

impl ResolvedApp {
    pub const fn plan(&self) -> &ResolvedAppPlan {
        &self.plan
    }

    pub fn instances(&self) -> &[ResolvedPluginInstance] {
        &self.instances
    }
}

#[derive(Clone, Debug)]
struct CandidateInstance<'a> {
    id: PluginInstanceId,
    descriptor: &'a PluginDescriptor,
    host_configuration: Option<&'a Value>,
    root_configuration: Option<&'a Value>,
    source: PluginInstanceSource,
}

/// A Host Catalog and Plugin Root could not resolve one unambiguous App.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginRootResolutionError {
    DuplicateHostSlot(String),
    DuplicatePluginRelease(String),
    RootReleaseOverrideDenied(String),
    DuplicateInstance(PluginInstanceId),
    DuplicateDisabledMarker(PluginInstanceId),
    UnknownPlugin(PluginInstanceId),
    UnknownDisabledInstance(PluginInstanceId),
    RequiredInstanceDisabled(PluginInstanceId),
    UnknownRootSlot {
        plugin_id: String,
        slot: String,
    },
    MultipleHostDefaults {
        slot: String,
        instances: Vec<String>,
    },
    ExplicitProviderDenied {
        slot: String,
        instance: PluginInstanceId,
    },
    MissingRequiredSlot(String),
    AmbiguousSlot {
        slot: String,
        instances: Vec<String>,
    },
    MissingCapability {
        consumer: PluginInstanceId,
        capability_id: String,
        descriptor_version: String,
    },
    AmbiguousCapability {
        consumer: PluginInstanceId,
        capability_id: String,
        candidates: Vec<PluginInstanceId>,
    },
    InvalidHostBinding(String),
    InvalidHostConfiguration(String),
    InvalidConfiguration {
        instance: PluginInstanceId,
        detail: String,
    },
    InvalidResolvedApp(String),
}

impl fmt::Display for PluginRootResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateHostSlot(slot) => write!(formatter, "duplicate Host Slot `{slot}`"),
            Self::DuplicatePluginRelease(plugin) => {
                write!(formatter, "duplicate Plugin Release `{plugin}`")
            }
            Self::RootReleaseOverrideDenied(plugin) => write!(
                formatter,
                "Plugin Root cannot replace Host Release `{plugin}`"
            ),
            Self::DuplicateInstance(instance) => {
                write!(formatter, "duplicate Plugin Instance `{instance}`")
            }
            Self::DuplicateDisabledMarker(instance) => {
                write!(formatter, "duplicate disabled marker for `{instance}`")
            }
            Self::UnknownPlugin(instance) => {
                write!(
                    formatter,
                    "Plugin Instance `{instance}` has no exact Release"
                )
            }
            Self::UnknownDisabledInstance(instance) => {
                write!(
                    formatter,
                    "disabled marker refers to unknown Instance `{instance}`"
                )
            }
            Self::RequiredInstanceDisabled(instance) => {
                write!(
                    formatter,
                    "required Host Instance `{instance}` cannot be disabled"
                )
            }
            Self::UnknownRootSlot { plugin_id, slot } => write!(
                formatter,
                "Plugin `{plugin_id}` offers unknown Host Slot `{slot}`"
            ),
            Self::MultipleHostDefaults { slot, instances } => write!(
                formatter,
                "Host Slot `{slot}` has multiple defaults: {}",
                instances.join(", ")
            ),
            Self::ExplicitProviderDenied { slot, instance } => write!(
                formatter,
                "Host Slot `{slot}` does not allow `{instance}` to replace its default"
            ),
            Self::MissingRequiredSlot(slot) => {
                write!(
                    formatter,
                    "required Host Slot `{slot}` has no enabled Plugin"
                )
            }
            Self::AmbiguousSlot { slot, instances } => write!(
                formatter,
                "Host Slot `{slot}` has multiple explicit Plugins: {}",
                instances.join(", ")
            ),
            Self::MissingCapability {
                consumer,
                capability_id,
                descriptor_version,
            } => write!(
                formatter,
                "Plugin Instance `{consumer}` has no provider for Capability `{capability_id}` version `{descriptor_version}`"
            ),
            Self::AmbiguousCapability {
                consumer,
                capability_id,
                candidates,
            } => write!(
                formatter,
                "Plugin Instance `{consumer}` has multiple providers for Capability `{capability_id}`: {}",
                candidates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::InvalidHostBinding(detail) => write!(formatter, "invalid Host binding: {detail}"),
            Self::InvalidHostConfiguration(detail) => {
                write!(formatter, "invalid Host Plugin configuration: {detail}")
            }
            Self::InvalidConfiguration { instance, detail } => {
                write!(
                    formatter,
                    "Plugin Instance `{instance}` has invalid configuration: {detail}"
                )
            }
            Self::InvalidResolvedApp(detail) => {
                write!(formatter, "derived App is invalid: {detail}")
            }
        }
    }
}

impl Error for PluginRootResolutionError {}

/// Resolves one Host and one Plugin Root into a complete immutable App Plan.
pub fn resolve_plugin_root(
    host: &HostCatalog,
    root: &PluginRootSnapshot,
) -> Result<ResolvedApp, PluginRootResolutionError> {
    let slots = index_slots(&host.slots)?;
    let releases = index_releases(&host.plugins, &root.releases)?;
    validate_release_slots(&releases, &slots)?;
    let defaults = index_defaults(&host.defaults)?;
    let configurations = index_configurations(&host.configurations, &defaults, &releases)?;
    let explicit = index_root_instances(&root.instances)?;
    let disabled = index_disabled(&root.disabled)?;
    validate_disabled(&defaults, &explicit, &disabled)?;
    let candidates = build_candidates(&defaults, &configurations, &explicit, &disabled, &releases)?;
    let selected = select_slot_candidates(&slots, candidates)?;
    materialize_app(selected, &host.bindings, &host.execution_lanes)
}

fn index_slots(slots: &[HostSlot]) -> Result<BTreeMap<&str, &HostSlot>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for slot in slots {
        if indexed.insert(slot.id.as_str(), slot).is_some() {
            return Err(PluginRootResolutionError::DuplicateHostSlot(
                slot.id.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn index_releases<'a>(
    host: &'a [HostPluginRelease],
    root: &'a [PluginDescriptor],
) -> Result<BTreeMap<&'a str, &'a PluginDescriptor>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    let mut overrides = BTreeMap::new();
    for release in host {
        let id = release.descriptor.plugin_id();
        if indexed.insert(id, &release.descriptor).is_some() {
            return Err(PluginRootResolutionError::DuplicatePluginRelease(
                id.to_owned(),
            ));
        }
        overrides.insert(id, release.allow_root_override);
    }
    let mut root_ids = BTreeSet::new();
    for descriptor in root {
        let id = descriptor.plugin_id();
        if !root_ids.insert(id) {
            return Err(PluginRootResolutionError::DuplicatePluginRelease(
                id.to_owned(),
            ));
        }
        if indexed.contains_key(id) && !overrides.get(id).copied().unwrap_or(false) {
            return Err(PluginRootResolutionError::RootReleaseOverrideDenied(
                id.to_owned(),
            ));
        }
        indexed.insert(id, descriptor);
    }
    Ok(indexed)
}

fn validate_release_slots(
    releases: &BTreeMap<&str, &PluginDescriptor>,
    slots: &BTreeMap<&str, &HostSlot>,
) -> Result<(), PluginRootResolutionError> {
    for descriptor in releases.values() {
        if !slots.contains_key(descriptor.root_slot()) {
            return Err(PluginRootResolutionError::UnknownRootSlot {
                plugin_id: descriptor.plugin_id().to_owned(),
                slot: descriptor.root_slot().to_owned(),
            });
        }
    }
    Ok(())
}

fn index_defaults(
    defaults: &[HostDefaultPlugin],
) -> Result<BTreeMap<&PluginInstanceId, &HostDefaultPlugin>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for default in defaults {
        if indexed.insert(&default.id, default).is_some() {
            return Err(PluginRootResolutionError::DuplicateInstance(
                default.id.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn index_configurations<'a>(
    configurations: &'a [HostPluginConfiguration],
    defaults: &BTreeMap<&PluginInstanceId, &HostDefaultPlugin>,
    releases: &BTreeMap<&str, &PluginDescriptor>,
) -> Result<BTreeMap<&'a PluginInstanceId, &'a Value>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for configuration in configurations {
        if !releases.contains_key(configuration.id.plugin_id()) {
            return Err(PluginRootResolutionError::InvalidHostConfiguration(
                format!("`{}` has no exact Plugin Release", configuration.id),
            ));
        }
        if defaults.contains_key(&configuration.id) {
            return Err(PluginRootResolutionError::InvalidHostConfiguration(
                format!(
                    "`{}` duplicates configuration owned by a Host default",
                    configuration.id
                ),
            ));
        }
        if indexed
            .insert(&configuration.id, &configuration.configuration)
            .is_some()
        {
            return Err(PluginRootResolutionError::InvalidHostConfiguration(
                format!("duplicate configuration for `{}`", configuration.id),
            ));
        }
    }
    Ok(indexed)
}

fn index_root_instances(
    instances: &[PluginRootInstance],
) -> Result<BTreeMap<&PluginInstanceId, &PluginRootInstance>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for instance in instances {
        if indexed.insert(&instance.id, instance).is_some() {
            return Err(PluginRootResolutionError::DuplicateInstance(
                instance.id.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn index_disabled(
    disabled: &[PluginInstanceId],
) -> Result<BTreeSet<&PluginInstanceId>, PluginRootResolutionError> {
    let mut indexed = BTreeSet::new();
    for instance in disabled {
        if !indexed.insert(instance) {
            return Err(PluginRootResolutionError::DuplicateDisabledMarker(
                instance.clone(),
            ));
        }
    }
    Ok(indexed)
}

fn validate_disabled(
    defaults: &BTreeMap<&PluginInstanceId, &HostDefaultPlugin>,
    explicit: &BTreeMap<&PluginInstanceId, &PluginRootInstance>,
    disabled: &BTreeSet<&PluginInstanceId>,
) -> Result<(), PluginRootResolutionError> {
    for id in disabled {
        if let Some(default) = defaults.get(id) {
            if !default.disableable {
                return Err(PluginRootResolutionError::RequiredInstanceDisabled(
                    (*id).clone(),
                ));
            }
        } else if !explicit.contains_key(id) {
            return Err(PluginRootResolutionError::UnknownDisabledInstance(
                (*id).clone(),
            ));
        }
    }
    Ok(())
}

fn build_candidates<'a>(
    defaults: &BTreeMap<&'a PluginInstanceId, &'a HostDefaultPlugin>,
    configurations: &BTreeMap<&'a PluginInstanceId, &'a Value>,
    explicit: &BTreeMap<&'a PluginInstanceId, &'a PluginRootInstance>,
    disabled: &BTreeSet<&PluginInstanceId>,
    releases: &BTreeMap<&'a str, &'a PluginDescriptor>,
) -> Result<Vec<CandidateInstance<'a>>, PluginRootResolutionError> {
    let ids = defaults
        .keys()
        .chain(explicit.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    for id in ids {
        if disabled.contains(id) {
            continue;
        }
        let descriptor = releases
            .get(id.plugin_id())
            .copied()
            .ok_or_else(|| PluginRootResolutionError::UnknownPlugin(id.clone()))?;
        let host_default = defaults.get(id).copied();
        let root_instance = explicit.get(id).copied();
        let source = match (host_default, root_instance) {
            (Some(_), Some(_)) => PluginInstanceSource::HostDefaultConfiguredByRoot,
            (Some(_), None) => PluginInstanceSource::HostDefault,
            (None, Some(_)) => PluginInstanceSource::PluginRoot,
            (None, None) => unreachable!("candidate IDs come from default or explicit input"),
        };
        candidates.push(CandidateInstance {
            id: id.clone(),
            descriptor,
            host_configuration: host_default
                .map(|default| &default.configuration)
                .or_else(|| configurations.get(id).copied()),
            root_configuration: root_instance.map(|instance| &instance.configuration),
            source,
        });
    }
    Ok(candidates)
}

fn select_slot_candidates<'a>(
    slots: &BTreeMap<&str, &'a HostSlot>,
    candidates: Vec<CandidateInstance<'a>>,
) -> Result<Vec<(CandidateInstance<'a>, &'a HostSlot)>, PluginRootResolutionError> {
    let mut by_slot = BTreeMap::<&str, Vec<CandidateInstance<'a>>>::new();
    for candidate in candidates {
        by_slot
            .entry(candidate.descriptor.root_slot())
            .or_default()
            .push(candidate);
    }
    let mut selected = Vec::new();
    for (slot_id, slot) in slots {
        let mut candidates = by_slot.remove(slot_id).unwrap_or_default();
        candidates.sort_by(|left, right| left.id.cmp(&right.id));
        if slot.cardinality == HostSlotCardinality::Many {
            selected.extend(candidates.into_iter().map(|candidate| (candidate, *slot)));
            continue;
        }
        let (defaults, explicit): (Vec<_>, Vec<_>) = candidates
            .into_iter()
            .partition(|candidate| candidate.source != PluginInstanceSource::PluginRoot);
        if defaults.len() > 1 {
            return Err(PluginRootResolutionError::MultipleHostDefaults {
                slot: (*slot_id).to_owned(),
                instances: defaults
                    .iter()
                    .map(|candidate| candidate.id.to_string())
                    .collect(),
            });
        }
        if explicit.len() > 1 {
            return Err(PluginRootResolutionError::AmbiguousSlot {
                slot: (*slot_id).to_owned(),
                instances: explicit
                    .iter()
                    .map(|candidate| candidate.id.to_string())
                    .collect(),
            });
        }
        if let Some(candidate) = explicit.into_iter().next() {
            if !defaults.is_empty() && !slot.replaceable {
                return Err(PluginRootResolutionError::ExplicitProviderDenied {
                    slot: (*slot_id).to_owned(),
                    instance: candidate.id,
                });
            }
            selected.push((candidate, *slot));
        } else if let Some(candidate) = defaults.into_iter().next() {
            selected.push((candidate, *slot));
        } else if slot.cardinality == HostSlotCardinality::One {
            return Err(PluginRootResolutionError::MissingRequiredSlot(
                (*slot_id).to_owned(),
            ));
        }
    }
    Ok(selected)
}

fn materialize_app(
    selected: Vec<(CandidateInstance<'_>, &HostSlot)>,
    host_bindings: &[HostBinding],
    lanes: &[ExecutionLanePlan],
) -> Result<ResolvedApp, PluginRootResolutionError> {
    let mut plan_instances = Vec::with_capacity(selected.len());
    let mut resolved_instances = Vec::with_capacity(selected.len());
    let mut plan_slots = BTreeMap::new();
    for (candidate, slot) in selected {
        let plan_key = candidate.id.plan_key();
        plan_slots.insert(plan_key.clone(), slot.id().to_owned());
        let overlays = candidate
            .host_configuration
            .into_iter()
            .chain(candidate.root_configuration)
            .collect::<Vec<_>>();
        let configuration = resolve_configuration_layers(
            candidate.descriptor.configuration_defaults(),
            &overlays,
            candidate.descriptor.configuration_schema(),
            &plan_key,
        )
        .map_err(|error| map_configuration_error(&candidate.id, error))?;
        let configuration = serde_json::to_string(&configuration).map_err(|error| {
            PluginRootResolutionError::InvalidConfiguration {
                instance: candidate.id.clone(),
                detail: error.to_string(),
            }
        })?;
        let descriptor = candidate.descriptor;
        let mut instance = PluginInstancePlan::new(&plan_key, descriptor.runtime_package_id())
            .with_entrypoint(descriptor.entrypoint())
            .with_package_revision(descriptor.runtime_package_revision())
            .with_configuration(configuration)
            .with_execution_class(descriptor.execution_class().clone())
            .with_restart_policy(descriptor.restart_policy())
            .with_criticality(descriptor.criticality())
            .with_execution_lane(ExecutionLaneId::new(&slot.execution_lane));
        for capability in descriptor.provided_capabilities() {
            instance = instance.with_capability(capability.clone());
        }
        for requirement in descriptor.required_capabilities() {
            instance = instance.with_requirement(requirement.clone());
        }
        plan_instances.push(instance);
        resolved_instances.push(ResolvedPluginInstance {
            id: candidate.id,
            plan_key,
            source: candidate.source,
        });
    }
    let bindings = derive_root_bindings(
        &plan_instances,
        &resolved_instances,
        &plan_slots,
        host_bindings,
    )?;
    let composition =
        AppComposition::new(plan_instances, bindings).with_execution_lanes(lanes.to_vec());
    let plan = composition
        .resolve()
        .map_err(|error| PluginRootResolutionError::InvalidResolvedApp(error.to_string()))?;
    resolved_instances.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ResolvedApp {
        plan,
        instances: resolved_instances,
    })
}

fn derive_root_bindings(
    instances: &[PluginInstancePlan],
    resolved: &[ResolvedPluginInstance],
    plan_slots: &BTreeMap<String, String>,
    host_bindings: &[HostBinding],
) -> Result<Vec<CapabilityBinding>, PluginRootResolutionError> {
    let ids_by_plan_key = resolved
        .iter()
        .map(|instance| (instance.plan_key.as_str(), &instance.id))
        .collect::<BTreeMap<_, _>>();
    let plan_keys_by_id = resolved
        .iter()
        .map(|instance| (&instance.id, instance.plan_key.as_str()))
        .collect::<BTreeMap<_, _>>();
    let indexed = index_host_bindings(host_bindings)?;

    let mut consumed = BTreeSet::new();
    let mut bindings = Vec::new();
    for consumer in instances {
        let Some(consumer_id) = ids_by_plan_key.get(consumer.instance_key()) else {
            return Err(PluginRootResolutionError::InvalidResolvedApp(format!(
                "missing Plugin identity for `{}`",
                consumer.instance_key()
            )));
        };
        for requirement in consumer.required_capabilities() {
            let key = (*consumer_id, requirement.capability_id());
            let host_binding = indexed.get(&key).copied();
            if host_binding.is_some() {
                consumed.insert(key);
            }
            let mut candidates = instances
                .iter()
                .filter(|provider| {
                    provider.provided_capabilities().iter().any(|endpoint| {
                        endpoint.capability_id() == requirement.capability_id()
                            && endpoint.descriptor_version() == requirement.descriptor_version()
                    }) && host_binding.is_none_or(|binding| {
                        binding.provider_slot.as_ref().is_none_or(|slot| {
                            plan_slots.get(provider.instance_key()) == Some(slot)
                        }) && binding.provider_instance.as_ref().is_none_or(|id| {
                            plan_keys_by_id.get(id).copied() == Some(provider.instance_key())
                        })
                    })
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|candidate| candidate.instance_key());
            let selected = match requirement.cardinality() {
                CapabilityCardinality::Many => candidates,
                CapabilityCardinality::One if candidates.len() == 1 => candidates,
                CapabilityCardinality::Optional if candidates.len() <= 1 => candidates,
                CapabilityCardinality::One if candidates.is_empty() => {
                    return Err(PluginRootResolutionError::MissingCapability {
                        consumer: (*consumer_id).clone(),
                        capability_id: requirement.capability_id().to_owned(),
                        descriptor_version: requirement.descriptor_version().to_owned(),
                    });
                }
                CapabilityCardinality::Optional | CapabilityCardinality::One => {
                    return Err(PluginRootResolutionError::AmbiguousCapability {
                        consumer: (*consumer_id).clone(),
                        capability_id: requirement.capability_id().to_owned(),
                        candidates: candidates
                            .iter()
                            .filter_map(|candidate| {
                                ids_by_plan_key
                                    .get(candidate.instance_key())
                                    .copied()
                                    .cloned()
                            })
                            .collect(),
                    });
                }
            };
            for provider in selected {
                let mut binding = CapabilityBinding::new(
                    consumer.instance_key(),
                    requirement.capability_id(),
                    requirement.descriptor_version(),
                    provider.instance_key(),
                );
                if let Some(admission) = host_binding.and_then(HostBinding::admission) {
                    binding = binding.with_admission(admission);
                }
                bindings.push(binding);
            }
        }
    }
    validate_host_bindings_consumed(host_bindings, &plan_keys_by_id, &consumed)?;
    Ok(bindings)
}

type IndexedHostBindings<'a> = BTreeMap<(&'a PluginInstanceId, &'a str), &'a HostBinding>;

fn index_host_bindings(
    host_bindings: &[HostBinding],
) -> Result<IndexedHostBindings<'_>, PluginRootResolutionError> {
    let mut indexed = BTreeMap::new();
    for binding in host_bindings {
        let key = (&binding.consumer, binding.capability_id.as_str());
        if indexed.insert(key, binding).is_some() {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "duplicate attachment for `{}` Capability `{}`",
                binding.consumer, binding.capability_id
            )));
        }
        if binding.provider_slot.is_some() == binding.provider_instance.is_some() {
            return Err(PluginRootResolutionError::InvalidHostBinding(format!(
                "attachment for `{}` must select exactly one provider Slot or Instance",
                binding.consumer
            )));
        }
    }
    Ok(indexed)
}

fn validate_host_bindings_consumed(
    host_bindings: &[HostBinding],
    plan_keys_by_id: &BTreeMap<&PluginInstanceId, &str>,
    consumed: &BTreeSet<(&PluginInstanceId, &str)>,
) -> Result<(), PluginRootResolutionError> {
    let Some(binding) = host_bindings.iter().find(|binding| {
        plan_keys_by_id.contains_key(&binding.consumer)
            && !consumed.contains(&(&binding.consumer, binding.capability_id.as_str()))
    }) else {
        return Ok(());
    };
    let consumer_plan_key = plan_keys_by_id
        .get(&binding.consumer)
        .copied()
        .unwrap_or("<missing>");
    Err(PluginRootResolutionError::InvalidHostBinding(format!(
        "attachment for `{consumer_plan_key}` Capability `{}` matches no requirement",
        binding.capability_id
    )))
}

fn map_configuration_error(
    instance: &PluginInstanceId,
    error: ConfigurationError,
) -> PluginRootResolutionError {
    PluginRootResolutionError::InvalidConfiguration {
        instance: instance.clone(),
        detail: error.detail,
    }
}
