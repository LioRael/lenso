use super::scaffold::{
    LENSO_FRAMEWORK_REVISION, bun_plugin_scaffold, create, multi_plugin_scaffold, plugin_scaffold,
    process_plugin_scaffold, web_plugin_scaffold,
};
use super::*;

#[test]
fn bun_descriptor_lowers_named_dependencies_into_the_plugin_contract() {
    let descriptor = parse_descriptor_bytes(
        br#"{
            "abi":"lenso.json-host-imports@2",
            "configuration_schema":{"type":"object","required":["prefix"]},
            "capabilities":[{
                "capability_id":"company.notes@1",
                "descriptor_version":"1.0.0",
                "request_operations":["list"]
            }],
            "required_capabilities":[{
                "requirement_id":"store",
                "capability_id":"company.notes-store@1",
                "descriptor_version":"1.0.0",
                "cardinality":"one"
            }]
        }"#,
    )
    .unwrap();
    let package = BunPackage {
        version: "1.0.0".to_owned(),
        metadata: BunPackageMetadata {
            source: None,
            plugin_id: "company.notes".to_owned(),
            root_slot: "notes".to_owned(),
            runtime: "bun".to_owned(),
        },
    };

    let contract = contract_from_bun_descriptor(&package, &descriptor).unwrap();
    let requirement = &contract.required_capabilities()[0];

    assert_eq!(
        contract.configuration_schema(),
        Some(&serde_json::json!({"type":"object","required":["prefix"]}))
    );
    assert_eq!(requirement.requirement_id(), "store");
    assert_eq!(requirement.capability_id(), "company.notes-store@1");
}

#[test]
fn bun_descriptor_preserves_guest_import_codec_evidence() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let descriptor = parse_descriptor_bytes(
        serde_json::to_vec(&serde_json::json!({
            "abi":"lenso.json-host-imports@2",
            "capabilities":[],
            "required_capabilities":[{
                "requirement_id":"jobs",
                "capability_id":"lenso.jobs@1",
                "descriptor_version":"1.0.0",
                "descriptor_digest":digest,
                "request_operations":["enqueue", "claim"],
                "stream_operations":[],
                "event_operations":[],
                "cardinality":"optional"
            }]
        }))
        .unwrap()
        .as_slice(),
    )
    .unwrap();

    let requirement = &descriptor.required_capabilities[0];
    assert_eq!(
        requirement.descriptor_digest.as_deref(),
        Some(digest.as_str())
    );
    assert_eq!(requirement.request_operations, ["enqueue", "claim"]);
    assert!(requirement.stream_operations.is_empty());
    assert!(requirement.event_operations.is_empty());
}

#[test]
fn bun_descriptor_accepts_a_providerless_lifecycle_plugin() {
    let descriptor = parse_descriptor_bytes(
        br#"{
            "abi":"lenso.json-request@1",
            "capabilities":[],
            "required_capabilities":[{
                "requirement_id":"store",
                "capability_id":"company.notes-store@1",
                "descriptor_version":"1.0.0",
                "cardinality":"one"
            }]
        }"#,
    )
    .unwrap();

    assert!(descriptor.capabilities.is_empty());
    assert_eq!(descriptor.required_capabilities[0].requirement_id, "store");
}

#[test]
fn bun_descriptor_rejects_duplicate_providers() {
    let error = parse_descriptor_bytes(
        br#"{
            "abi":"lenso.json-request@1",
            "capabilities":[
                {"capability_id":"company.notes@1","descriptor_version":"1.0.0","request_operations":["read"]},
                {"capability_id":"company.notes@1","descriptor_version":"1.0.0","request_operations":["write"]}
            ]
        }"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("repeats provided Capability"));
}

#[test]
fn web_plugin_scaffold_uses_canonical_endpoint_authoring() {
    let files = web_plugin_scaffold("company.greetings-http");
    let manifest = files.get(Path::new("Cargo.toml")).unwrap();
    let build = files.get(Path::new("build.rs")).unwrap();
    let source = files.get(Path::new("src/lib.rs")).unwrap();
    let create_route = files.get(Path::new("src/routes/create.rs")).unwrap();
    let search_route = files.get(Path::new("src/routes/search.rs")).unwrap();
    let readme = files.get(Path::new("README.md")).unwrap();

    assert!(manifest.contains("plugin-id = \"company.greetings-http\""));
    assert!(manifest.contains("root-slot = \"web\""));
    assert!(manifest.contains("lenso-capability-http-endpoint"));
    assert!(manifest.contains("version = \"0.3.4\""));
    assert!(manifest.contains("lenso = { version = \"=0.5.25\""));
    assert!(manifest.contains(LENSO_FRAMEWORK_REVISION));
    assert!(manifest.contains("lenso-app-plan = { version = \"=0.4.5\""));
    assert!(manifest.contains("lenso-kernel = { version = \"=0.3.11\""));
    assert!(manifest.contains("[patch.crates-io]"));
    assert!(manifest.contains("lenso-native-adapter"));
    assert!(manifest.contains("lenso-test = { version = \"=0.1.2\""));
    assert!(manifest.contains("lenso-web-host"));
    assert!(manifest.contains("lenso-test"));
    assert!(manifest.contains("schemars = \"1.2\""));
    assert!(manifest.contains("syn = { version = \"2\", features = [\"full\"] }"));
    assert!(source.contains("#[lenso::plugin]"));
    assert!(source.contains("include!(concat!(env!(\"OUT_DIR\"), \"/web_routes.rs\"))"));
    assert!(build.contains("syn::parse_file"));
    assert!(build.contains("#[endpoint]"));
    assert!(create_route.contains("#[openapi_contract("));
    assert!(create_route.contains("requestBody:"));
    assert!(create_route.contains("\"400\":"));
    assert!(create_route.contains("\"415\":"));
    assert!(source.contains("JsonSchema"));
    assert!(search_route.contains("#[post("));
    assert!(search_route.contains("#[openapi_contract("));
    assert!(search_route.contains("requestBody:"));
    assert!(search_route.contains("\"invalid_term\""));
    assert!(create_route.contains("Result<(StatusCode, Json<Greeting>), Problem>"));
    assert!(source.contains("EndpointTest"));
    assert!(source.contains("pub const fn link()"));
    let simulated = files.get(Path::new("tests/simulated_web.rs")).unwrap();
    assert!(simulated.contains("SimulatedWebHost"));
    assert!(simulated.contains("prepare_simulated"));
    assert!(simulated.contains(r##"Bytes::from_static(br#"{"name":"Lenso"}"#)"##));
    let golden_path = files.get(Path::new("WEB_GOLDEN_PATH.md")).unwrap();
    assert!(golden_path.contains("business Capability"));
    assert!(golden_path.contains("open_stream"));
    assert!(golden_path.contains("lenso-test@0.1.2"));
    assert!(golden_path.contains("root patch"));
    assert!(!source.contains("NativeModuleFactory"));
    assert!(!readme.contains("lenso plugin pack"));
    assert!(readme.contains("lenso plugin dev"));
}

#[test]
fn web_plugin_new_writes_the_complete_project() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "company.greetings-http".to_owned(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Multi,
        web: true,
        no_install: true,
        dry_run: false,
    })
    .unwrap();

    let project = root.path().join("company.greetings-http");
    for path in [
        "Cargo.toml",
        "build.rs",
        "src/lib.rs",
        "src/routes/create.rs",
        "src/routes/search.rs",
        "tests/simulated_web.rs",
        "WEB_GOLDEN_PATH.md",
        "README.md",
    ] {
        assert!(project.join(path).is_file(), "missing generated {path}");
    }
}

#[test]
#[ignore = "clean-room test downloads pinned Web dependencies and runs generated tests"]
fn clean_room_web_plugin_runs_generated_tests() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "company.greetings-http".to_owned(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Multi,
        web: true,
        no_install: false,
        dry_run: false,
    })
    .unwrap();
    let project = root.path().join("company.greetings-http");
    let library = project.join("src/lib.rs");
    let original = fs::read_to_string(&library).unwrap();
    fs::write(
        project.join("src/routes/health.rs"),
        "#[get(\"greetings.health\", \"/health\")]\nasync fn health(&self) -> Result<HandleResponse, Problem> {\n    Ok(lenso_capability_http_endpoint::response::text(StatusCode::OK, \"ok\"))\n}\n",
    )
    .unwrap();
    fs::write(
        &library,
        format!(
            "{original}\n#[cfg(test)] mod route_file_proof {{\n    use super::*;\n    #[test] fn added_route_dispatches() {{\n        let result = futures::executor::block_on(lenso_capability_http_endpoint::testing::EndpointTest::new(GreetingsHttp::default()).request(\"greetings.health\").send()).unwrap();\n        assert_eq!(result.status(), StatusCode::OK);\n    }}\n}}\n"
        ),
    )
    .unwrap();
    run_cargo(&project, &["test", "--locked"], "test added Web route").unwrap();

    fs::write(
        project.join("src/routes/duplicate.rs"),
        "#[post(\"greetings.other\", \"/greetings\")]\nasync fn duplicate(&self) -> Result<HandleResponse, Problem> { unreachable!() }\n",
    )
    .unwrap();
    let conflict = Command::new("cargo")
        .args(["check", "--locked"])
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    let diagnostic = String::from_utf8_lossy(&conflict.stderr);
    assert!(diagnostic.contains("duplicate Web POST /greetings"));
    assert!(diagnostic.contains("duplicate.rs"));
    assert!(diagnostic.contains("create.rs"));

    fs::write(
        project.join("src/routes/duplicate.rs"),
        "#[get(\"greetings.create\", \"/another\")]\nasync fn duplicate(&self) -> Result<HandleResponse, Problem> { unreachable!() }\n",
    )
    .unwrap();
    let conflict = Command::new("cargo")
        .args(["check", "--locked"])
        .current_dir(&project)
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    let diagnostic = String::from_utf8_lossy(&conflict.stderr);
    assert!(diagnostic.contains("duplicate Web route ID `greetings.create`"));
    assert!(diagnostic.contains("duplicate.rs"));
    assert!(diagnostic.contains("create.rs"));

    fs::remove_file(project.join("src/routes/duplicate.rs")).unwrap();
    fs::remove_file(project.join("src/routes/health.rs")).unwrap();
    fs::write(&library, original).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let routes = project.join("src/routes");
        let outside = root.path().join("outside-routes");
        let build = project.join("build.rs");
        let original_build = fs::read_to_string(&build).unwrap();
        fs::rename(&routes, &outside).unwrap();
        symlink(&outside, &routes).unwrap();
        fs::write(&build, format!("{original_build}\n// symlink proof\n")).unwrap();
        let rejected = Command::new("cargo")
            .args(["check", "--locked"])
            .current_dir(&project)
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        let diagnostic = String::from_utf8_lossy(&rejected.stderr);
        assert!(diagnostic.contains("route source must be a real directory"));
        assert!(diagnostic.contains("src/routes"));
        fs::remove_file(&routes).unwrap();
        fs::rename(&outside, &routes).unwrap();
        fs::write(&build, original_build).unwrap();
    }

    run_cargo(&project, &["test", "--locked"], "test removed Web route").unwrap();
}

#[test]
#[ignore = "clean-room test downloads pinned Web dependencies and builds the generated dev Host"]
fn clean_room_web_plugin_builds_generated_dev_host() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "company.greetings-http".to_owned(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Multi,
        web: true,
        no_install: false,
        dry_run: false,
    })
    .unwrap();
    let project = root.path().join("company.greetings-http");
    let package = read_package(&project.join("Cargo.toml")).unwrap();
    let host = super::web_dev::DevHost::prepare(&project, &package).unwrap();
    host.build().unwrap();
}

#[test]
fn rust_plugin_scaffold_exposes_only_portable_authoring() {
    let files = plugin_scaffold("uppercase");
    let author_source = files.get(Path::new("src/lib.rs")).unwrap();
    let all = files.values().cloned().collect::<String>();

    assert!(author_source.contains("#[lenso::plugin]"));
    assert!(author_source.contains("#[lenso_agent_tool_sdk::tool_provider]"));
    assert!(author_source.contains("#[tool("));
    assert!(author_source.contains("fn execute(arguments: Arguments)"));
    assert!(all.contains("plugin-id = \"uppercase\""));
    assert!(all.contains("root-slot = \"tool-providers\""));
    assert!(all.contains("lenso plugin new"));
    assert!(all.contains("lenso plugin dev"));
    assert!(all.contains("lenso plugin check"));
    assert!(all.contains("lenso plugin pack"));
    for internal in [
        "wit_bindgen",
        "guest_request_plugin",
        "ProcessPlugin",
        "ProcessOutcome",
        "request_json",
        "arguments_json",
        "lenso.agent.tool-provider",
        "lenso.generated",
    ] {
        assert!(
            !author_source.contains(internal),
            "author source leaked `{internal}`"
        );
    }
    for removed in [
        "src/plugin.rs",
        "src/lenso.generated.rs",
        "src/lenso.wasm.generated.rs",
        "src/lenso.process.generated.rs",
        "lenso.generated.descriptor.json",
        "wit/world.wit",
    ] {
        assert!(
            !files.contains_key(Path::new(removed)),
            "unexpected `{removed}`"
        );
    }
}

#[test]
fn bun_plugin_scaffold_uses_generic_and_product_owned_declarations() {
    let files = bun_plugin_scaffold("example.echo");
    let package = files.get(Path::new("package.json")).unwrap();
    let author = files.get(Path::new("src/plugin.ts")).unwrap();

    assert!(package.contains("\"runtime\": \"bun\""));
    assert!(package.contains("\"@lenso/bun-plugin\": \"0.2.2\""));
    assert!(package.contains("\"@lenso/agent-tool-sdk\": \"0.1.0\""));
    assert!(author.contains("tools(["));
    assert!(author.contains("schema.object"));
    assert!(author.contains("definePlugin"));
    assert!(!author.contains("serve("));
    for generated in [
        "src/lenso.bun.generated.ts",
        "src/lenso.describe.generated.ts",
        "src/lenso.invoke.generated.ts",
    ] {
        assert!(!files.contains_key(Path::new(generated)));
    }
}

#[test]
fn process_plugin_scaffold_uses_the_sdk_owned_lowering() {
    let files = process_plugin_scaffold("uppercase");
    let manifest = files.get(Path::new("Cargo.toml")).unwrap();
    let entrypoint = files.get(Path::new("src/main.rs")).unwrap();

    assert!(manifest.contains("runtime = \"process\""));
    assert!(manifest.contains("package = \"lenso-plugin-sdk\", version = \"0.4.1\""));
    assert!(!manifest.contains("lenso-runtime-rust\""));
    assert!(manifest.contains("lenso-agent-tool-sdk"));
    assert!(!manifest.contains("github.com/LioRael/lenso-agent"));
    assert_eq!(
        entrypoint,
        "// Cargo Process entrypoint; the SDK supplies main and protocol lowering.\ninclude!(\"lib.rs\");\n"
    );
    assert!(!files.contains_key(Path::new("lenso.generated.descriptor.json")));
}

#[test]
fn multi_scaffold_keeps_one_business_source_for_two_outputs() {
    let files = multi_plugin_scaffold("uppercase");
    let manifest = files.get(Path::new("Cargo.toml")).unwrap();

    assert!(manifest.contains("outputs = [\"wasm\", \"process\"]"));
    assert!(files.contains_key(Path::new("src/lib.rs")));
    assert!(files.contains_key(Path::new("src/main.rs")));
    assert_eq!(
        files
            .keys()
            .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
            .count(),
        2
    );
    let author_source = files.get(Path::new("src/lib.rs")).unwrap();
    for runtime_detail in ["wit_bindgen", "ProcessPlugin", "ProcessOutcome", "Guest"] {
        assert!(
            !author_source.contains(runtime_detail),
            "author source leaked runtime detail `{runtime_detail}`"
        );
    }
}

#[test]
fn cargo_project_can_declare_rust_and_typescript_implementations() {
    let root = tempfile::tempdir().unwrap();
    let manifest = root.path().join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "document-sync"
version = "0.1.0"
[package.metadata.lenso]
plugin-id = "example.document-sync"
root-slot = "document-sync"
[package.metadata.lenso-cli]
implementations = [
  { id = "rust-process", path = ".", runtime = "process" },
  { id = "typescript-bun", path = "typescript", runtime = "bun" },
]
"#,
    )
    .unwrap();

    let package = read_package(&manifest).unwrap();
    assert_eq!(
        project_runtime(&package).unwrap(),
        ProjectRuntime::Composite
    );
    let implementations = &package.metadata.lenso_cli.unwrap().implementations;
    assert_eq!(implementations[0].id, "rust-process");
    assert_eq!(implementations[1].path, Path::new("typescript"));
}

#[test]
fn cargo_project_can_declare_explicit_variant_groups() {
    let root = tempfile::tempdir().unwrap();
    let manifest = root.path().join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "grouped"
version = "1.0.0"
[package.metadata.lenso]
plugin-id = "example.grouped"
root-slot = "tools"
[package.metadata.lenso-cli]
implementations = [
  { id = "mac", group = "portable", path = "mac", runtime = "process" },
  { id = "linux", group = "portable", path = "linux", runtime = "process" },
]
"#,
    )
    .unwrap();

    let package = read_package(&manifest).unwrap();
    assert_eq!(
        project_runtime(&package).unwrap(),
        ProjectRuntime::Composite
    );
    let declarations = &package.metadata.lenso_cli.unwrap().implementations;
    assert!(
        declarations
            .iter()
            .all(|declaration| declaration.group.as_deref() == Some("portable"))
    );
}

#[test]
#[ignore = "clean-room test downloads Plugin SDK crates and compiles two Process variants"]
fn clean_room_grouped_plugin_pack_produces_v5_archive() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("grouped");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("src/lib.rs"), "pub fn source_root() {}\n").unwrap();
    fs::write(
        project.join("Cargo.toml"),
        r#"[package]
name = "grouped"
version = "0.1.0"
edition = "2024"
publish = false
[package.metadata.lenso]
plugin-id = "example.grouped"
root-slot = "tool-providers"
[package.metadata.lenso-cli]
implementations = [
  { id = "first", group = "portable", path = "first", runtime = "process" },
  { id = "second", group = "portable", path = "second", runtime = "process" },
]
[workspace]
"#,
    )
    .unwrap();
    for variant in ["first", "second"] {
        let directory = project.join(variant);
        for (relative, content) in process_plugin_scaffold("example.grouped") {
            let path = directory.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }
    let output = root.path().join("grouped.lenso-plugin");

    pack(PluginPackArgs {
        repo_root: Some(project),
        output: Some(output.clone()),
        json: true,
    })
    .unwrap();
    let manifest = with_bundle_directory(&output, |directory| {
        read_bundle_manifest(directory).map_err(Into::into)
    })
    .unwrap();
    assert!(matches!(
        manifest,
        PluginManifest::V5(value)
            if value.implementations.len() == 1
                && value.implementations[0].id == "portable"
                && value.implementations[0].variants.len() == 2
    ));
}

#[test]
fn process_artifacts_use_the_canonical_rust_host_target() {
    assert_eq!(
        rust_host_target(Path::new(".")).unwrap(),
        native_host_target()
    );
}

#[test]
fn duplicate_plugin_identity_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let manifest = root.path().join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "duplicate"
version = "0.1.0"
[package.metadata.lenso]
plugin-id = "first"
plugin-id = "second"
"#,
    )
    .unwrap();

    assert!(read_package(&manifest).is_err());
}

#[test]
fn multi_dev_auto_selects_only_the_fast_process_path() {
    assert_eq!(
        resolve_dev_selection(ProjectRuntime::Multi, DevImplementationArg::Auto).unwrap(),
        DevSelection {
            build: DevBuild::Process,
            invoke: ProjectRuntime::Process,
        }
    );
    assert_eq!(
        resolve_dev_selection(ProjectRuntime::Multi, DevImplementationArg::Wasm).unwrap(),
        DevSelection {
            build: DevBuild::Wasm,
            invoke: ProjectRuntime::Wasm,
        }
    );
    assert_eq!(
        resolve_dev_selection(ProjectRuntime::Multi, DevImplementationArg::All).unwrap(),
        DevSelection {
            build: DevBuild::All,
            invoke: ProjectRuntime::Process,
        }
    );
}

#[test]
fn dev_rejects_an_implementation_the_project_does_not_declare() {
    assert!(resolve_dev_selection(ProjectRuntime::Wasm, DevImplementationArg::Process).is_err());
    assert!(resolve_dev_selection(ProjectRuntime::Process, DevImplementationArg::Wasm).is_err());
}

#[test]
fn malformed_plugin_package_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lenso-plugin.json"), b"{}\n").unwrap();

    assert!(verify_bundle_directory(root.path()).is_err());
}

#[tokio::test]
#[ignore = "clean-room test downloads released crates and compiles wasm32"]
async fn clean_room_plugin_runs_new_check_dev_and_pack() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "company.uppercase".to_owned(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Wasm,
        web: false,
        no_install: false,
        dry_run: false,
    })
    .unwrap();
    let project = root.path().join("company.uppercase");
    check(PluginCheckArgs {
        repo_root: Some(project.clone()),
        json: true,
    })
    .unwrap();
    dev::run(PluginDevArgs {
        repo_root: Some(project.clone()),
        operation: Some("execute".to_owned()),
        request_json: r#"{"name":"company.uppercase","arguments_json":"{\"text\":\"hello\"}"}"#
            .to_owned(),
        config_json: "{}".to_owned(),
        json: true,
        watch: false,
        implementation: DevImplementationArg::Auto,
    })
    .await
    .unwrap();
    let output = project.join("dist/company.uppercase.lenso-plugin");
    pack(PluginPackArgs {
        repo_root: Some(project.clone()),
        output: Some(output.clone()),
        json: true,
    })
    .unwrap();
    with_bundle_directory(&output, |directory| {
        verify_bundle_directory(directory)
            .map(|_| ())
            .map_err(Into::into)
    })
    .unwrap();
    assert!(
        pack(PluginPackArgs {
            repo_root: Some(project),
            output: Some(output),
            json: false,
        })
        .is_err()
    );
}

#[tokio::test]
#[ignore = "clean-room test downloads git dependencies and compiles both scaffold outputs"]
async fn clean_room_multi_plugin_auto_dev_runs_the_process_build() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "company.multi-smoke".to_owned(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Multi,
        web: false,
        no_install: false,
        dry_run: false,
    })
    .unwrap();
    dev::run(PluginDevArgs {
        repo_root: Some(root.path().join("company.multi-smoke")),
        operation: Some("execute".to_owned()),
        request_json:
            r#"{"name":"company.multi-smoke","arguments_json":"{\"text\":\"auto-process\"}"}"#
                .to_owned(),
        config_json: "{}".to_owned(),
        json: true,
        watch: false,
        implementation: DevImplementationArg::Auto,
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "clean-room test downloads git dependencies and compiles a native executable"]
async fn clean_room_process_plugin_runs_new_check_dev_and_pack() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "company.uppercase".to_owned(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Process,
        web: false,
        no_install: false,
        dry_run: false,
    })
    .unwrap();
    let project = root.path().join("company.uppercase");
    check(PluginCheckArgs {
        repo_root: Some(project.clone()),
        json: true,
    })
    .unwrap();
    dev::run(PluginDevArgs {
        repo_root: Some(project.clone()),
        operation: Some("execute".to_owned()),
        request_json: r#"{"name":"company.uppercase","arguments_json":"{\"text\":\"hello\"}"}"#
            .to_owned(),
        config_json: "{}".to_owned(),
        json: true,
        watch: false,
        implementation: DevImplementationArg::Auto,
    })
    .await
    .unwrap();
    let output = project.join("dist/company.uppercase.lenso-plugin");
    pack(PluginPackArgs {
        repo_root: Some(project),
        output: Some(output.clone()),
        json: true,
    })
    .unwrap();
    with_bundle_directory(&output, |directory| {
        verify_bundle_directory(directory)
            .map(|_| ())
            .map_err(Into::into)
    })
    .unwrap();
}

#[test]
fn dependency_free_implementations_share_the_same_authoring_contract() {
    let legacy =
        lenso_app_plan::authoring::PluginContract::new("dev.fixture.echo", "1.0.0", "tools");
    let modern = legacy.clone().with_authoring_version(2);
    assert_eq!(
        super::shared_portable_contract(legacy.clone(), modern.clone()).unwrap(),
        modern
    );
    assert_eq!(
        super::shared_portable_contract(legacy.clone(), legacy.clone()).unwrap(),
        legacy
    );
    let different = modern
        .clone()
        .with_capability(lenso_app_plan::CapabilityEndpointPlan::new(
            "dev.fixture.other@1",
            "1.0.0",
            ["echo"],
        ));
    assert!(super::shared_portable_contract(legacy.clone(), different).is_err());
    let legacy_dependency = legacy.with_requirement(
        lenso_app_plan::CapabilityRequirementPlan::one("dev.fixture.store@1", "1.0.0"),
    );
    let modern_dependency = legacy_dependency.clone().with_authoring_version(2);
    assert!(super::shared_portable_contract(legacy_dependency, modern_dependency).is_err());
}

// The CLI runs under Tokio; Bun startup has its own synchronous RPC runtime.
// This regression must exercise real startup and invocation, not only lowering.
#[tokio::test]
#[ignore = "clean-room test installs Bun dependencies and invokes a Tool Provider"]
async fn clean_room_bun_tool_dev_does_not_nest_tokio_runtimes() {
    let root = tempfile::tempdir().unwrap();
    create(PluginNewArgs {
        plugin_id: "example.echo".into(),
        repo_root: Some(root.path().to_path_buf()),
        dir: None,
        runtime: PluginRuntimeArg::Bun,
        web: false,
        no_install: false,
        dry_run: false,
    })
    .unwrap();
    dev::run(PluginDevArgs {
        repo_root: Some(root.path().join("example.echo")),
        operation: Some("execute".into()),
        request_json: r#"{"name":"example.echo","arguments_json":"{\"text\":\"hello\"}"}"#.into(),
        config_json: "{}".into(),
        json: true,
        watch: false,
        implementation: DevImplementationArg::Bun,
    })
    .await
    .unwrap();
}
