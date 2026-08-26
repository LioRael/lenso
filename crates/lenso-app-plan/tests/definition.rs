use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan, RequestAdmissionPlan,
    authoring::{
        AppDefinition, BindingDecision, BindingPolicy, DefinitionResolutionError, ModuleCatalog,
        ModuleDescriptor, ModuleSelection,
    },
};
use serde_json::json;

const CAPABILITY: &str = "example.model@1";
const VERSION: &str = "1.0.0";

fn provider(package: &str) -> ModuleDescriptor {
    ModuleDescriptor::new(package, "1.2.3").with_capability(CapabilityEndpointPlan::new(
        CAPABILITY,
        VERSION,
        ["complete"],
    ))
}

fn consumer() -> ModuleDescriptor {
    ModuleDescriptor::new("example.agent", "2.0.0")
        .with_requirement(CapabilityRequirementPlan::one(CAPABILITY, VERSION))
}

#[test]
fn definition_derives_unique_binding_and_descriptor_facts() {
    let catalog = ModuleCatalog::new([consumer(), provider("example.model")]).unwrap();
    let definition = AppDefinition::new("agent")
        .with_module(ModuleSelection::new("agent", "example.agent"))
        .with_module(ModuleSelection::new("model", "example.model"));

    let composition = definition.derive(&catalog).unwrap();
    let plan = composition.resolve().unwrap();

    assert_eq!(plan.module_instances().len(), 2);
    assert_eq!(plan.capability_bindings().len(), 1);
    assert_eq!(plan.capability_bindings()[0].provider_instance(), "model");
    let model = plan
        .module_instances()
        .iter()
        .find(|module| module.instance_key() == "model")
        .unwrap();
    assert_eq!(model.package_revision(), "1.2.3");
    assert_eq!(model.provided_capabilities()[0].operations(), &["complete"]);
}

#[test]
fn ambiguous_one_returns_stable_decision_request() {
    let catalog =
        ModuleCatalog::new([consumer(), provider("model.a"), provider("model.b")]).unwrap();
    let definition = AppDefinition::new("agent")
        .with_module(ModuleSelection::new("agent", "example.agent"))
        .with_module(ModuleSelection::new("z-model", "model.b"))
        .with_module(ModuleSelection::new("a-model", "model.a"));

    assert_eq!(
        definition.derive(&catalog),
        Err(DefinitionResolutionError::NeedsDecision {
            consumer: "agent".to_owned(),
            capability_id: CAPABILITY.to_owned(),
            candidates: vec!["a-model".to_owned(), "z-model".to_owned()],
        })
    );
}

#[test]
fn explicit_decision_resolves_ambiguity() {
    let catalog =
        ModuleCatalog::new([consumer(), provider("model.a"), provider("model.b")]).unwrap();
    let definition = AppDefinition::new("agent")
        .with_module(ModuleSelection::new("agent", "example.agent"))
        .with_module(ModuleSelection::new("a-model", "model.a"))
        .with_module(ModuleSelection::new("b-model", "model.b"))
        .with_decision(BindingDecision::new("agent", CAPABILITY, "b-model"));

    let plan = definition.derive(&catalog).unwrap().resolve().unwrap();
    assert_eq!(plan.capability_bindings()[0].provider_instance(), "b-model");
}

#[test]
fn many_binds_every_provider_in_stable_order() {
    let collector = ModuleDescriptor::new("example.tools", "1.0.0")
        .with_requirement(CapabilityRequirementPlan::many(CAPABILITY, VERSION));
    let catalog =
        ModuleCatalog::new([collector, provider("model.a"), provider("model.b")]).unwrap();
    let definition = AppDefinition::new("tools")
        .with_module(ModuleSelection::new("tools", "example.tools"))
        .with_module(ModuleSelection::new("z-provider", "model.b"))
        .with_module(ModuleSelection::new("a-provider", "model.a"));

    let plan = definition.derive(&catalog).unwrap().resolve().unwrap();
    let providers = plan
        .capability_bindings()
        .iter()
        .map(|binding| (binding.provider_instance(), binding.provider_order()))
        .collect::<Vec<_>>();
    assert_eq!(providers, vec![("a-provider", 0), ("z-provider", 1)]);
}

#[test]
fn binding_policy_overrides_one_derived_binding_admission() {
    let catalog = ModuleCatalog::new([consumer(), provider("example.model")]).unwrap();
    let definition = AppDefinition::new("agent")
        .with_module(ModuleSelection::new("agent", "example.agent"))
        .with_module(ModuleSelection::new("model", "example.model"))
        .with_binding_policy(BindingPolicy::with_limits(
            "agent", CAPABILITY, "model", 3, 4,
        ));

    let encoded = serde_json::to_value(&definition).unwrap();
    assert_eq!(
        encoded["binding_policies"][0]["admission"],
        json!({"queue_capacity": 3, "max_concurrency": 4})
    );
    let plan = definition.derive(&catalog).unwrap().resolve().unwrap();
    let binding = &plan.capability_bindings()[0];
    assert!(binding.has_explicit_admission());
    assert_eq!(binding.admission(), RequestAdmissionPlan::new(3, 4));
}

#[test]
fn definition_without_binding_policies_remains_deserializable() {
    let definition: AppDefinition = serde_json::from_value(json!({
        "schema_version": 1,
        "name": "agent",
        "modules": [],
        "decisions": [],
        "execution_lanes": []
    }))
    .unwrap();

    assert!(definition.binding_policies().is_empty());
}

#[test]
fn many_binding_policies_can_assign_each_provider_distinct_capacity() {
    let collector = ModuleDescriptor::new("example.tools", "1.0.0")
        .with_requirement(CapabilityRequirementPlan::many(CAPABILITY, VERSION));
    let catalog =
        ModuleCatalog::new([collector, provider("model.a"), provider("model.b")]).unwrap();
    let definition = AppDefinition::new("tools")
        .with_module(ModuleSelection::new("tools", "example.tools"))
        .with_module(ModuleSelection::new("a-provider", "model.a"))
        .with_module(ModuleSelection::new("b-provider", "model.b"))
        .with_binding_policy(BindingPolicy::with_limits(
            "tools",
            CAPABILITY,
            "a-provider",
            0,
            2,
        ))
        .with_binding_policy(BindingPolicy::with_limits(
            "tools",
            CAPABILITY,
            "b-provider",
            1,
            8,
        ));

    let plan = definition.derive(&catalog).unwrap().resolve().unwrap();
    assert_eq!(
        plan.capability_bindings()
            .iter()
            .map(|binding| (binding.provider_instance(), binding.admission()))
            .collect::<Vec<_>>(),
        [
            ("a-provider", RequestAdmissionPlan::new(0, 2)),
            ("b-provider", RequestAdmissionPlan::new(1, 8))
        ]
    );
}

#[test]
fn binding_policies_fail_closed_when_duplicate_unused_or_invalid() {
    let catalog = ModuleCatalog::new([consumer(), provider("example.model")]).unwrap();
    let base = || {
        AppDefinition::new("agent")
            .with_module(ModuleSelection::new("agent", "example.agent"))
            .with_module(ModuleSelection::new("model", "example.model"))
    };
    let duplicate = base()
        .with_binding_policy(BindingPolicy::with_limits(
            "agent", CAPABILITY, "model", 0, 2,
        ))
        .with_binding_policy(BindingPolicy::with_limits(
            "agent", CAPABILITY, "model", 0, 3,
        ));
    assert_eq!(
        duplicate.derive(&catalog),
        Err(DefinitionResolutionError::DuplicateBindingPolicy {
            consumer: "agent".to_owned(),
            capability_id: CAPABILITY.to_owned(),
            provider: "model".to_owned(),
        })
    );

    let unused = base().with_binding_policy(BindingPolicy::with_limits(
        "agent", CAPABILITY, "missing", 0, 2,
    ));
    assert_eq!(
        unused.derive(&catalog),
        Err(DefinitionResolutionError::UnusedBindingPolicy {
            consumer: "agent".to_owned(),
            capability_id: CAPABILITY.to_owned(),
            provider: "missing".to_owned(),
        })
    );

    let invalid = base().with_binding_policy(BindingPolicy::with_limits(
        "agent", CAPABILITY, "model", 0, 0,
    ));
    assert!(matches!(
        invalid.derive(&catalog),
        Err(DefinitionResolutionError::InvalidComposition(_))
    ));
}

#[test]
fn package_owned_schema_validates_configuration_before_materialization() {
    let configurable = ModuleDescriptor::new("example.configurable", "1.0.0")
        .with_configuration_schema(json!({
            "type": "object",
            "required": ["model"],
            "properties": {"model": {"const": "fixture"}},
            "additionalProperties": false
        }));
    let catalog = ModuleCatalog::new([configurable]).unwrap();
    let valid = AppDefinition::new("valid").with_module(
        ModuleSelection::new("configured", "example.configurable")
            .with_configuration(json!({"model": "fixture"})),
    );
    assert!(valid.derive(&catalog).is_ok());

    let invalid = AppDefinition::new("invalid").with_module(
        ModuleSelection::new("configured", "example.configurable")
            .with_configuration(json!({"model": "other"})),
    );
    assert!(matches!(
        invalid.derive(&catalog),
        Err(DefinitionResolutionError::InvalidConfiguration { .. })
    ));
}

#[test]
fn package_owned_schema_enforces_numeric_minimum() {
    let descriptor =
        ModuleDescriptor::new("example.numeric", "1.0.0").with_configuration_schema(json!({
            "type": "object",
            "required": ["ttl"],
            "properties": {"ttl": {"type": "integer", "minimum": 1}},
            "additionalProperties": false
        }));
    let catalog = ModuleCatalog::new([descriptor]).unwrap();
    let boundary = AppDefinition::new("boundary").with_module(
        ModuleSelection::new("numeric", "example.numeric").with_configuration(json!({"ttl": 1})),
    );
    assert!(boundary.derive(&catalog).is_ok());

    let below = AppDefinition::new("below").with_module(
        ModuleSelection::new("numeric", "example.numeric").with_configuration(json!({"ttl": 0})),
    );
    assert_eq!(
        below.derive(&catalog),
        Err(DefinitionResolutionError::InvalidConfiguration {
            instance_key: "numeric".to_owned(),
            detail: "$.ttl: number must be greater than or equal to 1".to_owned(),
        })
    );
}

#[test]
fn malformed_numeric_minimum_fails_closed() {
    let descriptor =
        ModuleDescriptor::new("example.numeric", "1.0.0").with_configuration_schema(json!({
            "type": "integer",
            "minimum": "one"
        }));
    let catalog = ModuleCatalog::new([descriptor]).unwrap();
    let definition = AppDefinition::new("malformed").with_module(
        ModuleSelection::new("numeric", "example.numeric").with_configuration(json!(1)),
    );

    assert_eq!(
        definition.derive(&catalog),
        Err(DefinitionResolutionError::InvalidConfiguration {
            instance_key: "numeric".to_owned(),
            detail: "$: schema minimum must be a number".to_owned(),
        })
    );
}

#[test]
fn non_empty_configuration_without_package_schema_fails_closed() {
    let catalog = ModuleCatalog::new([ModuleDescriptor::new("example.raw", "1.0.0")]).unwrap();
    let definition = AppDefinition::new("invalid").with_module(
        ModuleSelection::new("raw", "example.raw").with_configuration(json!({"undeclared": true})),
    );

    assert_eq!(
        definition.derive(&catalog),
        Err(DefinitionResolutionError::InvalidConfiguration {
            instance_key: "raw".to_owned(),
            detail: "$: non-empty configuration requires a package-owned schema".to_owned(),
        })
    );
}

#[test]
fn sensitive_configuration_accepts_only_secret_references() {
    let descriptor =
        ModuleDescriptor::new("example.secret", "1.0.0").with_configuration_schema(json!({
            "type": "object",
            "required": ["token"],
            "properties": {"token": {"x-lenso-sensitive": true}},
            "additionalProperties": false
        }));
    let catalog = ModuleCatalog::new([descriptor]).unwrap();
    let raw = AppDefinition::new("raw").with_module(
        ModuleSelection::new("secret", "example.secret")
            .with_configuration(json!({"token": "plaintext"})),
    );
    assert!(matches!(
        raw.derive(&catalog),
        Err(DefinitionResolutionError::InvalidConfiguration { .. })
    ));
    let reference = AppDefinition::new("reference").with_module(
        ModuleSelection::new("secret", "example.secret")
            .with_configuration(json!({"token": {"secret_ref": "API_TOKEN"}})),
    );
    assert!(reference.derive(&catalog).is_ok());
}
