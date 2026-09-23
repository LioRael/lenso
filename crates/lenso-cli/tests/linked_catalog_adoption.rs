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
        "[package]\nname={package:?}\nversion={version:?}\nedition='2024'\n[package.metadata.lenso]\nplugin-id={plugin_id:?}\nroot-slot='tools'\n[dependencies]\nlenso='=0.5.25'\n"
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
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut patches = toml::map::Map::new();
    for entry in fs::read_dir(workspace.join("crates")).unwrap() {
        let directory = entry.unwrap().path();
        let manifest = directory.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let package: toml::Value = toml::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
        let Some(name) = package["package"]["name"].as_str() else {
            continue;
        };
        patches.insert(
            name.to_owned(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "path".to_owned(),
                toml::Value::String(directory.to_str().unwrap().to_owned()),
            )])),
        );
    }
    let config = root.join(".cargo/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(
        &config,
        toml::to_string(&toml::Value::Table(toml::map::Map::from_iter([(
            "patch".to_owned(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "crates-io".to_owned(),
                toml::Value::Table(patches),
            )])),
        )])))
        .unwrap(),
    )
    .unwrap();
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(root)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", workspace.join("target"))
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
            .arg(root.join("dist"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        if action == "check" {
            assert_eq!(report["plugin_instances"], 1);
        } else {
            assert_eq!(report["instances"].as_array().unwrap().len(), 1);
        }
    }
}

fn modified_linked_source_cannot_build(cli: &str, root: &std::path::Path) {
    let source = root.join("vendor/lenso/example.web/0.4.5/src/lib.rs");
    let original = fs::read(&source).unwrap();
    fs::write(&source, b"pub fn modified() {}\n").unwrap();
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(!built.status.success());
    assert!(
        String::from_utf8_lossy(&built.stderr).contains("linked Cargo source changed"),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    fs::write(source, original).unwrap();
}

fn generated_cargo_lock_is_not_authored_source(cli: &str, root: &std::path::Path) {
    let lock = root.join("vendor/lenso/example.web/0.4.5/Cargo.lock");
    fs::write(&lock, "version = 4\n").unwrap();
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(root)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&built.stderr).contains("linked Cargo source changed"));
    fs::remove_file(lock).unwrap();
}

fn unadopt_exact_source(cli: &str, root: &std::path::Path) {
    let removed = Command::new(cli)
        .args(["app", "unadopt", "example.web@0.4.5", "--root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    let receipt = String::from_utf8(removed.stdout).unwrap();
    let trash = receipt.trim().rsplit_once(" at ").unwrap().1;
    assert!(
        std::path::Path::new(trash)
            .join("source/Cargo.toml")
            .exists()
    );
    assert!(
        std::path::Path::new(trash)
            .join("plugin-root/default.toml")
            .exists()
    );
    let after = Command::new(cli)
        .args(["app", "discover", "--json", "--root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(after.status.success());
    let report: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    assert!(report["candidates"].as_array().unwrap().is_empty());
    assert!(!root.join("plugins/example.web").exists());
    assert!(!root.join("vendor/lenso/example.web/0.4.5").exists());
}

fn prove_removed_build_when_requested(cli: &str, root: &std::path::Path) {
    if std::env::var_os("LENSO_LINKED_BUILD_PROOF").is_none() {
        return;
    }
    let output = root.join("dist-removed");
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(root)
        .arg("--out")
        .arg(&output)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let checked = Command::new(cli)
        .args(["app", "check", "--json", "--root"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(checked.status.success());
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["plugin_instances"], 0);
    let shown = Command::new(cli)
        .args(["app", "show", "--json", "--root"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(shown.status.success());
    let report: serde_json::Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(report["instances"].as_array().unwrap().len(), 0);
    assert_eq!(
        fs::read_to_string(output.join(".lenso/host-mode")).unwrap(),
        "portable"
    );
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

fn reject_unlisted_version_and_changed_archive(
    cli: &str,
    root: &std::path::Path,
    snapshot_path: &std::path::Path,
    trust_path: &std::path::Path,
    archive: &std::path::Path,
) {
    let wrong_version = Command::new(cli)
        .args(["app", "add", "example.web@0.4.6", "--root"])
        .arg(root)
        .arg("--linked-snapshot")
        .arg(snapshot_path)
        .arg("--trust")
        .arg(trust_path)
        .arg("--crate")
        .arg(archive)
        .output()
        .unwrap();
    assert!(!wrong_version.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_version.stderr)
            .contains("exact linked Cargo release is not in this catalog")
    );
    assert!(!root.join("lenso.toml").exists());

    let wrong_archive = archive.with_file_name("wrong.crate");
    fs::write(&wrong_archive, b"not the signed archive").unwrap();
    let wrong_digest = Command::new(cli)
        .args(["app", "add", "example.web@0.4.5", "--root"])
        .arg(root)
        .arg("--linked-snapshot")
        .arg(snapshot_path)
        .arg("--trust")
        .arg(trust_path)
        .arg("--crate")
        .arg(&wrong_archive)
        .output()
        .unwrap();
    assert!(!wrong_digest.status.success());
    assert!(String::from_utf8_lossy(&wrong_digest.stderr).contains("digest"));
    assert!(!root.join("lenso.toml").exists());
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
    reject_unlisted_version_and_changed_archive(cli, &root, &snapshot_path, &trust_path, &archive);
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
    modified_linked_source_cannot_build(cli, &root);
    generated_cargo_lock_is_not_authored_source(cli, &root);
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
    unadopt_exact_source(cli, &root);
    prove_removed_build_when_requested(cli, &root);
}
