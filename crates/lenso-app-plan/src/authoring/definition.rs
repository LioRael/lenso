use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppComposition, CapabilityBinding, CapabilityCardinality, CapabilityEndpointPlan,
    CapabilityRequirementPlan, ExecutionClassId, ExecutionLaneId, ExecutionLanePlan,
    ModuleCriticality, ModuleInstancePlan, PlanResolutionError, RestartPolicy,
};

fn empty_configuration() -> Value {
    Value::Object(serde_json::Map::new())
}

fn default_entrypoint() -> String {
    "default".to_owned()
}

fn default_execution_lanes() -> Vec<ExecutionLanePlan> {
    vec![ExecutionLanePlan::new("main")]
}

/// Package-owned facts for one executable Module entrypoint.
///
/// This data is generated and shipped by a Module package. App authors select
/// descriptors; they do not repeat Capability or lifecycle metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModuleDescriptor {
    package_id: String,
    package_revision: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration_schema: Option<Value>,
    provided_capabilities: Vec<CapabilityEndpointPlan>,
    required_capabilities: Vec<CapabilityRequirementPlan>,
    execution_class: ExecutionClassId,
    restart_policy: RestartPolicy,
    criticality: ModuleCriticality,
}

impl ModuleDescriptor {
    /// Starts a descriptor for one locked package revision and entrypoint.
    pub fn new(package_id: impl Into<String>, package_revision: impl Into<String>) -> Self {
        Self {
            package_id: package_id.into(),
            package_revision: package_revision.into(),
            entrypoint: default_entrypoint(),
            configuration_schema: None,
            provided_capabilities: Vec::new(),
            required_capabilities: Vec::new(),
            execution_class: ExecutionClassId::native_rust(),
            restart_policy: RestartPolicy::default(),
            criticality: ModuleCriticality::default(),
        }
    }

    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }

    /// Selects the package-owned JSON Schema used to validate Instance configuration.
    #[must_use]
    pub fn with_configuration_schema(mut self, schema: Value) -> Self {
        self.configuration_schema = Some(schema);
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
    pub const fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    #[must_use]
    pub const fn with_criticality(mut self, criticality: ModuleCriticality) -> Self {
        self.criticality = criticality;
        self
    }

    pub fn package_id(&self) -> &str {
        &self.package_id
    }
    pub fn package_revision(&self) -> &str {
        &self.package_revision
    }
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }
    pub const fn configuration_schema(&self) -> Option<&Value> {
        self.configuration_schema.as_ref()
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
    pub const fn criticality(&self) -> ModuleCriticality {
        self.criticality
    }
}

/// Descriptor set discovered from the exact package lock selected for an App.
#[derive(Clone, Debug, Default)]
pub struct ModuleCatalog {
    descriptors: BTreeMap<(String, String), ModuleDescriptor>,
}

impl ModuleCatalog {
    pub fn new(
        descriptors: impl IntoIterator<Item = ModuleDescriptor>,
    ) -> Result<Self, DefinitionResolutionError> {
        let mut catalog = Self::default();
        for descriptor in descriptors {
            catalog.insert(descriptor)?;
        }
        Ok(catalog)
    }

    pub fn insert(
        &mut self,
        descriptor: ModuleDescriptor,
    ) -> Result<(), DefinitionResolutionError> {
        let key = (descriptor.package_id.clone(), descriptor.entrypoint.clone());
        if self.descriptors.insert(key.clone(), descriptor).is_some() {
            return Err(DefinitionResolutionError::DuplicateDescriptor {
                package_id: key.0,
                entrypoint: key.1,
            });
        }
        Ok(())
    }

    pub fn get(&self, package_id: &str, entrypoint: &str) -> Option<&ModuleDescriptor> {
        self.descriptors
            .get(&(package_id.to_owned(), entrypoint.to_owned()))
    }
}

/// One App-local use of a package-owned Module descriptor.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ModuleSelection {
    key: String,
    package: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default = "empty_configuration")]
    configuration: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_lane: Option<String>,
}

impl ModuleSelection {
    pub fn new(key: impl Into<String>, package: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            package: package.into(),
            entrypoint: default_entrypoint(),
            configuration: empty_configuration(),
            execution_lane: None,
        }
    }

    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }

    #[must_use]
    pub fn with_configuration(mut self, configuration: Value) -> Self {
        self.configuration = configuration;
        self
    }

    #[must_use]
    pub fn with_execution_lane(mut self, execution_lane: impl Into<String>) -> Self {
        self.execution_lane = Some(execution_lane.into());
        self
    }

    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn package(&self) -> &str {
        &self.package
    }
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }
    pub fn configuration(&self) -> &Value {
        &self.configuration
    }
    pub fn execution_lane(&self) -> Option<&str> {
        self.execution_lane.as_deref()
    }
}

/// An explicit answer to an otherwise ambiguous one/optional Capability slot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BindingDecision {
    consumer: String,
    capability_id: String,
    provider: String,
}

impl BindingDecision {
    pub fn new(
        consumer: impl Into<String>,
        capability_id: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            consumer: consumer.into(),
            capability_id: capability_id.into(),
            provider: provider.into(),
        }
    }
    pub fn consumer(&self) -> &str {
        &self.consumer
    }
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    pub fn provider(&self) -> &str {
        &self.provider
    }
}

/// Small, human-authored input for one App variant.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppDefinition {
    name: String,
    #[serde(default)]
    modules: Vec<ModuleSelection>,
    #[serde(default)]
    decisions: Vec<BindingDecision>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLanePlan>,
}

impl AppDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            modules: Vec::new(),
            decisions: Vec::new(),
            execution_lanes: default_execution_lanes(),
        }
    }

    #[must_use]
    pub fn with_module(mut self, module: ModuleSelection) -> Self {
        self.modules.push(module);
        self
    }
    #[must_use]
    pub fn with_decision(mut self, decision: BindingDecision) -> Self {
        self.decisions.push(decision);
        self
    }
    #[must_use]
    pub fn with_execution_lanes(mut self, lanes: Vec<ExecutionLanePlan>) -> Self {
        self.execution_lanes = lanes;
        self
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn modules(&self) -> &[ModuleSelection] {
        &self.modules
    }
    pub fn decisions(&self) -> &[BindingDecision] {
        &self.decisions
    }

    /// Derives the complete, explicit App Composition from selected descriptors.
    pub fn derive(
        &self,
        catalog: &ModuleCatalog,
    ) -> Result<AppComposition, DefinitionResolutionError> {
        let instances = materialize_instances(&self.modules, catalog)?;
        let bindings = derive_bindings(&instances, &self.decisions)?;
        let composition = AppComposition::new(instances, bindings)
            .with_execution_lanes(self.execution_lanes.clone());
        composition
            .resolve()
            .map_err(DefinitionResolutionError::InvalidComposition)?;
        Ok(composition)
    }
}

fn materialize_instances(
    selections: &[ModuleSelection],
    catalog: &ModuleCatalog,
) -> Result<Vec<ModuleInstancePlan>, DefinitionResolutionError> {
    let mut instances = Vec::with_capacity(selections.len());
    let mut instance_keys = BTreeSet::new();
    for selection in selections {
        if !instance_keys.insert(selection.key.clone()) {
            return Err(DefinitionResolutionError::DuplicateModuleSelection {
                instance_key: selection.key.clone(),
            });
        }
        let descriptor = catalog
            .get(selection.package(), selection.entrypoint())
            .ok_or_else(|| DefinitionResolutionError::UnknownDescriptor {
                instance_key: selection.key.clone(),
                package_id: selection.package.clone(),
                entrypoint: selection.entrypoint.clone(),
            })?;
        instances.push(materialize_instance(selection, descriptor)?);
    }
    Ok(instances)
}

fn materialize_instance(
    selection: &ModuleSelection,
    descriptor: &ModuleDescriptor,
) -> Result<ModuleInstancePlan, DefinitionResolutionError> {
    validate_configuration(
        selection.configuration(),
        descriptor.configuration_schema(),
        selection.key(),
    )?;
    let configuration = serde_json::to_string(&selection.configuration).map_err(|error| {
        DefinitionResolutionError::InvalidConfiguration {
            instance_key: selection.key.clone(),
            detail: error.to_string(),
        }
    })?;
    let mut instance = ModuleInstancePlan::new(selection.key(), descriptor.package_id())
        .with_entrypoint(descriptor.entrypoint())
        .with_package_revision(descriptor.package_revision())
        .with_configuration(configuration)
        .with_execution_class(descriptor.execution_class().clone())
        .with_restart_policy(descriptor.restart_policy())
        .with_criticality(descriptor.criticality())
        .with_execution_lane(ExecutionLaneId::new(
            selection.execution_lane().unwrap_or("main"),
        ));
    for capability in descriptor.provided_capabilities() {
        instance = instance.with_capability(capability.clone());
    }
    for requirement in descriptor.required_capabilities() {
        instance = instance.with_requirement(requirement.clone());
    }
    Ok(instance)
}

const SUPPORTED_SCHEMA_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "const",
    "enum",
    "items",
    "properties",
    "required",
    "type",
    "x-lenso-sensitive",
];
const SCHEMA_METADATA_KEYWORDS: &[&str] = &[
    "$anchor",
    "$comment",
    "$id",
    "$schema",
    "default",
    "deprecated",
    "description",
    "examples",
    "readOnly",
    "title",
    "writeOnly",
];

fn validate_configuration(
    configuration: &Value,
    schema: Option<&Value>,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    match schema {
        Some(schema) => validate_json_schema(configuration, schema, "$", instance_key),
        None if configuration == &empty_configuration() => Ok(()),
        None => Err(invalid_configuration(
            instance_key,
            "$",
            "non-empty configuration requires a package-owned schema",
        )),
    }
}

fn validate_json_schema(
    value: &Value,
    schema: &Value,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let schema = schema.as_object().ok_or_else(|| {
        invalid_configuration(
            instance_key,
            path,
            "configuration schema must be a JSON object",
        )
    })?;
    if let Some(keyword) = schema.keys().find(|keyword| {
        !SUPPORTED_SCHEMA_KEYWORDS.contains(&keyword.as_str())
            && !SCHEMA_METADATA_KEYWORDS.contains(&keyword.as_str())
    }) {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("unsupported JSON Schema keyword `{keyword}`"),
        ));
    }
    if schema
        .get("x-lenso-sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let secret_reference = value.as_object().is_some_and(|object| {
            object.len() == 1 && object.get("secret_ref").is_some_and(Value::is_string)
        });
        return secret_reference.then_some(()).ok_or_else(|| {
            invalid_configuration(instance_key, path, "sensitive value must be a secret_ref")
        });
    }
    validate_schema_type(value, schema, path, instance_key)?;
    if let Some(expected) = schema.get("const")
        && value != expected
    {
        return Err(invalid_configuration(
            instance_key,
            path,
            "value does not match schema const",
        ));
    }
    if let Some(values) = schema.get("enum") {
        let values = values.as_array().ok_or_else(|| {
            invalid_configuration(instance_key, path, "schema enum must be an array")
        })?;
        if !values.iter().any(|expected| expected == value) {
            return Err(invalid_configuration(
                instance_key,
                path,
                "value is not in schema enum",
            ));
        }
    }
    validate_required(value, schema, path, instance_key)?;
    validate_properties(value, schema, path, instance_key)?;
    validate_items(value, schema, path, instance_key)
}

fn validate_schema_type(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(expected) = schema.get("type") else {
        return Ok(());
    };
    let expected = expected
        .as_str()
        .ok_or_else(|| invalid_configuration(instance_key, path, "schema type must be a string"))?;
    if ![
        "array", "boolean", "integer", "null", "number", "object", "string",
    ]
    .contains(&expected)
    {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("unsupported JSON Schema type `{expected}`"),
        ));
    }
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let integer = value
        .as_number()
        .is_some_and(|number| number.is_i64() || number.is_u64());
    if expected != actual && !(expected == "integer" && integer) {
        return Err(invalid_configuration(
            instance_key,
            path,
            format!("expected {expected}, found {actual}"),
        ));
    }
    Ok(())
}

fn validate_required(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(required) = schema.get("required") else {
        return Ok(());
    };
    let required = required.as_array().ok_or_else(|| {
        invalid_configuration(instance_key, path, "schema required must be an array")
    })?;
    let object = value.as_object().ok_or_else(|| {
        invalid_configuration(instance_key, path, "required fields need an object")
    })?;
    for name in required {
        let name = name.as_str().ok_or_else(|| {
            invalid_configuration(
                instance_key,
                path,
                "schema required entries must be strings",
            )
        })?;
        if !object.contains_key(name) {
            return Err(invalid_configuration(
                instance_key,
                &format!("{path}.{name}"),
                "required field is missing",
            ));
        }
    }
    Ok(())
}

fn validate_properties(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let empty = serde_json::Map::new();
    let properties = match schema.get("properties") {
        Some(properties) => properties.as_object().ok_or_else(|| {
            invalid_configuration(instance_key, path, "schema properties must be an object")
        })?,
        None => &empty,
    };
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    for (name, child_schema) in properties {
        if let Some(child) = object.get(name) {
            validate_json_schema(child, child_schema, &format!("{path}.{name}"), instance_key)?;
        }
    }
    if let Some(additional) = schema.get("additionalProperties") {
        match additional {
            Value::Bool(true) => {}
            Value::Bool(false) => {
                if let Some(name) = object.keys().find(|name| !properties.contains_key(*name)) {
                    return Err(invalid_configuration(
                        instance_key,
                        &format!("{path}.{name}"),
                        "additional property is not allowed",
                    ));
                }
            }
            additional_schema => {
                for (name, child) in object {
                    if !properties.contains_key(name) {
                        validate_json_schema(
                            child,
                            additional_schema,
                            &format!("{path}.{name}"),
                            instance_key,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_items(
    value: &Value,
    schema: &serde_json::Map<String, Value>,
    path: &str,
    instance_key: &str,
) -> Result<(), DefinitionResolutionError> {
    let Some(item_schema) = schema.get("items") else {
        return Ok(());
    };
    if !item_schema.is_object() {
        return Err(invalid_configuration(
            instance_key,
            path,
            "schema items must be an object",
        ));
    }
    let Some(items) = value.as_array() else {
        return Ok(());
    };
    for (index, item) in items.iter().enumerate() {
        validate_json_schema(item, item_schema, &format!("{path}[{index}]"), instance_key)?;
    }
    Ok(())
}

fn invalid_configuration(
    instance_key: &str,
    path: &str,
    detail: impl Into<String>,
) -> DefinitionResolutionError {
    DefinitionResolutionError::InvalidConfiguration {
        instance_key: instance_key.to_owned(),
        detail: format!("{path}: {}", detail.into()),
    }
}

fn derive_bindings(
    instances: &[ModuleInstancePlan],
    authored_decisions: &[BindingDecision],
) -> Result<Vec<CapabilityBinding>, DefinitionResolutionError> {
    let decisions = index_decisions(authored_decisions)?;
    let mut consumed_decisions = BTreeSet::new();
    let mut bindings = Vec::new();
    for consumer in instances {
        for requirement in consumer.required_capabilities() {
            let key = (
                consumer.instance_key().to_owned(),
                requirement.capability_id().to_owned(),
            );
            let candidates = provider_candidates(instances, requirement);
            let selected = select_providers(
                requirement,
                &key,
                candidates,
                &decisions,
                &mut consumed_decisions,
            )?;
            bindings.extend(selected.into_iter().map(|provider| {
                CapabilityBinding::new(
                    consumer.instance_key(),
                    requirement.capability_id(),
                    requirement.descriptor_version(),
                    provider,
                )
            }));
        }
    }
    if let Some(((consumer, capability_id), provider)) = decisions
        .iter()
        .find(|(key, _)| !consumed_decisions.contains(*key))
    {
        return Err(DefinitionResolutionError::UnusedDecision {
            consumer: consumer.clone(),
            capability_id: capability_id.clone(),
            provider: provider.clone(),
        });
    }
    Ok(bindings)
}

fn provider_candidates(
    instances: &[ModuleInstancePlan],
    requirement: &CapabilityRequirementPlan,
) -> Vec<String> {
    let mut candidates = instances
        .iter()
        .filter(|provider| {
            provider.provided_capabilities().iter().any(|endpoint| {
                endpoint.capability_id() == requirement.capability_id()
                    && endpoint.descriptor_version() == requirement.descriptor_version()
            })
        })
        .map(|provider| provider.instance_key().to_owned())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

fn select_providers(
    requirement: &CapabilityRequirementPlan,
    key: &(String, String),
    candidates: Vec<String>,
    decisions: &BTreeMap<(String, String), String>,
    consumed_decisions: &mut BTreeSet<(String, String)>,
) -> Result<Vec<String>, DefinitionResolutionError> {
    if requirement.cardinality() == CapabilityCardinality::Many {
        return Ok(candidates);
    }
    if let Some(provider) = decisions.get(key) {
        consumed_decisions.insert(key.clone());
        if !candidates.contains(provider) {
            return Err(DefinitionResolutionError::InvalidDecision {
                consumer: key.0.clone(),
                capability_id: key.1.clone(),
                provider: provider.clone(),
                candidates,
            });
        }
        return Ok(vec![provider.clone()]);
    }
    match candidates.len() {
        0 if requirement.cardinality() == CapabilityCardinality::One => {
            Err(DefinitionResolutionError::MissingProvider {
                consumer: key.0.clone(),
                capability_id: key.1.clone(),
                descriptor_version: requirement.descriptor_version().to_owned(),
            })
        }
        0 => Ok(Vec::new()),
        1 => Ok(candidates),
        _ => Err(DefinitionResolutionError::NeedsDecision {
            consumer: key.0.clone(),
            capability_id: key.1.clone(),
            candidates,
        }),
    }
}

fn index_decisions(
    decisions: &[BindingDecision],
) -> Result<BTreeMap<(String, String), String>, DefinitionResolutionError> {
    let mut indexed = BTreeMap::new();
    for decision in decisions {
        let key = (decision.consumer.clone(), decision.capability_id.clone());
        if indexed
            .insert(key.clone(), decision.provider.clone())
            .is_some()
        {
            return Err(DefinitionResolutionError::DuplicateDecision {
                consumer: key.0,
                capability_id: key.1,
            });
        }
    }
    Ok(indexed)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefinitionResolutionError {
    DuplicateDescriptor {
        package_id: String,
        entrypoint: String,
    },
    DuplicateModuleSelection {
        instance_key: String,
    },
    UnknownDescriptor {
        instance_key: String,
        package_id: String,
        entrypoint: String,
    },
    InvalidConfiguration {
        instance_key: String,
        detail: String,
    },
    DuplicateDecision {
        consumer: String,
        capability_id: String,
    },
    MissingProvider {
        consumer: String,
        capability_id: String,
        descriptor_version: String,
    },
    NeedsDecision {
        consumer: String,
        capability_id: String,
        candidates: Vec<String>,
    },
    InvalidDecision {
        consumer: String,
        capability_id: String,
        provider: String,
        candidates: Vec<String>,
    },
    UnusedDecision {
        consumer: String,
        capability_id: String,
        provider: String,
    },
    InvalidComposition(PlanResolutionError),
}

impl fmt::Display for DefinitionResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDescriptor {
                package_id,
                entrypoint,
            } => write!(
                formatter,
                "duplicate Module Descriptor `{package_id}#{entrypoint}`"
            ),
            Self::DuplicateModuleSelection { instance_key } => {
                write!(formatter, "duplicate Module selection `{instance_key}`")
            }
            Self::UnknownDescriptor {
                instance_key,
                package_id,
                entrypoint,
            } => write!(
                formatter,
                "Module selection `{instance_key}` refers to unknown descriptor `{package_id}#{entrypoint}`"
            ),
            Self::InvalidConfiguration {
                instance_key,
                detail,
            } => write!(
                formatter,
                "Module selection `{instance_key}` has invalid configuration: {detail}"
            ),
            Self::DuplicateDecision {
                consumer,
                capability_id,
            } => write!(
                formatter,
                "duplicate binding decision for `{consumer}` Capability `{capability_id}`"
            ),
            Self::MissingProvider {
                consumer,
                capability_id,
                descriptor_version,
            } => write!(
                formatter,
                "consumer `{consumer}` has no provider for Capability `{capability_id}` version `{descriptor_version}`"
            ),
            Self::NeedsDecision {
                consumer,
                capability_id,
                candidates,
            } => write!(
                formatter,
                "consumer `{consumer}` needs a provider decision for Capability `{capability_id}`; candidates: {}",
                candidates.join(", ")
            ),
            Self::InvalidDecision {
                consumer,
                capability_id,
                provider,
                candidates,
            } => write!(
                formatter,
                "binding decision `{consumer}` -> `{provider}` for Capability `{capability_id}` is invalid; candidates: {}",
                candidates.join(", ")
            ),
            Self::UnusedDecision {
                consumer,
                capability_id,
                provider,
            } => write!(
                formatter,
                "binding decision `{consumer}` -> `{provider}` for Capability `{capability_id}` does not match a one/optional requirement"
            ),
            Self::InvalidComposition(error) => {
                write!(formatter, "derived App Composition is invalid: {error}")
            }
        }
    }
}

impl Error for DefinitionResolutionError {}
