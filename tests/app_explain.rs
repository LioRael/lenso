use std::{fs, process::Command};

use lenso_app_plan::{
    CapabilityEndpointPlan, ExecutionClassId,
    authoring::{HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor},
};
use serde_json::json;

fn app_root() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    fs::create_dir(root.join(".lenso")).unwrap();
    fs::create_dir(root.join("plugins")).unwrap();
    let descriptor = PluginDescriptor::new("company.orders", "1.0.0", "orders")
        .with_authoring(2, "lenso.native-rust@1")
        .with_execution_class(ExecutionClassId::native_rust())
        .with_capability(
            CapabilityEndpointPlan::new("company.orders@1", "1.0.0", ["create", "watch"])
                .with_stream_operation("watch"),
        );
    let catalog = HostCatalog::new(
        [HostSlot::one("orders")],
        [HostPluginRelease::new(descriptor)],
        [HostDefaultPlugin::new("company.orders", "default")],
    );
    fs::write(
        root.join(".lenso/host-catalog.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    temporary
}

fn write_profile(root: &std::path::Path, capabilities: &serde_json::Value) -> std::path::PathBuf {
    let profile = root.join("target-profile.json");
    fs::write(
        &profile,
        serde_json::to_vec_pretty(&json!({
            "profile": "lenso.execution-target-capability-profile@1",
            "target_profile": "lenso.native-rust@1",
            "capabilities": capabilities,
        }))
        .unwrap(),
    )
    .unwrap();
    profile
}

#[test]
fn app_explain_reports_a_current_plan_and_admits_a_complete_explicit_target() {
    let temporary = app_root();
    let profile = write_profile(temporary.path(), &json!(["request", "stream"]));
    let output = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .current_dir(temporary.path())
        .args(["app", "explain", "--profile"])
        .arg(profile)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["kind"], "lenso.app-explain");
    assert_eq!(report["status"], "admitted");
    assert_eq!(report["requirements"].as_array().unwrap().len(), 2);
    assert_eq!(report["reasons"], json!([]));
}

#[test]
fn app_explain_fails_closed_with_grouped_missing_capability_evidence() {
    let temporary = app_root();
    let profile = write_profile(temporary.path(), &json!(["request"]));
    let output = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .current_dir(temporary.path())
        .args(["app", "explain", "--profile"])
        .arg(profile)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "rejected");
    let missing = report["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|reason| reason["kind"] == "missing_target_capability")
        .unwrap();
    assert_eq!(missing["target_profile"], "lenso.native-rust@1");
    assert_eq!(missing["feature"], "stream");
    assert_eq!(missing["requirements"].as_array().unwrap().len(), 1);
}

#[test]
fn app_explain_rejects_unknown_target_capability_tokens_before_app_resolution() {
    let temporary = app_root();
    let profile = write_profile(temporary.path(), &json!(["future-capability"]));
    let output = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .current_dir(temporary.path())
        .args(["app", "explain", "--profile"])
        .arg(profile)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "rejected");
    assert_eq!(report["reasons"][0]["kind"], "invalid_target_profile");
}

#[test]
fn app_explain_rejects_duplicate_target_profiles_as_ambiguous() {
    let temporary = app_root();
    let first = write_profile(temporary.path(), &json!(["request", "stream"]));
    let second = temporary.path().join("duplicate-target-profile.json");
    fs::copy(&first, &second).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .current_dir(temporary.path())
        .args(["app", "explain", "--profile"])
        .arg(first)
        .arg("--profile")
        .arg(second)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        report["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason["kind"] == "duplicate_target_profile")
    );
}
