use std::{fs, process::Command};

use lenso_app_plan::authoring::{
    HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
};

#[test]
fn external_file_source_reconciles_through_public_app_commands() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".lenso")).unwrap();
    fs::create_dir_all(root.path().join("intent/.lenso")).unwrap();
    let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
        .with_configuration_schema(serde_json::json!({
            "type": "object",
            "properties": {"greeting": {"type": "string"}},
            "additionalProperties": false
        }));
    let host = HostCatalog::new(
        [HostSlot::one("agent")],
        [HostPluginRelease::new(descriptor)],
        [HostDefaultPlugin::new("example.agent", "default")],
    );
    fs::write(
        root.path().join(".lenso/host-catalog.json"),
        serde_json::to_vec(&host).unwrap(),
    )
    .unwrap();
    let source = root.path().join("snapshot.json");
    fs::write(
        &source,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.plugin-configuration-snapshot.v1",
            "revision": 1,
            "configurations": [{
                "plugin_id": "example.agent", "instance_key": "default",
                "toml": "greeting = 'hello'\n"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let policy = root.path().join("policy.json");
    fs::write(
        &policy,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.configuration-source-policy.v1",
            "source_reference": "development",
            "source": {"type": "file", "path": source},
            "objects": [{
                "plugin_id": "example.agent", "instance_key": "default",
                "fields": ["greeting"]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let cli = env!("CARGO_BIN_EXE_lenso");
    let synced = Command::new(cli)
        .args(["app", "config-sync", "--root"])
        .arg(root.path())
        .arg("--policy")
        .arg(&policy)
        .output()
        .unwrap();
    assert!(
        synced.status.success(),
        "{}",
        String::from_utf8_lossy(&synced.stderr)
    );
    let checked = Command::new(cli)
        .args(["app", "check", "--root"])
        .arg(root.path().join("intent"))
        .output()
        .unwrap();
    assert!(checked.status.success());
    let shown = Command::new(cli)
        .args(["app", "show", "--root"])
        .arg(root.path().join("intent"))
        .output()
        .unwrap();
    assert!(shown.status.success());
    assert!(
        fs::read_to_string(
            root.path()
                .join("intent/plugins/example.agent/default.toml")
        )
        .unwrap()
        .contains("hello")
    );
    let missing_policy = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(root.path())
        .arg("--check")
        .output()
        .unwrap();
    assert!(!missing_policy.status.success());
    assert!(String::from_utf8_lossy(&missing_policy.stderr).contains("configuration-policy"));
}
