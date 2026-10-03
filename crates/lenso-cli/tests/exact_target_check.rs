//! Real CLI preflight with poisoned build tools. No target compilation is permitted.
#![cfg(unix)]
use lenso_app_plan::authoring::PluginDescriptor;
use lenso_engine_authoring::host_authoring::{GeneratedHostBuild, LocalPluginInput};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("app/store/src")).unwrap();
    fs::create_dir_all(root.join(".lenso")).unwrap();
    fs::create_dir_all(root.join("plugins")).unwrap();
    fs::write(
        root.join("app/store/Cargo.toml"),
        r#"
[package]
name = "store-fixture"
version = "1.0.0"
[package.metadata.lenso]
plugin-id = "example.store"
root-slot = "store"
[package.metadata.lenso-cli]
runtime = "native-linked"
[package.metadata.lenso-cli.support]
evidence = ["external-receipt-only"]
[[package.metadata.lenso-cli.support.combinations]]
environment = "native"
execution = "lenso.native-rust@1"
resources = { db = "postgresql" }
[[package.metadata.lenso-cli.support.combinations]]
environment = "workers"
execution = "lenso.native-rust@1"
resources = { db = "d1" }
"#,
    )
    .unwrap();
    fs::write(
        root.join("app/store/src/lib.rs"),
        "compile_error!(\"preflight must never compile this\");\n",
    )
    .unwrap();
    let input = |descriptor| LocalPluginInput {
        descriptor,
        manifest_digest: format!("sha256:{}", "a".repeat(64)),
        app_owned: true,
        source: "offline fixture".into(),
    };
    let authority = GeneratedHostBuild::lower_local(
        "example.app",
        vec![
            input(PluginDescriptor::new("example.store", "1.0.0", "store")),
            input(PluginDescriptor::new(
                "lenso.web-ingress",
                "1.0.0",
                "ingress",
            )),
        ],
    )
    .unwrap();
    fs::write(
        root.join(".lenso/host-build.json"),
        serde_json::to_vec(&authority).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".lenso/host-facility-sources.json"),
        serde_json::to_vec(&json!([{
            "alias":"store_fixture","package_id":"example.store","slot":"db",
            "native":"native_db","workers":"workers_db","workers_adapter":"adapter.mjs"
        }]))
        .unwrap(),
    )
    .unwrap();
    fs::create_dir(root.join("tools")).unwrap();
    for tool in ["cargo", "rustc", "bun", "wasm-bindgen", "jco"] {
        let path = root.join("tools").join(tool);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nprintf '%s\\n' '{tool}' >> '{}'/tool-started\nexit 97\n",
                root.display()
            ),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn grants(root: &Path, target: &str, implementation: &str) {
    fs::write(root.join("grants.json"), serde_json::to_vec(&json!({"schema":"lenso.host-facilities.v1","instances":{
        "example.store/default":{"db":{"configuration":{"implementation":implementation,"reference":"SECRET_REFERENCE_ONLY"},
            "binding":if target == "workers" {json!("DB")} else {Value::Null}}}
    }})).unwrap()).unwrap();
}

fn command(root: &Path, operation: &str, target: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lenso"));
    command
        .args(["app", operation, "--root"])
        .arg(root)
        .env("PATH", root.join("tools"))
        .env("SECRET_REFERENCE_ONLY", "MUST_NOT_APPEAR_IN_REPORT");
    if operation == "check" || target == "workers" {
        command.args(["--target", target]);
    }
    command
        .arg("--host-facilities")
        .arg(root.join("grants.json"));
    command
}

fn no_build(root: &Path, output: &Output) {
    assert!(!root.join("tool-started").exists(), "a build tool started");
    assert!(!root.join("target").exists());
    assert!(!root.join("dist").exists());
    assert!(!root.join("dist-workers").exists());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("MUST_NOT_APPEAR_IN_REPORT"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("MUST_NOT_APPEAR_IN_REPORT"));
}

#[test]
fn native_pg_and_workers_d1_pass_without_starting_any_build() {
    for (target, implementation) in [("native", "postgresql"), ("workers", "d1")] {
        let temp = tempfile::tempdir().unwrap();
        fixture(temp.path());
        grants(temp.path(), target, implementation);
        let output = command(temp.path(), "check", target)
            .arg("--json")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["build_started"], false);
        assert_eq!(report["secret_values_read"], false);
        assert_eq!(report["qualification"], "not_assessed");
        assert_eq!(report["resource_readiness"], "not_run");
        assert_eq!(report["instances"][0]["support"], "declared");
        no_build(temp.path(), &output);
    }
}

#[test]
fn cross_product_is_rejected_by_check_and_build_before_tools_start() {
    for (target, implementation) in [("native", "d1"), ("workers", "postgresql")] {
        for operation in ["check", "build"] {
            let temp = tempfile::tempdir().unwrap();
            fixture(temp.path());
            grants(temp.path(), target, implementation);
            let output = command(temp.path(), operation, target).output().unwrap();
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains("example.store/default")
                    && error.contains("required exact support")
                    && error.contains("available declared combinations"),
                "{error}"
            );
            if operation == "build" {
                assert!(error.contains("no Cargo/Bun/Wasm build started"), "{error}");
            }
            no_build(temp.path(), &output);
        }
    }
}

#[test]
fn missing_resource_binding_and_source_entry_are_reported_without_tools() {
    let temp = tempfile::tempdir().unwrap();
    fixture(temp.path());
    grants(temp.path(), "workers", "d1");
    let mut value: Value =
        serde_json::from_slice(&fs::read(temp.path().join("grants.json")).unwrap()).unwrap();
    value["instances"]["example.store/default"]["db"]["binding"] = Value::Null;
    fs::write(
        temp.path().join("grants.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let output = command(temp.path(), "check", "workers").output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("required Workers binding reference"));
    no_build(temp.path(), &output);
    grants(temp.path(), "native", "postgresql");
    fs::remove_file(temp.path().join("app/store/src/lib.rs")).unwrap();
    let output = command(temp.path(), "check", "native").output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("source entry/export is missing"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    no_build(temp.path(), &output);
}
