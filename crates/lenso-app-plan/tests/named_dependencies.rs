use lenso_app_plan::authoring::*;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan, TerminalPolicy,
};

const CAP: &str = "example.storage@1";

fn consumer() -> PluginInstancePlan {
    PluginInstancePlan::new("consumer", "consumer")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_requirement(CapabilityRequirementPlan::one(CAP, "1").with_requirement_id("source"))
        .with_requirement(
            CapabilityRequirementPlan::optional(CAP, "1").with_requirement_id("destination"),
        )
}

fn provider(id: &str) -> PluginInstancePlan {
    PluginInstancePlan::new(id, "storage").with_capability(CapabilityEndpointPlan::new(
        CAP,
        "1",
        ["read"],
    ))
}

fn binding(id: &str, provider: &str) -> CapabilityBinding {
    CapabilityBinding::new("consumer", CAP, "1", provider).with_requirement_id(id)
}

#[test]
fn named_requirements_select_independently_and_allow_the_same_target() {
    for destination in ["a", "b"] {
        let plan = AppComposition::new(
            vec![consumer(), provider("a"), provider("b")],
            vec![binding("source", "a"), binding("destination", destination)],
        )
        .resolve()
        .unwrap();
        assert_eq!(
            plan.capability_bindings()
                .iter()
                .map(|binding| (
                    binding.requirement_id(),
                    binding.provider_instance(),
                    binding.provider_order()
                ))
                .collect::<Vec<_>>(),
            vec![("destination", destination, 0), ("source", "a", 0)]
        );
        let json = serde_json::to_string(&plan).unwrap();
        assert_eq!(
            serde_json::from_str::<ResolvedAppPlan>(&json).unwrap(),
            plan
        );
    }
}

#[test]
fn names_versions_and_capabilities_cannot_be_confused() {
    let duplicate = consumer().with_requirement(
        CapabilityRequirementPlan::many("other", "1").with_requirement_id("source"),
    );
    assert!(
        AppComposition::new(vec![duplicate, provider("a")], vec![binding("source", "a")])
            .resolve()
            .is_err()
    );
    for name in ["", "Source", "~example.storage@1", "a-b"] {
        let consumer = PluginInstancePlan::new("consumer", "consumer")
            .with_authoring(2, "lenso.native-authoring@2")
            .with_requirement(
                CapabilityRequirementPlan::optional(CAP, "1").with_requirement_id(name),
            );
        assert!(
            AppComposition::new(vec![consumer], vec![])
                .resolve()
                .is_err()
        );
    }
    for invalid in [
        binding("unknown", "a"),
        CapabilityBinding::new("consumer", "other", "1", "a").with_requirement_id("source"),
        CapabilityBinding::new("consumer", CAP, "2", "a").with_requirement_id("source"),
    ] {
        assert!(
            AppComposition::new(vec![consumer(), provider("a")], vec![invalid])
                .resolve()
                .is_err()
        );
    }
}

#[test]
fn old_plan_decoding_is_explicit_and_cannot_smuggle_new_authoring() {
    let old = AppComposition::new(
        vec![
            provider("a"),
            PluginInstancePlan::new("consumer", "consumer")
                .with_requirement(CapabilityRequirementPlan::one(CAP, "1")),
        ],
        vec![CapabilityBinding::new("consumer", CAP, "1", "a")],
    )
    .resolve()
    .unwrap();
    let mut json = serde_json::to_value(&old).unwrap();
    json["schema_version"] = 2.into();
    assert!(serde_json::from_value::<ResolvedAppPlan>(json.clone()).is_err());
    json.as_object_mut().unwrap().remove("terminal_policy");
    for instance in json["plugin_instances"].as_array_mut().unwrap() {
        instance
            .as_object_mut()
            .unwrap()
            .remove("authoring_version");
        instance.as_object_mut().unwrap().remove("runtime_profile");
        for requirement in instance["required_capabilities"].as_array_mut().unwrap() {
            requirement
                .as_object_mut()
                .unwrap()
                .remove("requirement_id");
        }
    }
    for binding in json["capability_bindings"].as_array_mut().unwrap() {
        binding.as_object_mut().unwrap().remove("requirement_id");
    }
    let decoded: ResolvedAppPlan = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, old);
    decoded.validate().unwrap();
    assert_eq!(
        decoded.capability_bindings()[0].requirement_id(),
        "~example.storage@1"
    );
    decoded
        .with_terminal_policy(TerminalPolicy::HostEssential {
            roots: vec![],
            closure: vec![],
        })
        .validate()
        .unwrap();
}

#[test]
fn host_essential_recomputes_only_the_transitive_one_closure() {
    let upstream = PluginInstancePlan::new("upstream", "storage")
        .with_capability(CapabilityEndpointPlan::new(CAP, "1", ["read"]));
    let source = PluginInstancePlan::new("source", "source")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_capability(CapabilityEndpointPlan::new("source@1", "1", ["read"]))
        .with_requirement(CapabilityRequirementPlan::one(CAP, "1").with_requirement_id("upstream"));
    let optional = PluginInstancePlan::new("optional", "storage")
        .with_capability(CapabilityEndpointPlan::new(CAP, "1", ["read"]));
    let root = PluginInstancePlan::new("root", "root")
        .with_authoring(2, "lenso.native-authoring@2")
        .with_requirement(
            CapabilityRequirementPlan::one("source@1", "1").with_requirement_id("source"),
        )
        .with_requirement(
            CapabilityRequirementPlan::optional(CAP, "1").with_requirement_id("cache"),
        );
    let plan = AppComposition::new(
        vec![root, source, upstream, optional],
        vec![
            CapabilityBinding::new("root", "source@1", "1", "source").with_requirement_id("source"),
            CapabilityBinding::new("root", CAP, "1", "optional").with_requirement_id("cache"),
            CapabilityBinding::new("source", CAP, "1", "upstream").with_requirement_id("upstream"),
        ],
    )
    .resolve()
    .unwrap();
    let adopted = plan
        .clone()
        .with_terminal_policy(TerminalPolicy::HostEssential {
            roots: vec!["root".to_owned()],
            closure: vec![
                "root".to_owned(),
                "source".to_owned(),
                "upstream".to_owned(),
            ],
        });
    adopted.validate().unwrap();
    assert!(adopted.plugin_instance_is_terminal("root"));
    assert!(adopted.plugin_instance_is_terminal("source"));
    assert!(adopted.plugin_instance_is_terminal("upstream"));
    assert!(!adopted.plugin_instance_is_terminal("optional"));

    assert!(
        plan.with_terminal_policy(TerminalPolicy::HostEssential {
            roots: vec!["root".to_owned()],
            closure: vec!["root".to_owned(), "source".to_owned()],
        })
        .validate()
        .is_err()
    );
}

fn selectable_host() -> HostCatalog {
    HostCatalog::new(
        [HostSlot::one("consumer"), HostSlot::many("storage")],
        [
            HostPluginRelease::new(
                PluginDescriptor::new("consumer", "1", "consumer")
                    .with_authoring(2, "lenso.native-authoring@2")
                    .with_requirement(
                        CapabilityRequirementPlan::optional(CAP, "1").with_requirement_id("source"),
                    ),
            ),
            HostPluginRelease::new(
                PluginDescriptor::new("storage", "1", "storage")
                    .with_capability(CapabilityEndpointPlan::new(CAP, "1", ["read"])),
            ),
        ],
        [HostDefaultPlugin::new("consumer", "default")],
    )
    .with_bindings([HostBinding::new(
        PluginInstanceId::new("consumer", "default"),
        CAP,
        "storage",
    )
    .with_requirement_id("source")
    .selectable(None)])
}

#[test]
fn proposal_materializes_unique_or_absent_choices_and_saved_intent_never_falls_back() {
    let host = selectable_host();
    let root = PluginRootSnapshot::new([], [PluginRootInstance::new("storage", "a")], []);
    assert!(resolve_plugin_root(&host, &root).is_err());
    let proposed = propose_plugin_root(&host, &root).unwrap();
    let adopted = root.with_dependency_choices(proposed.dependency_choices().to_vec());
    assert_eq!(resolve_plugin_root(&host, &adopted).unwrap(), proposed);
    let mut choices = adopted.dependency_choices().to_vec();
    choices[0].provider = Some(PluginInstanceId::new("storage", "missing"));
    assert!(propose_plugin_root(&host, &adopted.clone().with_dependency_choices(choices)).is_err());
    let mut choices = adopted.dependency_choices().to_vec();
    choices[0].provider = None;
    let absent = resolve_plugin_root(&host, &adopted.with_dependency_choices(choices)).unwrap();
    assert!(absent.plan().capability_bindings().is_empty());
    assert_eq!(
        absent
            .plan()
            .plugin_instances()
            .iter()
            .find(|instance| instance.package_id() == "consumer")
            .unwrap()
            .required_capabilities()
            .len(),
        1
    );
}
