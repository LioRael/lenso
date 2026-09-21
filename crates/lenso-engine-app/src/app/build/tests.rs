use super::*;
use lenso_app_plan::{
    CapabilityRequirementPlan, ExecutionTargetCapability, authoring::PluginContract,
};
use lenso_plugin_bundle::{
    SourcePluginImplementation, SourcePluginReleaseBuild, build_source_plugin_release_bundle,
};

fn bundle(root: &std::path::Path) -> PathBuf {
    bundle_with_endpoint(
        root,
        lenso_app_plan::CapabilityEndpointPlan::new("company.notes", "1", ["get"]),
    )
}

fn bundle_with_endpoint(
    root: &std::path::Path,
    endpoint: lenso_app_plan::CapabilityEndpointPlan,
) -> PathBuf {
    let artifact = root.join("plugin.js");
    fs::write(
        &artifact,
        "throw new Error('must not execute during authoring');",
    )
    .unwrap();
    let bundle = root.join("bundle");
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract: PluginContract::new("company.store", "1.0.0", "store")
            .with_authoring_version(2)
            .with_capability(endpoint),
        implementations: vec![SourcePluginImplementation {
            id: "bun".into(),
            host_targets: vec!["*".into()],
            artifact,
            bundle_path: "implementations/bun/plugin.js".into(),
            media_type: "application/javascript".into(),
            target: "javascript-bun".into(),
            entrypoint: "plugin.js".into(),
            execution_class: ExecutionClassId::bun_child_process(),
            runtime_profile: lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE.into(),
            required_target_capabilities: vec![ExecutionTargetCapability::NativeProcess],
        }],
        output: bundle.clone(),
    })
    .unwrap();
    bundle
}

fn named_dependency_bundle(root: &std::path::Path) -> (PathBuf, PathBuf) {
    let store_artifact = root.join("store.js");
    let copy_artifact = root.join("copy.js");
    fs::write(&store_artifact, "export default {};").unwrap();
    fs::write(&copy_artifact, "export default {};").unwrap();
    let store = root.join("store-bundle");
    let copy = root.join("copy-bundle");
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract: PluginContract::new("company.store", "1.0.0", "store")
            .with_authoring_version(2)
            .with_capability(lenso_app_plan::CapabilityEndpointPlan::new(
                "company.storage@1",
                "1.0.0",
                ["get"],
            )),
        implementations: vec![SourcePluginImplementation {
            id: "bun".into(),
            host_targets: vec!["*".into()],
            artifact: store_artifact,
            bundle_path: "implementations/bun/plugin.js".into(),
            media_type: "application/javascript".into(),
            target: "javascript-bun".into(),
            entrypoint: "plugin.js".into(),
            execution_class: ExecutionClassId::bun_child_process(),
            runtime_profile: lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE.into(),
            required_target_capabilities: vec![ExecutionTargetCapability::NativeProcess],
        }],
        output: store.clone(),
    })
    .unwrap();
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract: PluginContract::new("company.copy", "1.0.0", "copy")
            .with_authoring_version(2)
            .with_capability(lenso_app_plan::CapabilityEndpointPlan::new(
                "company.copy@1",
                "1.0.0",
                ["copy"],
            ))
            .with_requirement(
                CapabilityRequirementPlan::one("company.storage@1", "1.0.0")
                    .with_requirement_id("source"),
            ),
        implementations: vec![SourcePluginImplementation {
            id: "bun".into(),
            host_targets: vec!["*".into()],
            artifact: copy_artifact,
            bundle_path: "implementations/bun/plugin.js".into(),
            media_type: "application/javascript".into(),
            target: "javascript-bun".into(),
            entrypoint: "plugin.js".into(),
            execution_class: ExecutionClassId::bun_child_process(),
            runtime_profile: lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE.into(),
            required_target_capabilities: vec![ExecutionTargetCapability::NativeProcess],
        }],
        output: copy.clone(),
    })
    .unwrap();
    (store, copy)
}

fn target_fallback_bundle(root: &std::path::Path) -> PathBuf {
    let rejected = root.join("rejected.js");
    let selected = root.join("selected.js");
    fs::write(&rejected, "export default {};").unwrap();
    fs::write(&selected, "export default {};").unwrap();
    let bundle = root.join("target-fallback-bundle");
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract: PluginContract::new("company.target", "1.0.0", "target")
            .with_authoring_version(2)
            .with_capability(lenso_app_plan::CapabilityEndpointPlan::new(
                "company.target@1",
                "1.0.0",
                ["get"],
            )),
        implementations: vec![
            SourcePluginImplementation {
                id: "wrong-target".into(),
                host_targets: vec!["unavailable-target".into()],
                artifact: rejected,
                bundle_path: "implementations/wrong-target/plugin.js".into(),
                media_type: "application/javascript".into(),
                target: "javascript-bun".into(),
                entrypoint: "plugin.js".into(),
                execution_class: ExecutionClassId::bun_child_process(),
                runtime_profile: lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE.into(),
                required_target_capabilities: vec![ExecutionTargetCapability::NativeProcess],
            },
            SourcePluginImplementation {
                id: "selected-target".into(),
                host_targets: vec!["javascript-bun".into()],
                artifact: selected,
                bundle_path: "implementations/selected-target/plugin.js".into(),
                media_type: "application/javascript".into(),
                target: "javascript-bun".into(),
                entrypoint: "plugin.js".into(),
                execution_class: ExecutionClassId::bun_child_process(),
                runtime_profile: lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE.into(),
                required_target_capabilities: vec![ExecutionTargetCapability::NativeProcess],
            },
        ],
        output: bundle.clone(),
    })
    .unwrap();
    bundle
}

fn workers_required_bundle(root: &std::path::Path) -> PathBuf {
    let artifact = root.join("workers-required.js");
    fs::write(&artifact, "export default {};\n").unwrap();
    let bundle = root.join("workers-required-bundle");
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract: PluginContract::new("company.workers-required", "1.0.0", "workers required")
            .with_authoring_version(2)
            .with_capability(lenso_app_plan::CapabilityEndpointPlan::new(
                "company.workers-required@1",
                "1.0.0",
                ["get"],
            )),
        implementations: vec![SourcePluginImplementation {
            id: "bun".into(),
            host_targets: vec!["*".into()],
            artifact,
            bundle_path: "implementations/bun/plugin.js".into(),
            media_type: "application/javascript".into(),
            target: "javascript-bun".into(),
            entrypoint: "plugin.js".into(),
            execution_class: ExecutionClassId::bun_child_process(),
            runtime_profile: lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE.into(),
            required_target_capabilities: vec![
                ExecutionTargetCapability::NativeProcess,
                ExecutionTargetCapability::Workers,
            ],
        }],
        output: bundle.clone(),
    })
    .unwrap();
    bundle
}

#[test]
fn host_build_rejects_stream_profile_and_does_not_replace_a_racing_output() {
    let root = tempfile::tempdir().unwrap();
    let bundle = bundle_with_endpoint(
        root.path(),
        lenso_app_plan::CapabilityEndpointPlan::new("company.notes", "1", ["get"])
            .with_stream_operation("get"),
    );
    let args = HostBuildArgs {
        source: root.path().join("app.ts"),
        target: "javascript-bun".into(),
        out: root.path().join("output"),
    };
    let error = materialize(declaration(bundle), &args).unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("missing_target_capabilities")
            || message.contains("Request Capabilities only")
    );
    assert!(!args.out.exists());
    let stage = root.path().join("stage");
    fs::create_dir(&stage).unwrap();
    fs::create_dir(&args.out).unwrap();
    assert!(publish_new_output(&stage, &args.out).is_err());
    assert!(stage.is_dir());
    assert!(args.out.is_dir());
}

#[test]
fn host_build_rejects_a_bun_implementation_that_requires_workers() {
    let root = tempfile::tempdir().unwrap();
    let bundle = workers_required_bundle(root.path());
    let args = HostBuildArgs {
        source: root.path().join("app.ts"),
        target: "javascript-bun".into(),
        out: root.path().join("output"),
    };
    let error = materialize(declaration(bundle), &args).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("missing_target_capabilities"));
    assert!(message.contains("workers"));
    assert!(!args.out.exists());
}

fn declaration(bundle: PathBuf) -> Declaration {
    Declaration {
        id: "company.app".into(),
        slots: vec![],
        dependencies: vec![],
        plugins: vec![Instance::Bare(Reference {
            bundle,
            execution: Execution::Bun,
            source: "app.ts:3:1".into(),
        })],
    }
}

#[test]
fn host_build_verifies_bundle_without_executing_and_reuses_management_admission() {
    let root = tempfile::tempdir().unwrap();
    let bundle = bundle(root.path());
    let args = HostBuildArgs {
        source: root.path().join("app.ts"),
        target: "aarch64-apple-darwin".into(),
        out: root.path().join("output"),
    };
    materialize(declaration(bundle.clone()), &args).unwrap();
    assert_eq!(
        lenso_app_authoring::load_resolved_app(&args.out)
            .unwrap()
            .instances()
            .len(),
        1
    );
    assert!(args.out.join("bundles/0.lenso-plugin").is_file());
    assert!(!args.out.join(".lenso/host").exists());
    let error = lenso_app_authoring::add_bundle(&args.out, &bundle).unwrap_err();
    assert!(format!("{error:#}").contains("admission denied"));
    assert!(
        !args
            .out
            .join("plugins/company.store/plugin.lenso-plugin")
            .exists()
    );
    let before = fs::read(args.out.join(".lenso/host-build.json")).unwrap();
    assert!(materialize(declaration(bundle), &args).is_err());
    assert_eq!(
        fs::read(args.out.join(".lenso/host-build.json")).unwrap(),
        before
    );
}

#[test]
fn host_build_rejects_corruption_and_unavailable_implementation_before_publication() {
    let root = tempfile::tempdir().unwrap();
    let bundle = bundle(root.path());
    let args = HostBuildArgs {
        source: root.path().join("app.ts"),
        target: "x86_64-unknown-linux-gnu".into(),
        out: root.path().join("output"),
    };
    let mut unavailable = declaration(bundle.clone());
    let Instance::Bare(reference) = &mut unavailable.plugins[0] else {
        unreachable!()
    };
    reference.execution = Execution::Process;
    assert!(materialize(unavailable, &args).is_err());
    assert!(!args.out.exists());
    fs::write(bundle.join("implementations/bun/plugin.js"), "tampered").unwrap();
    assert!(materialize(declaration(bundle), &args).is_err());
    assert!(!args.out.exists());
}

#[test]
fn host_build_persists_runtime_target_rejection_for_app_explain() {
    let root = tempfile::tempdir().unwrap();
    let args = HostBuildArgs {
        source: root.path().join("app.ts"),
        target: "javascript-bun".into(),
        out: root.path().join("output"),
    };
    materialize(declaration(target_fallback_bundle(root.path())), &args).unwrap();

    let report = crate::app::explain::report(&args.out).unwrap();
    assert_eq!(report["schema"], "lenso.app-explain.v1");
    assert_eq!(report["unmet_consumer_requirements"], serde_json::json!([]));
    assert_eq!(
        report["engine_execution"]["availability"]["reason"],
        "no_active_engine_session"
    );
    assert_eq!(
        report["implementation_selection"][0]["selected"]["implementation_id"],
        "selected-target"
    );
    assert_eq!(
        report["implementation_selection"][0]["rejected"][0]["reason"]["kind"],
        "host_target_mismatch"
    );
    assert_eq!(
        report["target_capability_profiles"][0]["target_profile"],
        lenso_bun_adapter::BUN_AUTHORING_RUNTIME_PROFILE
    );
}

#[test]
fn host_build_materializes_only_a_host_authorized_dependency_choice() {
    let root = tempfile::tempdir().unwrap();
    let (store, copy) = named_dependency_bundle(root.path());
    let reference = |bundle: PathBuf, source: &str| Reference {
        bundle,
        execution: Execution::Bun,
        source: source.into(),
    };
    let identity = |plugin: &str, instance: &str| InstanceIdentity {
        plugin: plugin.into(),
        instance: instance.into(),
    };
    let declaration = Declaration {
        id: "company.app".into(),
        plugins: vec![
            Instance::Named(NamedInstance {
                plugin: reference(store.clone(), "app.ts:4:1"),
                instance: "source".into(),
                configuration: empty_configuration(),
            }),
            Instance::Named(NamedInstance {
                plugin: reference(store, "app.ts:5:1"),
                instance: "destination".into(),
                configuration: empty_configuration(),
            }),
            Instance::Bare(reference(copy, "app.ts:6:1")),
        ],
        slots: vec![Slot {
            id: "store".into(),
            cardinality: Cardinality::Many,
            replaceable: false,
            max_instances: Some(2),
            allow: vec![],
            configuration_schema: None,
        }],
        dependencies: vec![Dependency {
            consumer: identity("company.copy", "default"),
            requirement: "source".into(),
            allow: vec![
                identity("company.store", "source"),
                identity("company.store", "destination"),
            ],
            default: Some(identity("company.store", "source")),
        }],
    };
    let args = HostBuildArgs {
        source: root.path().join("app.ts"),
        target: "javascript-bun".into(),
        out: root.path().join("output"),
    };

    materialize(declaration, &args).unwrap();

    let choices: lenso_app_authoring::DependencySelectionsDocument =
        serde_json::from_slice(&fs::read(args.out.join("plugins/.dependencies.json")).unwrap())
            .unwrap();
    assert_eq!(choices.choices.len(), 1);
    assert_eq!(choices.choices[0].requirement_id, "source");
    assert_eq!(
        choices.choices[0].provider.as_ref().unwrap().instance_key(),
        "source"
    );
    assert_eq!(
        lenso_app_authoring::load_resolved_app(&args.out)
            .unwrap()
            .plan()
            .capability_bindings()
            .len(),
        1
    );
    let report = crate::app::explain::report(&args.out).unwrap();
    let demand = report["consumer_requirements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|demand| {
            demand["consumer_instance"] == "company.copy/default"
                && demand["requirement_id"] == "source"
        })
        .unwrap();
    assert_eq!(demand["capability_id"], "company.storage@1");
    assert_eq!(
        demand["selected_providers"][0]["provider_instance"],
        "company.store/source"
    );
    assert_eq!(
        demand["non_selected_host_provider_candidates"][0]["provider_instance"],
        "company.store/destination"
    );
}
