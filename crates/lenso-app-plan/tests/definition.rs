use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan,
    authoring::{
        AppDefinition, BindingDecision, DefinitionResolutionError, ModuleCatalog, ModuleDescriptor,
        ModuleSelection,
    },
};

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
