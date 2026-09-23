use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Availability,
    linked_cargo::{LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot, sign},
};

fn crate_archive(package: &str, version: &str, plugin_id: &str) -> Vec<u8> {
    let manifest = format!(
        "[package]\nname={package:?}\nversion={version:?}\nedition='2024'\n[package.metadata.lenso]\nplugin-id={plugin_id:?}\nroot-slot='tools'\n[dependencies]\nlenso='=0.5.23'\n"
    );
    let source = b"#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n";
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("src/lib.rs", source.as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("{package}-{version}/{name}"), bytes)
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn prove_build_when_requested(cli: &str, root: &std::path::Path) {
    if std::env::var_os("LENSO_LINKED_BUILD_PROOF").is_none() {
        return;
    }
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for action in ["check", "show"] {
        let result = Command::new(cli)
            .args(["app", action, "--json", "--root"])
            .arg(root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

fn assert_host_provided_rejected(
    cli: &str,
    temp: &std::path::Path,
    snapshot_path: &std::path::Path,
    trust_path: &std::path::Path,
    archive: &std::path::Path,
    mut snapshot: LinkedCargoSnapshot,
    key: &SigningKey,
) {
    let host_root = temp.join("host-provided");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&host_root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    snapshot.releases[0].integration = LinkedCargoIntegration::HostProvided;
    fs::write(snapshot_path, sign(&snapshot, "test-key", key).unwrap()).unwrap();
    let rejected = Command::new(cli)
        .args(["app", "add", "example.web@0.4.5", "--root"])
        .arg(&host_root)
        .arg("--linked-snapshot")
        .arg(snapshot_path)
        .arg("--trust")
        .arg(trust_path)
        .arg("--crate")
        .arg(archive)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Host integration"));
    assert!(!host_root.join("lenso.toml").exists());
}

#[test]
fn linked_catalog_target_mismatch_leaves_app_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let key = SigningKey::from_bytes(&[61; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let release = LinkedCargoRelease {
        plugin_id: "example.web".into(),
        version: "0.4.5".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Native Web Plugin".into(),
        source_url: "https://example.com/web".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-web-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: lenso_plugin_catalog::digest(b"exact crate"),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec!["wasm32-unknown-unknown".into()],
        availability: Availability::Listed,
        documentation: Vec::new(),
    };
    let snapshot =
        LinkedCargoSnapshot::new("test-catalog".into(), 1, now - 1, now + 3600, vec![release]);
    let snapshot_path = temp.path().join("snapshot.json");
    fs::write(&snapshot_path, sign(&snapshot, "test-key", &key).unwrap()).unwrap();
    let trust_path = temp.path().join("trust.json");
    fs::write(
        &trust_path,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id": "test-catalog", "key_id": "test-key",
            "public_key_hex": hex::encode(key.verifying_key().to_bytes())
        }))
        .unwrap(),
    )
    .unwrap();
    let failed = Command::new(cli)
        .args(["app", "add", "example.web@0.4.5", "--root"])
        .arg(&root)
        .arg("--linked-snapshot")
        .arg(&snapshot_path)
        .arg("--trust")
        .arg(&trust_path)
        .arg("--crate")
        .arg(temp.path().join("plugin.crate"))
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("target"),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(!root.join("lenso.toml").exists());
    assert!(!root.join("app/example.web").exists());
}

#[test]
fn linked_catalog_adds_exact_source_once_and_discovers_it() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let bytes = crate_archive("example-web-plugin", "0.4.5", "example.web");
    let archive = temp.path().join("plugin.crate");
    fs::write(&archive, &bytes).unwrap();
    let key = SigningKey::from_bytes(&[62; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let release = LinkedCargoRelease {
        plugin_id: "example.web".into(),
        version: "0.4.5".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Native Web Plugin".into(),
        source_url: "https://example.com/web".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-web-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: lenso_plugin_catalog::digest(&bytes),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec![lenso_engine_authoring::native_host_target().into()],
        availability: Availability::Listed,
        documentation: Vec::new(),
    };
    let snapshot =
        LinkedCargoSnapshot::new("test-catalog".into(), 1, now - 1, now + 3600, vec![release]);
    let snapshot_path = temp.path().join("snapshot.json");
    fs::write(&snapshot_path, sign(&snapshot, "test-key", &key).unwrap()).unwrap();
    let trust_path = temp.path().join("trust.json");
    fs::write(&trust_path, serde_json::to_vec(&serde_json::json!({
        "catalog_id": "test-catalog", "key_id": "test-key", "public_key_hex": hex::encode(key.verifying_key().to_bytes())
    })).unwrap()).unwrap();
    for _ in 0..2 {
        let added = Command::new(cli)
            .args(["app", "add", "example.web@0.4.5", "--root"])
            .arg(&root)
            .arg("--linked-snapshot")
            .arg(&snapshot_path)
            .arg("--trust")
            .arg(&trust_path)
            .arg("--crate")
            .arg(&archive)
            .output()
            .unwrap();
        assert!(
            added.status.success(),
            "{}",
            String::from_utf8_lossy(&added.stderr)
        );
    }
    let discovery = Command::new(cli)
        .args(["app", "discover", "--json", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        discovery.status.success(),
        "{}",
        String::from_utf8_lossy(&discovery.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&discovery.stdout).unwrap();
    assert_eq!(report["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(report["candidates"][0]["plugin_id"], "example.web");
    let intent = root.join("plugins/example.web/default.toml");
    assert!(intent.exists());
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("lenso.toml")).unwrap()).unwrap();
    assert_eq!(config["plugin_sources"].as_array().unwrap().len(), 1);
    assert_eq!(
        config["plugin_sources"][0].as_str(),
        Some("vendor/lenso/example.web/0.4.5")
    );
    prove_build_when_requested(cli, &root);
    assert_host_provided_rejected(
        cli,
        temp.path(),
        &snapshot_path,
        &trust_path,
        &archive,
        snapshot,
        &key,
    );
}
