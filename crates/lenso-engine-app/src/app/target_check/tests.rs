use super::*;
use lenso_app_authoring::host_authoring::{GeneratedHostBuild, LocalPluginInput};
use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan, ExecutionClassId,
    authoring::PluginDescriptor,
};

fn resolved(descriptor: PluginDescriptor) -> ResolvedApp {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("plugins")).unwrap();
    GeneratedHostBuild::lower_local(
        "example.app",
        vec![LocalPluginInput {
            descriptor,
            manifest_digest: format!("sha256:{}", "a".repeat(64)),
            app_owned: true,
            source: "fixture".into(),
        }],
    )
    .unwrap()
    .with_local_root(root.path())
    .unwrap()
    .1
}

fn declarations() -> BTreeMap<String, Declaration> {
    BTreeMap::from([(
        "example.store".into(),
        serde_json::from_value(json!({"combinations":[
        {"environment":"native","execution":"lenso.native-rust@1","resources":{"db":"postgresql"}},
        {"environment":"workers","execution":"lenso.native-rust@1","resources":{"db":"d1"}}
    ],"evidence":["external-receipt-reference"]}))
        .unwrap(),
    )])
}

fn grants(target: &str, implementation: &str) -> Grants {
    serde_json::from_value(json!({"schema":"lenso.host-facilities.v1","instances":{
        "example.store/default":{"db":{"configuration":{"implementation":implementation,"reference":"DATABASE_URL"},
            "binding":if target == "workers" {json!("DB")} else {Value::Null}}}
    }})).unwrap()
}

#[test]
fn exact_native_pg_workers_d1_do_not_form_a_cartesian_product() {
    let resolved = resolved(PluginDescriptor::new("example.store", "1.0.0", "store"));
    for (target, implementation) in [("native", "postgresql"), ("workers", "d1")] {
        let report = admit(
            &resolved,
            target,
            &declarations(),
            &grants(target, implementation),
        )
        .unwrap();
        assert_eq!(report[0]["qualification"], "not_assessed");
        assert_eq!(report[0]["support"], "declared");
        assert_eq!(
            report[0]["evidence_references"][0],
            "external-receipt-reference"
        );
    }
    for (target, implementation) in [("native", "d1"), ("workers", "postgresql")] {
        let error = admit(
            &resolved,
            target,
            &declarations(),
            &grants(target, implementation),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("example.store/default")
                && error.contains("required exact support")
                && error.contains("available declared combinations"),
            "{error}"
        );
    }
}

#[test]
fn named_resources_missing_and_extra_grants_fail_closed() {
    let resolved = resolved(PluginDescriptor::new("example.store", "1.0.0", "store"));
    let mut grants = grants("workers", "d1");
    grants
        .instances
        .get_mut("example.store/default")
        .unwrap()
        .insert(
            "other_db".into(),
            json!({"configuration":{"implementation":"d1"},"binding":"OTHER_DB"}),
        );
    assert!(admit(&resolved, "workers", &declarations(), &grants).is_err());
    grants
        .instances
        .get_mut("example.store/default")
        .unwrap()
        .remove("db");
    assert!(admit(&resolved, "workers", &declarations(), &grants).is_err());
    grants
        .instances
        .insert("example.store/absent".into(), BTreeMap::new());
    assert!(
        admit(&resolved, "workers", &declarations(), &grants)
            .unwrap_err()
            .to_string()
            .contains("required selected Instance")
    );
}

#[test]
fn specialized_native_plugin_needs_no_matrix_or_resources() {
    let resolved = resolved(PluginDescriptor::new("example.store", "1.0.0", "store"));
    let grants = Grants {
        schema: "lenso.host-facilities.v1".into(),
        instances: BTreeMap::new(),
    };
    assert!(admit(&resolved, "native", &BTreeMap::new(), &grants).is_ok());
    assert!(admit(&resolved, "workers", &BTreeMap::new(), &grants).is_err());
}

#[test]
fn axis_lists_are_rejected_instead_of_expanded() {
    assert!(
        serde_json::from_value::<Declaration>(
            json!({"environments":["native","workers"],"storage":["postgresql","d1"]})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Support>(
            json!({"environment":"native","execution":"lenso.native-rust@1","storage":"postgresql"})
        )
        .is_err()
    );
}

#[test]
fn existing_resolver_checks_missing_capability_and_consumer_target_closure() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("plugins")).unwrap();
    let consumer = PluginDescriptor::new("example.consumer", "1.0.0", "tools")
        .with_execution_class(ExecutionClassId::new("lenso.process@1"))
        .with_authoring(2, lenso_process_adapter::RUNTIME_PROFILE_V2)
        .with_requirement(
            CapabilityRequirementPlan::one("example.store@1", "1").with_requirement_id("store"),
        );
    let input = |descriptor| LocalPluginInput {
        descriptor,
        manifest_digest: format!("sha256:{}", "a".repeat(64)),
        app_owned: true,
        source: "fixture".into(),
    };
    let build =
        GeneratedHostBuild::lower_local("example.app", vec![input(consumer.clone())]).unwrap();
    assert!(build.with_local_root(root.path()).is_err());
    let provider = PluginDescriptor::new("example.store", "1.0.0", "store").with_capability(
        CapabilityEndpointPlan::new("example.store@1", "1", ["open"])
            .with_operation_kind("open", CapabilityOperationKind::Stream),
    );
    let (_, resolved) =
        GeneratedHostBuild::lower_local("example.app", vec![input(consumer), input(provider)])
            .unwrap()
            .with_local_root(root.path())
            .unwrap();
    let error = admit(
        &resolved,
        "native",
        &BTreeMap::new(),
        &Grants {
            schema: "lenso.host-facilities.v1".into(),
            instances: BTreeMap::new(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("example.consumer/default")
            && error.contains("Stream")
            && error.contains("available"),
        "{error}"
    );
}

#[test]
fn secret_and_resource_references_are_never_dereferenced() {
    let resolved = resolved(PluginDescriptor::new("example.store", "1.0.0", "store"));
    let mut grants = grants("native", "postgresql");
    grants
        .instances
        .get_mut("example.store/default")
        .unwrap()
        .get_mut("db")
        .unwrap()["configuration"]["reference"] = json!("SECRET_THAT_DOES_NOT_EXIST");
    assert!(admit(&resolved, "native", &declarations(), &grants).is_ok());
    let mut grants = grants;
    grants
        .instances
        .get_mut("example.store/default")
        .unwrap()
        .get_mut("db")
        .unwrap()["configuration"]["reference"] = json!("postgres://password@example.com");
    assert!(admit(&resolved, "native", &declarations(), &grants).is_err());
}

#[test]
fn one_implementation_checks_each_instances_resources_independently() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("plugins/example.store")).unwrap();
    fs::write(
        root.path().join("plugins/example.store/second.toml"),
        "label = 'second'\n",
    )
    .unwrap();
    let (_, resolved) = GeneratedHostBuild::lower_local("example.app", vec![LocalPluginInput {
        descriptor: PluginDescriptor::new("example.store", "1.0.0", "store")
            .with_configuration_schema(json!({"type":"object","properties":{"label":{"type":"string"}},"additionalProperties":false})),
        manifest_digest: format!("sha256:{}", "a".repeat(64)), app_owned: true, source: "fixture".into(),
    }]).unwrap().with_local_root(root.path()).unwrap();
    assert_eq!(resolved.instances().len(), 2);
    let mut grants = grants("native", "postgresql");
    let first = grants.instances["example.store/default"].clone();
    grants
        .instances
        .insert("example.store/second".into(), first);
    let reports = admit(&resolved, "native", &declarations(), &grants).unwrap();
    assert_eq!(reports.len(), 2);
    grants
        .instances
        .get_mut("example.store/second")
        .unwrap()
        .get_mut("db")
        .unwrap()["configuration"]["implementation"] = json!("d1");
    let error = admit(&resolved, "native", &declarations(), &grants)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("example.store/second") && error.contains("required exact support"),
        "{error}"
    );
}

#[test]
fn source_check_reuses_generated_contract_and_rejects_changed_source() {
    let source = tempfile::tempdir().unwrap();
    let built = tempfile::tempdir().unwrap();
    fs::create_dir_all(source.path().join("app/store/src")).unwrap();
    fs::create_dir(source.path().join("plugins")).unwrap();
    fs::create_dir(built.path().join(".lenso")).unwrap();
    fs::write(source.path().join("app/store/Cargo.toml"), "[package]\nname='fixture'\nversion='1.0.0'\n[package.metadata.lenso]\nplugin-id='example.store'\nroot-slot='store'\n[package.metadata.lenso-cli]\nruntime='native-linked'\n").unwrap();
    fs::write(
        source.path().join("app/store/src/lib.rs"),
        "pub struct Store;\n",
    )
    .unwrap();
    let candidates = lenso_app_authoring::discovery::discover(source.path())
        .unwrap()
        .candidates;
    let authority = GeneratedHostBuild::lower_local(
        "example.app",
        vec![LocalPluginInput {
            descriptor: PluginDescriptor::new("example.store", "1.0.0", "store"),
            manifest_digest: format!("sha256:{}", "a".repeat(64)),
            app_owned: true,
            source: "generated fixture".into(),
        }],
    )
    .unwrap();
    fs::write(
        built.path().join(".lenso/host-build.json"),
        serde_json::to_vec(&authority).unwrap(),
    )
    .unwrap();
    let digest = super::super::local_host::input_digest(&candidates[0].project).unwrap();
    fs::write(built.path().join("local-sources.json"), serde_json::to_vec(&json!({
        "schema":"lenso.local-sources.v2","sources":candidates,"source_digests":{"example.store":digest}
    })).unwrap()).unwrap();
    assert_eq!(
        inspect(source.path(), "native", Some(built.path()), None).unwrap()["status"],
        "passed"
    );
    fs::write(
        source.path().join("app/store/src/lib.rs"),
        "pub struct Changed;\n",
    )
    .unwrap();
    let error = inspect(source.path(), "native", Some(built.path()), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("example.store/default") && error.contains("stale source"),
        "{error}"
    );
    assert!(!source.path().join("target").exists());
    assert!(!source.path().join(".lenso").exists());
}
