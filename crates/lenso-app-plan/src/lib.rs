//! App Composition and immutable execution input for the Lenso vNext Kernel.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// The Resolved App Plan schema understood by this Kernel version.
pub const PLAN_SCHEMA_VERSION: u32 = 1;

/// The cardinality of one Module's Capability requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityCardinality {
    /// Exactly one provider must be bound.
    One,
    /// Zero or one provider may be bound.
    Optional,
    /// Zero or more providers may be bound in deterministic order.
    Many,
}

/// One Capability required by a Module Instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRequirementPlan {
    capability_id: String,
    descriptor_version: String,
    cardinality: CapabilityCardinality,
}

impl CapabilityRequirementPlan {
    /// Declares one exact Capability Descriptor and its binding cardinality.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        cardinality: CapabilityCardinality,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            cardinality,
        }
    }

    /// Declares a required one-provider Capability.
    pub fn one(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(
            capability_id,
            descriptor_version,
            CapabilityCardinality::One,
        )
    }

    /// Declares an optional zero-or-one-provider Capability.
    pub fn optional(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
    ) -> Self {
        Self::new(
            capability_id,
            descriptor_version,
            CapabilityCardinality::Optional,
        )
    }

    /// Declares a many-provider Capability.
    pub fn many(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(
            capability_id,
            descriptor_version,
            CapabilityCardinality::Many,
        )
    }

    /// Returns the Capability series identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }

    /// Returns the exact Descriptor version selected by Composition.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }

    /// Returns the requirement cardinality.
    pub const fn cardinality(&self) -> CapabilityCardinality {
        self.cardinality
    }
}

/// Exact Capability endpoint metadata expected from one Module Instance.
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

/// One exact App-local Module Instance selected by the resolved Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleInstancePlan {
    instance_key: String,
    package_id: String,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
    required_capabilities: Vec<CapabilityRequirementPlan>,
}

impl ModuleInstancePlan {
    /// Selects one statically linked package under an App-local Instance key.
    pub fn new(instance_key: impl Into<String>, package_id: impl Into<String>) -> Self {
        Self {
            instance_key: instance_key.into(),
            package_id: package_id.into(),
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
        }
    }

    /// Declares one exact endpoint this Instance must prepare.
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpointPlan) -> Self {
        self.provided_capabilities.push(capability);
        self
    }

    /// Declares one Capability dependency for this Instance.
    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirementPlan) -> Self {
        self.required_capabilities.push(requirement);
        self
    }

    /// Alias that makes the authoring direction explicit at the call site.
    #[must_use]
    pub fn with_required_capability(self, requirement: CapabilityRequirementPlan) -> Self {
        self.with_requirement(requirement)
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

    /// Returns the exact Capability requirements this Instance receives.
    pub fn required_capabilities(&self) -> &[CapabilityRequirementPlan] {
        &self.required_capabilities
    }
}

/// One exact consumer-to-provider Capability binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityBinding {
    consumer_instance: String,
    capability_id: String,
    descriptor_version: String,
    provider_instance: String,
    provider_order: usize,
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
            provider_order: 0,
        }
    }

    fn with_provider_order(mut self, provider_order: usize) -> Self {
        self.provider_order = provider_order;
        self
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

    /// Returns the deterministic zero-based order within a `many` requirement.
    pub const fn provider_order(&self) -> usize {
        self.provider_order
    }
}

/// Declarative, language-independent authoring input for one App.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppComposition {
    module_instances: Vec<ModuleInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
}

impl AppComposition {
    /// Creates an App Composition with explicit Module Instances and bindings.
    pub fn new(
        module_instances: Vec<ModuleInstancePlan>,
        capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        Self {
            module_instances,
            capability_bindings,
        }
    }

    /// Materializes one deterministic, validated Resolved App Plan.
    pub fn resolve(&self) -> Result<ResolvedAppPlan, PlanResolutionError> {
        resolve_parts(&self.module_instances, &self.capability_bindings).map(
            |(module_instances, capability_bindings)| ResolvedAppPlan {
                schema_version: PLAN_SCHEMA_VERSION,
                module_instances,
                capability_bindings,
            },
        )
    }

    /// Returns the authoring Module Instances.
    pub fn module_instances(&self) -> &[ModuleInstancePlan] {
        &self.module_instances
    }

    /// Returns the authoring bindings.
    pub fn capability_bindings(&self) -> &[CapabilityBinding] {
        &self.capability_bindings
    }
}

/// A reason App Composition could not be materialized into a Plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanResolutionError {
    /// The Plan schema cannot be executed by this Kernel version.
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    /// Two Module Instances use the same App-local key.
    DuplicateModuleInstance { instance_key: String },
    /// A Module declares the same provided Capability more than once.
    DuplicateProvidedCapability {
        provider_instance: String,
        capability_id: String,
    },
    /// A Module declares the same required Capability more than once.
    DuplicateRequiredCapability {
        consumer_instance: String,
        capability_id: String,
    },
    /// A binding names a consumer that is not in the Composition.
    InvalidConsumerReference {
        consumer_instance: String,
        capability_id: String,
    },
    /// A binding names a provider that is not in the Composition or does not provide the Capability.
    InvalidProviderReference {
        consumer_instance: String,
        capability_id: String,
        provider_instance: String,
    },
    /// A binding exists without a matching consumer requirement.
    UndeclaredCapabilityRequirement {
        consumer_instance: String,
        capability_id: String,
    },
    /// The provider and consumer selected different exact Descriptor versions.
    IncompatibleCapabilityVersion {
        consumer_instance: String,
        capability_id: String,
        required: String,
        provided: String,
        provider_instance: String,
    },
    /// A `one` requirement has no explicit provider.
    MissingOneBinding {
        consumer_instance: String,
        capability_id: String,
    },
    /// A `one` requirement has more than one explicit provider.
    AmbiguousOneBinding {
        consumer_instance: String,
        capability_id: String,
        providers: usize,
    },
    /// An `optional` requirement has more than one explicit provider.
    AmbiguousOptionalBinding {
        consumer_instance: String,
        capability_id: String,
        providers: usize,
    },
    /// A provider is repeated for the same requirement.
    DuplicateBinding {
        consumer_instance: String,
        capability_id: String,
        provider_instance: String,
    },
    /// Required one-provider activation dependencies contain a cycle.
    ActivationCycle { instances: Vec<String> },
}

impl fmt::Display for PlanResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "unsupported Plan schema version {actual}; expected {expected}"
            ),
            Self::DuplicateModuleInstance { instance_key } => {
                write!(formatter, "duplicate Module Instance `{instance_key}`")
            }
            Self::DuplicateProvidedCapability {
                provider_instance,
                capability_id,
            } => write!(
                formatter,
                "Module Instance `{provider_instance}` provides Capability `{capability_id}` more than once"
            ),
            Self::DuplicateRequiredCapability {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "Module Instance `{consumer_instance}` requires Capability `{capability_id}` more than once"
            ),
            Self::InvalidConsumerReference {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "Capability `{capability_id}` names missing consumer `{consumer_instance}`"
            ),
            Self::InvalidProviderReference {
                consumer_instance,
                capability_id,
                provider_instance,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` names invalid provider `{provider_instance}` for Capability `{capability_id}`"
            ),
            Self::UndeclaredCapabilityRequirement {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` has no declared requirement for Capability `{capability_id}`"
            ),
            Self::IncompatibleCapabilityVersion {
                consumer_instance,
                capability_id,
                required,
                provided,
                provider_instance,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` requires Capability `{capability_id}` version `{required}`, but provider `{provider_instance}` provides `{provided}`"
            ),
            Self::MissingOneBinding {
                consumer_instance,
                capability_id,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` is missing one binding for Capability `{capability_id}`"
            ),
            Self::AmbiguousOneBinding {
                consumer_instance,
                capability_id,
                providers,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` has {providers} bindings for one Capability `{capability_id}`"
            ),
            Self::AmbiguousOptionalBinding {
                consumer_instance,
                capability_id,
                providers,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` has {providers} bindings for optional Capability `{capability_id}`"
            ),
            Self::DuplicateBinding {
                consumer_instance,
                capability_id,
                provider_instance,
            } => write!(
                formatter,
                "consumer `{consumer_instance}` binds Capability `{capability_id}` to provider `{provider_instance}` more than once"
            ),
            Self::ActivationCycle { instances } => write!(
                formatter,
                "required Capability activation cycle: {}",
                instances.join(" -> ")
            ),
        }
    }
}

impl std::error::Error for PlanResolutionError {}

/// Exact, immutable execution input supplied to the Kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAppPlan {
    schema_version: u32,
    module_instances: Vec<ModuleInstancePlan>,
    capability_bindings: Vec<CapabilityBinding>,
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

    /// Creates a Plan with exact entries, retaining invalid entries for later validation.
    pub fn new(
        mut module_instances: Vec<ModuleInstancePlan>,
        mut capability_bindings: Vec<CapabilityBinding>,
    ) -> Self {
        sort_module_instances(&mut module_instances);
        sort_bindings(&mut capability_bindings);
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

    /// Validates the immutable Plan graph before a Runtime Driver or Adapter boots it.
    pub fn validate(&self) -> Result<(), PlanResolutionError> {
        if self.schema_version != PLAN_SCHEMA_VERSION {
            return Err(PlanResolutionError::UnsupportedSchemaVersion {
                expected: PLAN_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }
        resolve_parts(&self.module_instances, &self.capability_bindings).map(|_| ())
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

fn resolve_parts(
    module_instances: &[ModuleInstancePlan],
    capability_bindings: &[CapabilityBinding],
) -> Result<(Vec<ModuleInstancePlan>, Vec<CapabilityBinding>), PlanResolutionError> {
    let (instances, instance_indices) = normalize_instances(module_instances)?;
    let grouped_bindings = group_bindings(&instances, &instance_indices, capability_bindings)?;
    validate_requirement_cardinality(&instances, &grouped_bindings)?;
    validate_required_cycles(&instances, &grouped_bindings)?;
    Ok((instances, order_bindings(grouped_bindings)))
}

fn normalize_instances(
    module_instances: &[ModuleInstancePlan],
) -> Result<(Vec<ModuleInstancePlan>, BTreeMap<String, usize>), PlanResolutionError> {
    let mut instances = module_instances.to_vec();
    sort_module_instances(&mut instances);

    let mut instance_indices = BTreeMap::new();
    for (index, instance) in instances.iter().enumerate() {
        if instance_indices
            .insert(instance.instance_key.clone(), index)
            .is_some()
        {
            return Err(PlanResolutionError::DuplicateModuleInstance {
                instance_key: instance.instance_key.clone(),
            });
        }
        validate_instance_declarations(instance)?;
    }
    Ok((instances, instance_indices))
}

fn group_bindings(
    instances: &[ModuleInstancePlan],
    instance_indices: &BTreeMap<String, usize>,
    capability_bindings: &[CapabilityBinding],
) -> Result<BTreeMap<(String, String), Vec<CapabilityBinding>>, PlanResolutionError> {
    let mut grouped_bindings = BTreeMap::new();
    for binding in capability_bindings {
        validate_binding(instances, instance_indices, binding)?;
        grouped_bindings
            .entry((
                binding.consumer_instance.clone(),
                binding.capability_id.clone(),
            ))
            .or_insert_with(Vec::new)
            .push(binding.clone());
    }
    Ok(grouped_bindings)
}

fn validate_binding(
    instances: &[ModuleInstancePlan],
    instance_indices: &BTreeMap<String, usize>,
    binding: &CapabilityBinding,
) -> Result<(), PlanResolutionError> {
    let Some(&consumer_index) = instance_indices.get(&binding.consumer_instance) else {
        return Err(PlanResolutionError::InvalidConsumerReference {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
        });
    };
    let consumer = &instances[consumer_index];
    let Some(requirement) = consumer
        .required_capabilities
        .iter()
        .find(|requirement| requirement.capability_id == binding.capability_id)
    else {
        return Err(PlanResolutionError::UndeclaredCapabilityRequirement {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
        });
    };

    let Some(&provider_index) = instance_indices.get(&binding.provider_instance) else {
        return Err(PlanResolutionError::InvalidProviderReference {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    };
    let provider = &instances[provider_index];
    let Some(endpoint) = provider
        .provided_capabilities
        .iter()
        .find(|endpoint| endpoint.capability_id == binding.capability_id)
    else {
        return Err(PlanResolutionError::InvalidProviderReference {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    };

    if endpoint.descriptor_version != requirement.descriptor_version {
        return Err(PlanResolutionError::IncompatibleCapabilityVersion {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            required: requirement.descriptor_version.clone(),
            provided: endpoint.descriptor_version.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    }
    if binding.descriptor_version != requirement.descriptor_version {
        return Err(PlanResolutionError::IncompatibleCapabilityVersion {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            required: requirement.descriptor_version.clone(),
            provided: binding.descriptor_version.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    }
    Ok(())
}

fn validate_requirement_cardinality(
    instances: &[ModuleInstancePlan],
    grouped_bindings: &BTreeMap<(String, String), Vec<CapabilityBinding>>,
) -> Result<(), PlanResolutionError> {
    for instance in instances {
        for requirement in &instance.required_capabilities {
            let key = (
                instance.instance_key.clone(),
                requirement.capability_id.clone(),
            );
            let bindings = grouped_bindings.get(&key).map_or(&[][..], Vec::as_slice);
            match (requirement.cardinality, bindings.len()) {
                (CapabilityCardinality::One, 0) => {
                    return Err(PlanResolutionError::MissingOneBinding {
                        consumer_instance: instance.instance_key.clone(),
                        capability_id: requirement.capability_id.clone(),
                    });
                }
                (CapabilityCardinality::One, providers) if providers > 1 => {
                    return Err(PlanResolutionError::AmbiguousOneBinding {
                        consumer_instance: instance.instance_key.clone(),
                        capability_id: requirement.capability_id.clone(),
                        providers,
                    });
                }
                (CapabilityCardinality::Optional, providers) if providers > 1 => {
                    return Err(PlanResolutionError::AmbiguousOptionalBinding {
                        consumer_instance: instance.instance_key.clone(),
                        capability_id: requirement.capability_id.clone(),
                        providers,
                    });
                }
                _ => {}
            }

            let mut provider_keys = BTreeSet::new();
            for binding in bindings {
                if !provider_keys.insert(binding.provider_instance.as_str()) {
                    return Err(PlanResolutionError::DuplicateBinding {
                        consumer_instance: binding.consumer_instance.clone(),
                        capability_id: binding.capability_id.clone(),
                        provider_instance: binding.provider_instance.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn order_bindings(
    grouped_bindings: BTreeMap<(String, String), Vec<CapabilityBinding>>,
) -> Vec<CapabilityBinding> {
    let mut ordered_bindings = Vec::new();
    for (_, mut bindings) in grouped_bindings {
        bindings.sort_by(|left, right| {
            left.provider_instance
                .cmp(&right.provider_instance)
                .then_with(|| left.descriptor_version.cmp(&right.descriptor_version))
        });
        for (provider_order, binding) in bindings.into_iter().enumerate() {
            ordered_bindings.push(binding.with_provider_order(provider_order));
        }
    }
    ordered_bindings
}

fn validate_required_cycles(
    instances: &[ModuleInstancePlan],
    grouped_bindings: &BTreeMap<(String, String), Vec<CapabilityBinding>>,
) -> Result<(), PlanResolutionError> {
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = instances
        .iter()
        .map(|instance| (instance.instance_key.clone(), BTreeSet::new()))
        .collect();

    for instance in instances {
        for requirement in &instance.required_capabilities {
            if requirement.cardinality != CapabilityCardinality::One {
                continue;
            }
            let key = (
                instance.instance_key.clone(),
                requirement.capability_id.clone(),
            );
            if let Some(binding) = grouped_bindings
                .get(&key)
                .and_then(|bindings| bindings.first())
            {
                dependencies
                    .entry(instance.instance_key.clone())
                    .or_default()
                    .insert(binding.provider_instance.clone());
            }
        }
    }

    let mut indegrees: BTreeMap<String, usize> = dependencies
        .keys()
        .cloned()
        .map(|instance| (instance, 0))
        .collect();
    for providers in dependencies.values() {
        for provider in providers {
            *indegrees
                .get_mut(provider)
                .expect("provider Instance was indexed before dependency validation") += 1;
        }
    }

    let mut ready: BTreeSet<String> = indegrees
        .iter()
        .filter(|(_, indegree)| **indegree == 0)
        .map(|(instance, _)| instance.clone())
        .collect();
    let mut processed = 0;
    while let Some(instance) = ready.pop_first() {
        processed += 1;
        if let Some(providers) = dependencies.get(&instance) {
            for provider in providers {
                let indegree = indegrees
                    .get_mut(provider)
                    .expect("provider Instance was indexed before dependency validation");
                *indegree -= 1;
                if *indegree == 0 {
                    ready.insert(provider.clone());
                }
            }
        }
    }

    if processed != instances.len() {
        return Err(PlanResolutionError::ActivationCycle {
            instances: indegrees
                .into_iter()
                .filter(|(_, indegree)| *indegree > 0)
                .map(|(instance, _)| instance)
                .collect(),
        });
    }
    Ok(())
}

fn validate_instance_declarations(
    instance: &ModuleInstancePlan,
) -> Result<(), PlanResolutionError> {
    let mut provided = BTreeSet::new();
    for endpoint in &instance.provided_capabilities {
        if !provided.insert(endpoint.capability_id.as_str()) {
            return Err(PlanResolutionError::DuplicateProvidedCapability {
                provider_instance: instance.instance_key.clone(),
                capability_id: endpoint.capability_id.clone(),
            });
        }
    }

    let mut required = BTreeSet::new();
    for requirement in &instance.required_capabilities {
        if !required.insert(requirement.capability_id.as_str()) {
            return Err(PlanResolutionError::DuplicateRequiredCapability {
                consumer_instance: instance.instance_key.clone(),
                capability_id: requirement.capability_id.clone(),
            });
        }
    }
    Ok(())
}

fn sort_module_instances(instances: &mut [ModuleInstancePlan]) {
    instances.sort_by(|left, right| left.instance_key.cmp(&right.instance_key));
}

fn sort_bindings(bindings: &mut [CapabilityBinding]) {
    bindings.sort_by(|left, right| {
        left.consumer_instance
            .cmp(&right.consumer_instance)
            .then_with(|| left.capability_id.cmp(&right.capability_id))
            .then_with(|| left.provider_instance.cmp(&right.provider_instance))
            .then_with(|| left.provider_order.cmp(&right.provider_order))
    });
}
