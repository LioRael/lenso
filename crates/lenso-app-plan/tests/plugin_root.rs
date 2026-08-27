use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan,
    authoring::{
        HostBinding, HostCatalog, HostDefaultPlugin, HostPluginConfiguration, HostPluginRelease,
        HostSlot, PluginDescriptor, PluginInstanceId, PluginInstanceSource, PluginRootInstance,
        PluginRootResolutionError, PluginRootSnapshot, resolve_plugin_root,
    },
};
use serde_json::json;

fn model_descriptor(plugin_id: &str) -> PluginDescriptor {
    PluginDescriptor::new(plugin_id, "1.0.0", "model").with_capability(CapabilityEndpointPlan::new(
        "example.model@1",
        "1.0.0",
        ["complete"],
    ))
}

fn agent_descriptor() -> PluginDescriptor {
    PluginDescriptor::new("example.agent", "1.0.0", "agent")
        .with_requirement(CapabilityRequirementPlan::one("example.model@1", "1.0.0"))
}

fn host_with_default_model(replaceable: bool) -> HostCatalog {
    let model_slot = if replaceable {
        HostSlot::one("model").replaceable()
    } else {
        HostSlot::one("model")
    };
    HostCatalog::new(
        [model_slot, HostSlot::one("agent")],
        [
            HostPluginRelease::new(model_descriptor("example.model.fixture")),
            HostPluginRelease::new(agent_descriptor()),
        ],
        [
            HostDefaultPlugin::new("example.model.fixture", "default"),
            HostDefaultPlugin::new("example.agent", "default"),
        ],
    )
}

#[test]
fn empty_plugin_root_resolves_host_defaults_and_unique_binding() {
    let resolved = resolve_plugin_root(
        &host_with_default_model(true),
        &PluginRootSnapshot::default(),
    )
    .unwrap();

    assert_eq!(resolved.plan().plugin_instances().len(), 2);
    assert_eq!(resolved.plan().capability_bindings().len(), 1);
    assert_eq!(
        resolved.plan().capability_bindings()[0].provider_instance(),
        "example.model.fixture/default"
    );
}

#[test]
fn host_binding_scopes_one_requirement_to_the_selected_provider_instance() {
    let consumer = PluginDescriptor::new("example.consumer", "1.0.0", "consumer")
        .with_requirement(CapabilityRequirementPlan::one("example.model@1", "1.0.0"));
    let host = HostCatalog::new(
        [HostSlot::one("consumer"), HostSlot::many("model")],
        [
            HostPluginRelease::new(consumer),
            HostPluginRelease::new(model_descriptor("example.model.a")),
            HostPluginRelease::new(model_descriptor("example.model.b")),
        ],
        [HostDefaultPlugin::new("example.consumer", "default")],
    )
    .with_bindings([HostBinding::to_instance(
        PluginInstanceId::new("example.consumer", "default"),
        "example.model@1",
        PluginInstanceId::new("example.model.b", "primary"),
    )]);
    let root = PluginRootSnapshot::new(
        [],
        [
            PluginRootInstance::new("example.model.a", "primary"),
            PluginRootInstance::new("example.model.b", "primary"),
        ],
        [],
    );

    let resolved = resolve_plugin_root(&host, &root).unwrap();

    assert_eq!(resolved.plan().capability_bindings().len(), 1);
    assert_eq!(
        resolved.plan().capability_bindings()[0].provider_instance(),
        "example.model.b/primary"
    );
}

#[test]
fn host_binding_for_an_absent_optional_consumer_is_dormant() {
    let optional_consumer = PluginDescriptor::new("example.consumer", "1.0.0", "consumer")
        .with_requirement(CapabilityRequirementPlan::one("example.model@1", "1.0.0"));
    let host = HostCatalog::new(
        [HostSlot::optional("consumer"), HostSlot::one("model")],
        [
            HostPluginRelease::new(optional_consumer),
            HostPluginRelease::new(model_descriptor("example.model.fixture")),
        ],
        [HostDefaultPlugin::new("example.model.fixture", "default")],
    )
    .with_bindings([HostBinding::to_instance(
        PluginInstanceId::new("example.consumer", "default"),
        "example.model@1",
        PluginInstanceId::new("example.model.fixture", "default"),
    )]);

    let resolved = resolve_plugin_root(&host, &PluginRootSnapshot::default()).unwrap();

    assert_eq!(resolved.plan().plugin_instances().len(), 1);
    assert!(resolved.plan().capability_bindings().is_empty());
}

#[test]
fn root_configuration_overlays_host_and_package_defaults() {
    let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
        .with_configuration_defaults(json!({"model": "small", "limits": {"steps": 8}}))
        .with_configuration_schema(json!({
            "type": "object",
            "required": ["model", "limits"],
            "properties": {
                "model": {"type": "string"},
                "limits": {
                    "type": "object",
                    "required": ["steps"],
                    "properties": {"steps": {"type": "integer"}},
                    "additionalProperties": false
                }
            },
            "additionalProperties": false
        }));
    let host = HostCatalog::new(
        [HostSlot::one("agent")],
        [HostPluginRelease::new(descriptor)],
        [HostDefaultPlugin::new("example.agent", "default")
            .with_configuration(json!({"limits": {"steps": 4}}))],
    );
    let root = PluginRootSnapshot::new(
        [],
        [PluginRootInstance::new("example.agent", "default")
            .with_configuration(json!({"model": "large"}))],
        [],
    );

    let resolved = resolve_plugin_root(&host, &root).unwrap();

    assert_eq!(
        resolved.plan().plugin_instances()[0].configuration(),
        r#"{"limits":{"steps":4},"model":"large"}"#
    );
    assert_eq!(
        resolved.instances()[0].source(),
        PluginInstanceSource::HostDefaultConfiguredByRoot
    );
}

#[test]
fn explicit_instance_uses_host_configuration_without_becoming_a_host_default() {
    let descriptor = PluginDescriptor::new("example.editor", "1.0.0", "tools")
        .with_configuration_defaults(json!({"max_bytes": 1024}))
        .with_configuration_schema(json!({
            "type": "object",
            "required": ["root", "max_bytes"],
            "properties": {
                "root": {"type": "string"},
                "max_bytes": {"type": "integer"}
            },
            "additionalProperties": false
        }));
    let host = HostCatalog::new(
        [HostSlot::many("tools")],
        [HostPluginRelease::new(descriptor)],
        [],
    )
    .with_configurations([HostPluginConfiguration::new(
        "example.editor",
        "default",
        json!({"root": "."}),
    )]);

    let empty = resolve_plugin_root(&host, &PluginRootSnapshot::default()).unwrap();
    assert!(empty.instances().is_empty());

    let enabled = resolve_plugin_root(
        &host,
        &PluginRootSnapshot::new(
            [],
            [PluginRootInstance::new("example.editor", "default")],
            [],
        ),
    )
    .unwrap();
    assert_eq!(
        enabled.plan().plugin_instances()[0].configuration(),
        r#"{"max_bytes":1024,"root":"."}"#
    );
    assert_eq!(
        enabled.instances()[0].source(),
        PluginInstanceSource::PluginRoot
    );
}

#[test]
fn one_explicit_plugin_replaces_a_replaceable_default() {
    let root = PluginRootSnapshot::new(
        [model_descriptor("company.model")],
        [PluginRootInstance::new("company.model", "primary")],
        [],
    );

    let resolved = resolve_plugin_root(&host_with_default_model(true), &root).unwrap();

    assert!(
        resolved
            .instances()
            .iter()
            .any(|instance| instance.id() == &PluginInstanceId::new("company.model", "primary"))
    );
    assert!(resolved.instances().iter().all(|instance| {
        instance.id() != &PluginInstanceId::new("example.model.fixture", "default")
    }));
}

#[test]
fn explicit_plugin_cannot_replace_a_fixed_default() {
    let root = PluginRootSnapshot::new(
        [model_descriptor("company.model")],
        [PluginRootInstance::new("company.model", "primary")],
        [],
    );

    assert_eq!(
        resolve_plugin_root(&host_with_default_model(false), &root),
        Err(PluginRootResolutionError::ExplicitProviderDenied {
            slot: "model".to_owned(),
            instance: PluginInstanceId::new("company.model", "primary"),
        })
    );
}

#[test]
fn multiple_explicit_plugins_for_one_slot_fail_without_binding_escape_hatch() {
    let root = PluginRootSnapshot::new(
        [model_descriptor("company.a"), model_descriptor("company.b")],
        [
            PluginRootInstance::new("company.a", "default"),
            PluginRootInstance::new("company.b", "default"),
        ],
        [],
    );

    assert_eq!(
        resolve_plugin_root(&host_with_default_model(true), &root),
        Err(PluginRootResolutionError::AmbiguousSlot {
            slot: "model".to_owned(),
            instances: vec![
                "company.a/default".to_owned(),
                "company.b/default".to_owned()
            ],
        })
    );
}

#[test]
fn required_host_default_cannot_be_disabled() {
    let root = PluginRootSnapshot::new([], [], [PluginInstanceId::new("example.agent", "default")]);

    assert_eq!(
        resolve_plugin_root(&host_with_default_model(true), &root),
        Err(PluginRootResolutionError::RequiredInstanceDisabled(
            PluginInstanceId::new("example.agent", "default")
        ))
    );
}

#[test]
fn many_slot_collects_plugins_in_identity_order() {
    let host = HostCatalog::new([HostSlot::many("tools")], [], []);
    let root = PluginRootSnapshot::new(
        [
            PluginDescriptor::new("example.z", "1.0.0", "tools"),
            PluginDescriptor::new("example.a", "1.0.0", "tools"),
        ],
        [
            PluginRootInstance::new("example.z", "default"),
            PluginRootInstance::new("example.a", "default"),
        ],
        [],
    );

    let resolved = resolve_plugin_root(&host, &root).unwrap();

    assert_eq!(
        resolved
            .instances()
            .iter()
            .map(|instance| instance.id().to_string())
            .collect::<Vec<_>>(),
        ["example.a/default", "example.z/default"]
    );
}

#[test]
fn instance_identity_includes_plugin_id() {
    let host = HostCatalog::new([HostSlot::many("tools")], [], []);
    let root = PluginRootSnapshot::new(
        [
            PluginDescriptor::new("example.a", "1.0.0", "tools"),
            PluginDescriptor::new("example.b", "1.0.0", "tools"),
        ],
        [
            PluginRootInstance::new("example.a", "default"),
            PluginRootInstance::new("example.b", "default"),
        ],
        [],
    );

    let resolved = resolve_plugin_root(&host, &root).unwrap();

    assert_eq!(resolved.plan().plugin_instances().len(), 2);
    assert_ne!(
        resolved.plan().plugin_instances()[0].instance_key(),
        resolved.plan().plugin_instances()[1].instance_key()
    );
}

#[test]
fn ambiguous_capability_names_plugins_instead_of_requesting_a_binding_document() {
    let provider = |plugin_id: &str| {
        PluginDescriptor::new(plugin_id, "1.0.0", "models").with_capability(
            CapabilityEndpointPlan::new("example.model@1", "1.0.0", ["complete"]),
        )
    };
    let host = HostCatalog::new(
        [HostSlot::one("agent"), HostSlot::many("models")],
        [HostPluginRelease::new(agent_descriptor())],
        [HostDefaultPlugin::new("example.agent", "default")],
    );
    let root = PluginRootSnapshot::new(
        [provider("example.a"), provider("example.b")],
        [
            PluginRootInstance::new("example.a", "default"),
            PluginRootInstance::new("example.b", "default"),
        ],
        [],
    );

    assert_eq!(
        resolve_plugin_root(&host, &root),
        Err(PluginRootResolutionError::AmbiguousCapability {
            consumer: PluginInstanceId::new("example.agent", "default"),
            capability_id: "example.model@1".to_owned(),
            candidates: vec![
                PluginInstanceId::new("example.a", "default"),
                PluginInstanceId::new("example.b", "default"),
            ],
        })
    );
}

#[test]
fn missing_capability_names_the_affected_plugin() {
    let host = HostCatalog::new(
        [HostSlot::one("agent")],
        [HostPluginRelease::new(agent_descriptor())],
        [HostDefaultPlugin::new("example.agent", "default")],
    );

    assert_eq!(
        resolve_plugin_root(&host, &PluginRootSnapshot::default()),
        Err(PluginRootResolutionError::MissingCapability {
            consumer: PluginInstanceId::new("example.agent", "default"),
            capability_id: "example.model@1".to_owned(),
            descriptor_version: "1.0.0".to_owned(),
        })
    );
}

#[test]
fn duplicate_root_release_fails_instead_of_using_discovery_order() {
    let host = HostCatalog::new([HostSlot::many("models")], [], []);
    let root = PluginRootSnapshot::new(
        [
            model_descriptor("example.model"),
            model_descriptor("example.model"),
        ],
        [],
        [],
    );

    assert_eq!(
        resolve_plugin_root(&host, &root),
        Err(PluginRootResolutionError::DuplicatePluginRelease(
            "example.model".to_owned()
        ))
    );
}
