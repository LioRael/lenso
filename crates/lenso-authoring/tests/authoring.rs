use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use lenso_app_plan::ModuleArtifact;
use lenso_authoring::{
    AddModule, CapabilityEndpoint, CapabilityRequirement, CheckOptions, ContractInput,
    LockedPackage, Module, PackageInput, PackageSource, ProjectFile, ProjectPath, RequestAdmission,
    ResolutionOptions, run_project,
};
use lenso_bun_adapter::{BunAdapter, BunAdapterConfig, BunWire};
use lenso_kernel::{ExecutionAdapterCatalog, TerminalOutcome};
use lenso_native_adapter::NativeModuleRegistry;
use lenso_native_greeter::GreeterFactory;

fn locked_package(
    root: &Path,
    package: &str,
    source: PackageSource,
) -> (PackageInput, LockedPackage) {
    let artifact = root.join(format!("{package}.artifact"));
    fs::write(&artifact, package).expect("fixture artifact should be writable");
    let digest = lenso_authoring::sha256_file(&artifact).expect("fixture digest should work");
    (
        PackageInput::new(package, source, "1.0.0"),
        LockedPackage::new(
            package,
            source,
            "1.0.0",
            artifact.file_name().unwrap().to_string_lossy(),
            digest,
        ),
    )
}

fn project_with_greeting(root: &Path) -> ProjectFile {
    let mut project = ProjectFile::default();
    let (package, locked) = locked_package(root, "example.greeter", PackageSource::Cargo);
    project
        .packages_mut()
        .insert(package.name().to_owned(), package);
    project.lock_mut().insert(locked);
    project.composition_mut().add_module(
        Module::new("greeter", "example.greeter").with_capability(CapabilityEndpoint::request(
            "example.greeting@1",
            "1.0.0",
            ["greet"],
        )),
    );
    project
}

fn clean_project(root: &Path) -> PathBuf {
    let path = root.join("lenso.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&ProjectFile::default()).unwrap(),
    )
    .expect("clean project should be writable");
    path
}

#[test]
fn resolve_is_canonical_and_carries_locked_artifact_identity() {
    let temporary = tempfile_dir();
    let first = project_with_greeting(&temporary);
    let mut second = first.clone();
    second.composition_mut().modules_mut().reverse();

    let first_resolved = first
        .resolve(&temporary, &ResolutionOptions::default())
        .expect("first project should resolve");
    let second_resolved = second
        .resolve(&temporary, &ResolutionOptions::default())
        .expect("equivalent project should resolve");

    assert_eq!(
        first_resolved.canonical_bytes(),
        second_resolved.canonical_bytes()
    );
    assert_eq!(
        first_resolved
            .plan()
            .module_instance("greeter")
            .unwrap()
            .artifact(),
        Some(&ModuleArtifact::new(
            "cargo",
            "example.greeter.artifact",
            "1.0.0",
            first.lock().get("example.greeter").unwrap().digest(),
        ))
    );
    assert_eq!(
        first_resolved.document().modules[0]
            .module
            .execution_class(),
        Some("lenso.native-rust@1")
    );
}

#[test]
fn resolve_preserves_explicit_binding_admission_policies() {
    let temporary = tempfile_dir();
    let mut project = project_with_greeting(&temporary);
    let (package, locked) = locked_package(&temporary, "example.consumer", PackageSource::Cargo);
    project
        .packages_mut()
        .insert(package.name().to_owned(), package);
    project.lock_mut().insert(locked);
    project.composition_mut().add_module(
        Module::new("consumer", "example.consumer")
            .with_requirement(CapabilityRequirement::one("example.greeting@1", "1.0.0")),
    );
    project.composition_mut().add_binding(
        lenso_authoring::Binding::new("consumer", "example.greeting@1", "1.0.0", "greeter")
            .with_admission(RequestAdmission::new(3, 2))
            .with_event_capacity(5),
    );

    let resolved = project
        .resolve(&temporary, &ResolutionOptions::default())
        .expect("binding policies should resolve");
    let binding = &resolved.document().bindings[0];
    assert_eq!(binding.admission(), Some(RequestAdmission::new(3, 2)));
    assert_eq!(binding.event_capacity(), Some(5));
}

#[test]
fn check_rejects_missing_one_binding_and_unavailable_execution_class() {
    let temporary = tempfile_dir();
    let mut project = project_with_greeting(&temporary);
    project.composition_mut().add_module(
        Module::new("consumer", "example.consumer")
            .with_requirement(CapabilityRequirement::one("example.greeting@1", "1.0.0")),
    );
    let (package, locked) = locked_package(&temporary, "example.consumer", PackageSource::Cargo);
    project
        .packages_mut()
        .insert(package.name().to_owned(), package);
    project.lock_mut().insert(locked);
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("the unresolved one binding should be reported");
    assert!(error.to_string().contains("missing one binding"));

    let mut project = project_with_greeting(&temporary);
    project
        .composition_mut()
        .modules_mut()
        .first_mut()
        .unwrap()
        .set_execution_class("community.missing@1");
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("an unavailable execution class should be rejected");
    assert!(error.to_string().contains("community.missing@1"));
}

#[test]
fn check_rejects_corrupt_lock_identity_and_secret_remote_locator() {
    let temporary = tempfile_dir();
    let mut project = project_with_greeting(&temporary);
    let package = project
        .packages_mut()
        .remove("example.greeter")
        .expect("greeting package should exist");
    project.packages_mut().insert(
        "example.greeter".to_owned(),
        PackageInput::new("different.package", package.source(), package.version()),
    );
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("embedded package identity must match its map key");
    assert!(error.to_string().contains("embedded package identity"));

    let mut project = project_with_greeting(&temporary);
    project.lock_mut().insert(LockedPackage::new(
        "example.greeter",
        PackageSource::Cargo,
        "1.0.0",
        "https://user:password@example.test/greeter",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ));
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("credential-bearing remote locators must not enter a Plan");
    assert!(error.to_string().contains("contains credentials"));
}

#[test]
fn check_rejects_unknown_bindings_and_unsupported_schema_keywords() {
    let temporary = tempfile_dir();
    let mut project = project_with_greeting(&temporary);
    project
        .composition_mut()
        .add_binding(lenso_authoring::Binding::new(
            "missing-consumer",
            "example.greeting@1",
            "1.0.0",
            "greeter",
        ));
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("unknown binding endpoints must not be discarded");
    assert!(error.to_string().contains("unknown Module Instance"));

    fs::write(
        temporary.join("config.schema.json"),
        r#"{"type":"string","pattern":"^[a-z]+$"}"#,
    )
    .expect("configuration schema should be writable");
    let mut project = project_with_greeting(&temporary);
    let module = project.composition_mut().modules_mut().first_mut().unwrap();
    let replacement = std::mem::replace(module, Module::new("greeter", "example.greeter"))
        .with_configuration_schema("config.schema.json")
        .with_configuration(serde_json::json!("hello"));
    *module = replacement;
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("unsupported schema constraints must be explicit");
    assert!(
        error
            .to_string()
            .contains("unsupported JSON Schema keyword pattern")
    );
}

#[test]
fn add_updates_composition_and_the_selected_package_input() {
    let temporary = tempfile_dir();
    let project_path = temporary.join("lenso.json");
    fs::write(temporary.join("Cargo.toml"), "[dependencies]\n")
        .expect("manifest should be writable");
    fs::write(
        &project_path,
        serde_json::to_vec_pretty(&ProjectFile::default()).unwrap(),
    )
    .expect("project should be writable");

    let result = ProjectPath::new(&project_path)
        .add(&AddModule::new(
            Module::new("greeter", "example.greeter"),
            PackageInput::new("example.greeter", PackageSource::Cargo, "1.0.0")
                .with_manifest("Cargo.toml"),
        ))
        .expect("add should update the project");

    assert!(
        result
            .changed_files()
            .iter()
            .any(|path| path.ends_with("lenso.json"))
    );
    assert!(
        result
            .changed_files()
            .iter()
            .any(|path| path.ends_with("Cargo.toml"))
    );
    assert_eq!(
        fs::read_to_string(temporary.join("Cargo.toml")).unwrap(),
        "[dependencies]\nexample.greeter = \"1.0.0\"\n"
    );
    let loaded = ProjectPath::load(&project_path).expect("updated project should parse");
    assert!(
        loaded
            .composition()
            .modules()
            .iter()
            .any(|module| module.key() == "greeter")
    );
    assert_eq!(loaded.packages()["example.greeter"].version(), "1.0.0");
}

#[test]
fn add_does_not_replace_a_structured_cargo_dependency() {
    let temporary = tempfile_dir();
    let project_path = temporary.join("lenso.json");
    let manifest_path = temporary.join("Cargo.toml");
    let original_manifest = "[dependencies]\nexample.greeter = { path = \"../greeter\" }\n";
    fs::write(&manifest_path, original_manifest).expect("manifest should be writable");
    fs::write(
        &project_path,
        serde_json::to_vec_pretty(&ProjectFile::default()).unwrap(),
    )
    .expect("project should be writable");

    let error = ProjectPath::new(&project_path)
        .add(&AddModule::new(
            Module::new("greeter", "example.greeter"),
            PackageInput::new("example.greeter", PackageSource::Cargo, "1.0.0")
                .with_manifest("Cargo.toml"),
        ))
        .expect_err("structured dependency declarations must not be overwritten");

    assert!(error.to_string().contains("not a simple version"));
    assert_eq!(
        fs::read_to_string(manifest_path).unwrap(),
        original_manifest
    );
    assert!(
        ProjectPath::load(&project_path)
            .unwrap()
            .composition()
            .modules()
            .is_empty()
    );
}

#[test]
fn web_profile_is_composition_data_not_a_runtime_mode() {
    let mut project = ProjectFile::default();
    project.profiles_mut().insert(
        "web".to_owned(),
        lenso_authoring::WebProfile::new(["shell", "business-ui", "business"]),
    );

    let profile = project.profile("web").expect("profile should be authored");
    assert_eq!(profile.modules(), &["shell", "business-ui", "business"]);
    assert!(
        project
            .resolve(
                Path::new("."),
                &ResolutionOptions::default().with_profile("web"),
            )
            .is_err()
    );
}

#[test]
fn check_rejects_secret_values_but_accepts_secret_references() {
    let temporary = tempfile_dir();
    let mut project = project_with_greeting(&temporary);
    project
        .composition_mut()
        .modules_mut()
        .first_mut()
        .unwrap()
        .set_configuration(serde_json::json!({"api_token": "not-a-reference"}));
    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("secret values must not enter the Plan");
    assert!(error.to_string().contains("secret value"));

    project
        .composition_mut()
        .modules_mut()
        .first_mut()
        .unwrap()
        .set_configuration(serde_json::json!({"api_token": {"secret_ref": "GREETER_TOKEN"}}));
    project
        .check(&temporary, &CheckOptions::default())
        .expect("secret references are authoring data, not secret values");
}

#[test]
fn check_validates_module_configuration_shape() {
    let temporary = tempfile_dir();
    fs::write(
        temporary.join("config.schema.json"),
        r#"{"type":"object","required":["port"],"properties":{"port":{"type":"integer"}},"additionalProperties":false}"#,
    )
    .expect("configuration schema should be writable");
    let mut project = project_with_greeting(&temporary);
    let module = project.composition_mut().modules_mut().first_mut().unwrap();
    let replacement = std::mem::replace(module, Module::new("greeter", "example.greeter"))
        .with_configuration_schema("config.schema.json")
        .with_configuration(serde_json::json!({"port": "not-an-integer"}));
    *module = replacement;

    let error = project
        .check(&temporary, &CheckOptions::default())
        .expect_err("configuration type mismatch should be rejected");
    assert!(error.to_string().contains("expected integer"));
}

#[test]
fn check_validates_bun_entrypoints_and_generated_contract_freshness() {
    let temporary = tempfile_dir();
    let artifact = temporary.join("provider.ts");
    fs::write(&artifact, "export const provider = true;\n")
        .expect("Bun artifact should be writable");
    let package = PackageInput::new("example.bun-provider", PackageSource::Bun, "1.0.0");
    let locked = LockedPackage::new(
        "example.bun-provider",
        PackageSource::Bun,
        "1.0.0",
        "provider.ts",
        lenso_authoring::sha256_file(&artifact).unwrap(),
    );
    let mut project = ProjectFile::default();
    project
        .packages_mut()
        .insert(package.name().to_owned(), package);
    project.lock_mut().insert(locked);
    project.composition_mut().add_module(
        Module::new("bun", "example.bun-provider")
            .with_entrypoint("provider.ts")
            .with_capability(CapabilityEndpoint::request(
                "example.greeting@1",
                "1.0.0",
                ["greet"],
            )),
    );
    project
        .check(&temporary, &CheckOptions::new(["lenso.bun-process@1"]))
        .expect("Bun entrypoint and execution class should check");

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut contracts = ProjectFile::default();
    contracts.contracts_mut().push(ContractInput::new(
        "crates/lenso-capability-greeting/capability.json",
        "crates/lenso-capability-greeting/src/generated.rs",
        "crates/lenso-capability-greeting/generated/bindings.ts",
    ));
    contracts
        .check(&workspace_root, &CheckOptions::default())
        .expect("checked-in generated contract should be fresh");
}

#[test]
fn cli_check_resolve_and_run_use_one_project_document() {
    let temporary = tempfile_dir();
    let project_path = temporary.join("lenso.json");
    fs::write(
        &project_path,
        serde_json::to_vec_pretty(&ProjectFile::default()).unwrap(),
    )
    .expect("project should be writable");

    let check = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["check", "--project"])
        .arg(&project_path)
        .output()
        .expect("check command should start");
    assert!(check.status.success());
    assert!(String::from_utf8_lossy(&check.stdout).contains("checked 0 Module Instances"));

    let output_path = temporary.join("resolved.json");
    let resolve = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["resolve", "--project"])
        .arg(&project_path)
        .args(["--output"])
        .arg(&output_path)
        .output()
        .expect("resolve command should start");
    assert!(resolve.status.success());
    assert!(output_path.is_file());

    let run = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["run", "--project"])
        .arg(&project_path)
        .output()
        .expect("run command should start");
    assert!(run.status.success());
    assert!(String::from_utf8_lossy(&run.stdout).contains("CleanShutdown"));
}

#[test]
fn resolved_plan_bytes_are_canonical_for_nested_configuration() {
    let temporary = tempfile_dir();
    let (package, locked) = locked_package(&temporary, "example.config", PackageSource::Cargo);
    let mut first = ProjectFile::default();
    first
        .packages_mut()
        .insert(package.name().to_owned(), package.clone());
    first.lock_mut().insert(locked.clone());
    first.composition_mut().add_module(
        Module::new("configured", "example.config").with_configuration(serde_json::json!({
            "nested": {"z": 1, "a": 2},
            "alpha": true,
        })),
    );

    let mut second = ProjectFile::default();
    second
        .packages_mut()
        .insert(package.name().to_owned(), package);
    second.lock_mut().insert(locked);
    second.composition_mut().add_module(
        Module::new("configured", "example.config").with_configuration(serde_json::json!({
            "alpha": true,
            "nested": {"a": 2, "z": 1},
        })),
    );

    let first_resolved = first
        .resolve(&temporary, &ResolutionOptions::default())
        .expect("first configuration should resolve");
    let second_resolved = second
        .resolve(&temporary, &ResolutionOptions::default())
        .expect("second configuration should resolve");
    assert_eq!(
        first_resolved.canonical_bytes(),
        second_resolved.canonical_bytes()
    );
}

#[test]
fn run_passes_the_resolved_plan_to_the_selected_native_adapter() {
    let temporary = tempfile_dir();
    let project_path = clean_project(&temporary);
    let (package, locked) =
        locked_package(&temporary, "example.native-greeter", PackageSource::Cargo);
    ProjectPath::new(&project_path)
        .add(&AddModule::new(
            Module::new("greeter", "example.native-greeter").with_capability(
                CapabilityEndpoint::request("example.greeting@1", "1.0.0", ["greet"]),
            ),
            package,
        ))
        .expect("native Module should be added to a clean project");
    let mut project = ProjectPath::load(&project_path).expect("added project should load");
    project.lock_mut().insert(locked);

    let driver = lenso_runner::TokioDriver::new();
    driver.request_shutdown();
    let adapters =
        ExecutionAdapterCatalog::single(NativeModuleRegistry::new().with_factory(GreeterFactory));
    let local = tokio::task::LocalSet::new();
    let outcome = local
        .block_on(
            &tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime should build"),
            run_project(
                &project,
                &temporary,
                driver,
                adapters,
                std::time::Duration::from_secs(1),
                ResolutionOptions::default(),
            ),
        )
        .expect("native project should run");

    assert!(matches!(outcome, TerminalOutcome::CleanShutdown));
}

#[test]
#[ignore = "requires Bun; CI runs ignored cross-runtime tests after installing Bun"]
fn run_passes_a_resolved_bun_module_to_the_bun_adapter() {
    let temporary = tempfile_dir();
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = "fixtures/bun/request-provider.ts";
    let fixture_source = workspace_root.join(fixture);
    let fixture_path = temporary.join(fixture);
    fs::create_dir_all(fixture_path.parent().unwrap()).expect("Bun fixture directory should exist");
    fs::copy(&fixture_source, &fixture_path).expect("Bun fixture should be copied");
    for relative in [
        "crates/lenso-capability-greeting/generated/bindings.ts",
        "crates/lenso-capability-secure-greeting/generated/bindings.ts",
        "crates/lenso-auth-sdk/typescript/actor.ts",
        "crates/lenso-otel-module/typescript/trace-context.ts",
    ] {
        let destination = temporary.join(relative);
        fs::create_dir_all(destination.parent().unwrap())
            .expect("Bun support directory should exist");
        fs::copy(workspace_root.join(relative), destination)
            .expect("Bun support module should be copied");
    }
    let project_path = clean_project(&temporary);
    let package = PackageInput::new("example.bun", PackageSource::Bun, "1.0.0");
    ProjectPath::new(&project_path)
        .add(&AddModule::new(
            Module::new("bun", "example.bun").with_entrypoint(fixture),
            package,
        ))
        .expect("Bun Module should be added to a clean project");
    let mut project = ProjectPath::load(&project_path).expect("added project should load");
    let locked = LockedPackage::new(
        "example.bun",
        PackageSource::Bun,
        "1.0.0",
        fixture,
        lenso_authoring::sha256_file(&fixture_path).expect("Bun fixture digest should work"),
    );
    project.lock_mut().insert(locked);

    let bun_binary = std::env::var_os("BUN_BIN")
        .map_or_else(|| std::path::PathBuf::from("bun"), std::path::PathBuf::from);
    let driver = lenso_runner::TokioDriver::new();
    driver.request_shutdown();
    let config =
        BunAdapterConfig::new(bun_binary, BunWire::FramedStdio).with_working_directory(&temporary);
    let adapters = ExecutionAdapterCatalog::single(
        BunAdapter::new("bun", BunWire::FramedStdio).with_config(config),
    );
    let local = tokio::task::LocalSet::new();
    let outcome = local
        .block_on(
            &tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime should build"),
            run_project(
                &project,
                &temporary,
                driver,
                adapters,
                std::time::Duration::from_secs(2),
                ResolutionOptions::default(),
            ),
        )
        .expect("Bun project should run");

    assert!(matches!(outcome, TerminalOutcome::CleanShutdown));
}

fn tempfile_dir() -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "lenso-authoring-test-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos(),
        // System time is not guaranteed to have unique nanoseconds across test threads.
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).expect("temporary directory should be created");
    path
}
