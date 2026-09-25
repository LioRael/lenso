use super::*;
use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan, ExecutionTargetCapability,
    authoring::PluginContract,
};
use lenso_plugin_bundle::{
    SourcePluginImplementation, SourcePluginReleaseBuild, build_source_plugin_release_bundle,
};

#[test]
fn pinned_runtime_rejects_a_different_module() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    let output = temp.path().join("output");
    fs::create_dir(&package).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(package.join("package.json"), r#"{"name":"@lenso/workers-runtime","version":"0.1.4","exports":{"./component-requests":"./component-requests.mjs"}}"#).unwrap();
    fs::write(
        package.join("component-requests.mjs"),
        "export const unsafe = true;",
    )
    .unwrap();
    let error = copy_pinned_runtime(&package, &output).unwrap_err();
    assert!(error.to_string().contains("differs from pinned"));
    assert!(!output.join("component-requests.mjs").exists());
}

fn test_bundle(
    output: PathBuf,
    artifact: &Path,
    contract: PluginContract,
    host_target: &str,
    capabilities: Vec<ExecutionTargetCapability>,
) {
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract,
        implementations: vec![SourcePluginImplementation {
            id: host_target.into(),
            host_targets: vec![host_target.into()],
            artifact: artifact.to_path_buf(),
            bundle_path: "implementations/guest.component.wasm".into(),
            media_type: "application/wasm".into(),
            target: "wasm32-unknown-unknown".into(),
            entrypoint: "plugin".into(),
            execution_class: ExecutionClassId::new("lenso.wasm-component@1"),
            runtime_profile: "lenso.wasm-component@1".into(),
            required_target_capabilities: capabilities,
        }],
        output,
    })
    .unwrap();
}

fn leb128(mut value: usize) -> Vec<u8> {
    let mut encoded = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        encoded.push(byte);
        if value == 0 {
            return encoded;
        }
    }
}

fn write_descriptor_component(path: &Path, contract: &PluginContract) {
    write_descriptor_component_with_digest(path, contract, None);
}

fn write_descriptor_component_with_digest(
    path: &Path,
    contract: &PluginContract,
    digest: Option<&str>,
) {
    let capabilities = contract
        .provided_capabilities()
        .iter()
        .map(|capability| {
            let mut entry = json!({
                "capability_id": capability.capability_id(),
                "descriptor_version": capability.descriptor_version(),
                "request_operations": capability.operations(),
            });
            if let Some(digest) = digest {
                entry["descriptor_digest"] = json!(digest);
            }
            entry
        })
        .collect::<Vec<_>>();
    let requirements = contract
        .required_capabilities()
        .iter()
        .map(|requirement| {
            json!({
                "requirement_id": requirement.requirement_id(),
                "capability_id": requirement.capability_id(),
                "descriptor_version": requirement.descriptor_version(),
                "cardinality": "one",
            })
        })
        .collect::<Vec<_>>();
    let mut descriptor = if requirements.is_empty() {
        json!({ "abi": "lenso.json-request@1", "capabilities": capabilities })
    } else {
        json!({ "abi": "lenso.json-host-imports@2", "capabilities": capabilities, "required_capabilities": requirements })
    };
    descriptor.sort_all_objects();
    let name = lenso_plugin_bundle::PLUGIN_DESCRIPTOR_SECTION.as_bytes();
    let mut section = leb128(name.len());
    section.extend(name);
    section.extend(serde_json::to_vec(&descriptor).unwrap());
    let mut component = b"\0asm\x0d\0\x01\0".to_vec();
    component.push(0);
    component.extend(leb128(section.len()));
    component.extend(section);
    fs::write(path, component).unwrap();
}

#[test]
fn source_descriptor_digest_is_trusted_and_authoring_version_specific() {
    let root = tempfile::tempdir().unwrap();
    let component = root.path().join("guest.component.wasm");
    let contract = PluginContract::new("local.endpoint", "1.0.0", "web")
        .with_authoring_version(2)
        .with_capability(CapabilityEndpointPlan::new(
            lenso_capability_http_endpoint::CAPABILITY_ID,
            lenso_capability_http_endpoint::DESCRIPTOR_VERSION,
            ["describe", "handle"],
        ));
    write_descriptor_component(&component, &contract);
    let error = source_descriptor_evidence(&component, 2).err().unwrap();
    assert!(error.to_string().contains("trusted Descriptor digest"));
    write_descriptor_component_with_digest(
        &component,
        &contract,
        Some(&format!("sha256:{}", "0".repeat(64))),
    );
    assert!(source_descriptor_evidence(&component, 2).is_err());
    write_descriptor_component_with_digest(
        &component,
        &contract,
        Some(lenso_capability_http_endpoint::DESCRIPTOR_DIGEST),
    );
    let evidence = source_descriptor_evidence(&component, 2).unwrap();
    assert_eq!(
        evidence.expected_digests.unwrap()[lenso_capability_http_endpoint::CAPABILITY_ID],
        lenso_capability_http_endpoint::DESCRIPTOR_DIGEST
    );
    assert!(source_descriptor_evidence(&component, 1).is_err());
    write_descriptor_component(&component, &contract);
    assert!(
        source_descriptor_evidence(&component, 1)
            .unwrap()
            .expected_digests
            .is_none()
    );
}

#[test]
fn native_only_bundle_cannot_fall_back_to_workers() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir(&app).unwrap();
    let artifact = root.path().join("dummy.wasm");
    let contract = PluginContract::new("local.native-only", "1.0.0", "web")
        .with_authoring_version(2)
        .with_capability(CapabilityEndpointPlan::new(
            "lenso.http.endpoint@1",
            "1.1.0",
            ["describe", "handle"],
        ));
    write_descriptor_component(&artifact, &contract);
    test_bundle(
        app.join("native-only"),
        &artifact,
        contract,
        "aarch64-apple-darwin",
        vec![
            ExecutionTargetCapability::Request,
            ExecutionTargetCapability::WasmComponent,
        ],
    );
    let output = root.path().join("dist-workers");
    let error = build(BuildArgs {
        root: root.path().to_path_buf(),
        out: output.clone(),
        workers_runtime: PathBuf::from("unused"),
        jco: PathBuf::from("unused"),
    })
    .unwrap_err();
    let explanation = format!("{error:#}");
    assert!(explanation.contains("local.native-only"), "{explanation}");
    assert!(
        explanation.contains("Workers target admission"),
        "{explanation}"
    );
    assert!(!output.exists());
}

#[test]
fn dependency_closure_reports_the_consumer_requirement_and_provider() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir(&app).unwrap();
    let artifact = root.path().join("dummy.wasm");
    let profile = vec![
        ExecutionTargetCapability::Request,
        ExecutionTargetCapability::WasmComponent,
        ExecutionTargetCapability::Workers,
    ];
    let consumer = PluginContract::new("local.consumer", "1.0.0", "web")
        .with_authoring_version(2)
        .with_capability(CapabilityEndpointPlan::new(
            "lenso.http.endpoint@1",
            "1.1.0",
            ["describe", "handle"],
        ))
        .with_requirement(
            CapabilityRequirementPlan::one("local.storage@1", "1.0.0")
                .with_requirement_id("storage"),
        );
    write_descriptor_component(&artifact, &consumer);
    test_bundle(
        app.join("consumer"),
        &artifact,
        consumer,
        HOST_TARGET,
        profile.clone(),
    );
    let provider = PluginContract::new("local.provider", "1.0.0", "web")
        .with_authoring_version(2)
        .with_capability(CapabilityEndpointPlan::new(
            "local.storage@1",
            "1.0.0",
            ["get"],
        ));
    write_descriptor_component(&artifact, &provider);
    test_bundle(
        app.join("provider"),
        &artifact,
        provider,
        HOST_TARGET,
        profile,
    );
    let output = root.path().join("dist-workers");
    let error = build(BuildArgs {
        root: root.path().to_path_buf(),
        out: output.clone(),
        workers_runtime: PathBuf::from("unused"),
        jco: PathBuf::from("unused"),
    })
    .unwrap_err();
    let explanation = format!("{error:#}");
    assert!(
        explanation.contains("local.consumer/default --storage--> local.provider/default"),
        "{explanation}"
    );
    assert!(!output.exists());
}

/// Run with exact local tools and the previously compiled portable WIT Guest:
/// LENSO_A8_COMPONENT=... LENSO_A8_RUNTIME=... LENSO_A8_JCO=... cargo test ... --ignored
#[test]
#[ignore = "requires an existing exact portable Component and Jco 1.35.0"]
fn verified_bundle_builds_a_self_contained_workers_app() {
    let component = PathBuf::from(std::env::var_os("LENSO_A8_COMPONENT").expect("Component path"));
    let workers_runtime =
        PathBuf::from(std::env::var_os("LENSO_A8_RUNTIME").expect("JS package path"));
    let jco = PathBuf::from(std::env::var_os("LENSO_A8_JCO").expect("Jco path"));
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir(&app).unwrap();
    let bundle = app.join("portable-http");
    let contract = PluginContract::new("local.portable-http", "1.0.0", "web")
        .with_authoring_version(2)
        .with_capability(CapabilityEndpointPlan::new(
            lenso_capability_http_endpoint::CAPABILITY_ID,
            lenso_capability_http_endpoint::DESCRIPTOR_VERSION,
            ["describe", "handle"],
        ));
    let variant = |id: &str, host_target: String, capabilities: Vec<ExecutionTargetCapability>| {
        SourcePluginImplementation {
            id: id.into(),
            host_targets: vec![host_target],
            artifact: component.clone(),
            bundle_path: format!("implementations/{id}/guest.component.wasm"),
            media_type: "application/wasm".into(),
            target: "wasm32-unknown-unknown".into(),
            entrypoint: "plugin".into(),
            execution_class: ExecutionClassId::new("lenso.wasm-component@1"),
            runtime_profile: "lenso.wasm-component@1".into(),
            required_target_capabilities: capabilities,
        }
    };
    build_source_plugin_release_bundle(&SourcePluginReleaseBuild {
        contract,
        implementations: vec![
            variant(
                "native",
                lenso_app_authoring::native_host_target().into(),
                vec![
                    ExecutionTargetCapability::Request,
                    ExecutionTargetCapability::WasmComponent,
                ],
            ),
            variant(
                "workers",
                HOST_TARGET.into(),
                vec![
                    ExecutionTargetCapability::Request,
                    ExecutionTargetCapability::WasmComponent,
                    ExecutionTargetCapability::Workers,
                ],
            ),
        ],
        output: bundle,
    })
    .unwrap();
    let discovered = discover(root.path()).unwrap();
    assert_eq!(discovered.candidates.len(), 1);
    assert_eq!(discovered.candidates[0].format, "bundle");
    let before = super::super::super::local_host::input_digest(root.path()).unwrap();
    let output = root.path().join("dist-workers");
    build(BuildArgs {
        root: root.path().to_path_buf(),
        out: output.clone(),
        workers_runtime,
        jco,
    })
    .unwrap();
    let after = super::super::super::local_host::input_digest(root.path()).unwrap();
    assert_eq!(
        before, after,
        "generated Workers output must not enter App source digest"
    );
    let receipt: Value =
        serde_json::from_slice(&fs::read(output.join("workers-build.json")).unwrap()).unwrap();
    assert_eq!(receipt["source_digest"], before);
    assert_eq!(receipt["target"], HOST_TARGET);
    assert_eq!(receipt["plugin_id"], "local.portable-http");
    assert_eq!(receipt["implementation_id"], "workers");
    assert_eq!(
        receipt["expected_descriptor_digests"][lenso_capability_http_endpoint::CAPABILITY_ID],
        lenso_capability_http_endpoint::DESCRIPTOR_DIGEST
    );
    for file in [
        "worker.mjs",
        "workers-http.mjs",
        "component-requests.mjs",
        "descriptor-digests.mjs",
        "plan.mjs",
        "guest.component.wasm",
        "guest.core.wasm",
        "guest.js",
        "wrangler.jsonc",
    ] {
        assert!(output.join(file).is_file(), "missing {file}");
    }
    // The bundled runtime is a copied, pinned input, never a sibling worktree import.
    assert_eq!(
        super::super::super::local_host::digest(&output.join("component-requests.mjs")).unwrap(),
        format!("sha256:{RUNTIME_MODULE_SHA256}")
    );
    if std::env::var_os("LENSO_A8_KEEP_OUTPUT").as_deref() == Some(std::ffi::OsStr::new("1")) {
        eprintln!("retained exact Workers App: {}", root.path().display());
        let _ = root.keep();
    }
}
