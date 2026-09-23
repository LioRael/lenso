use std::{
    fs,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
    let build_script = b"fn main() { assert!(std::env::var_os(\"LENSO_BUILD_SECRET_CANARY\").is_none(), \"ambient secret reached linked Cargo build script\"); println!(\"cargo:warning=LENSO_BUILD_ENV_CANARY_RAN\"); }\n";
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("build.rs", build_script.as_slice()),
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
    // This optional release gate deliberately uses registry dependencies only.
    // A workspace path patch would not prove an external consumer can build.
    assert!(!root.join(".cargo/config.toml").exists());
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(root)
        .current_dir(root)
        .env("LENSO_BUILD_SECRET_CANARY", "private-runtime-value")
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        String::from_utf8_lossy(&built.stderr).contains("LENSO_BUILD_ENV_CANARY_RAN"),
        "linked build script was not exercised"
    );
    let distribution = root.join("dist");
    assert_eq!(
        fs::read(distribution.join(".lenso/host-build.json")).unwrap(),
        fs::read(distribution.join("intent/.lenso/host-build.json")).unwrap()
    );
    for action in ["check", "show"] {
        let result = Command::new(cli)
            .args(["app", action, "--json", "--root"])
            .arg(distribution.join("intent"))
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
    assert_runtime_intent_and_start(cli, &distribution);
}

fn assert_runtime_intent_and_start(cli: &str, distribution: &std::path::Path) {
    let runtime = Command::new(cli)
        .args(["app", "show", "--runtime-json", "--host-build"])
        .arg(distribution.join(".lenso/host-build.json"))
        .arg("--root")
        .arg(distribution.join("intent"))
        .output()
        .unwrap();
    assert!(
        runtime.status.success(),
        "{}",
        String::from_utf8_lossy(&runtime.stderr)
    );
    let resolved: serde_json::Value = serde_json::from_slice(&runtime.stdout).unwrap();
    assert_eq!(
        resolved["plan"]["plugin_instances"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mirror = distribution.join("intent/.lenso/host-build.json");
    let original = fs::read(&mirror).unwrap();
    fs::write(&mirror, b"{}").unwrap();
    let rejected = Command::new(cli)
        .args(["app", "show", "--runtime-json", "--host-build"])
        .arg(distribution.join(".lenso/host-build.json"))
        .arg("--root")
        .arg(distribution.join("intent"))
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("differs"));
    fs::write(&mirror, original).unwrap();
    let started = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(distribution)
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert_external_source_bootstraps_before_start(cli, distribution);
}

fn assert_external_source_bootstraps_before_start(cli: &str, distribution: &std::path::Path) {
    let source = distribution.join("configuration-snapshot.json");
    write_external_snapshot(&source, 1, "");
    let policy = distribution.join("configuration-policy.json");
    fs::write(&policy, serde_json::to_vec(&serde_json::json!({
        "schema": "lenso.configuration-source-policy.v1",
        "source_reference": "linked-build-proof",
        "source": {"type": "file", "path": source},
        "objects": [{"plugin_id": "example.web", "instance_key": "default", "fields": ["unused"]}]
    })).unwrap()).unwrap();
    let started = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(distribution)
        .arg("--configuration-policy")
        .arg(&policy)
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert!(
        distribution
            .join("intent/.lenso/configuration-source-state.json")
            .exists()
    );
    let state_path = distribution.join("intent/.lenso/configuration-source-state.json");
    let state: serde_json::Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    assert!(
        state.get("last_activated").is_none(),
        "--check must not claim activation"
    );
    let report = config_status(cli, distribution);
    assert_eq!(report["state"], "pending_activation");
    assert_eq!(report["desired_revision"], 1);
    assert!(report["last_activated_revision"].is_null());
    start_and_observe_activation(cli, distribution, &policy, 1);
    let report = config_status(cli, distribution);
    assert_eq!(report["state"], "last_activated");
    assert_eq!(report["last_activated_revision"], 1);

    write_external_snapshot(&source, 2, "unauthorized = 'value'\n");
    let rejected = sync_external_snapshot(cli, distribution, &policy);
    assert!(!rejected.status.success());
    let report = config_status(cli, distribution);
    assert_eq!(report["desired_revision"], 1);
    assert_eq!(report["last_activated_revision"], 1);

    write_external_snapshot(&source, 2, "");
    let accepted = sync_external_snapshot(cli, distribution, &policy);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let report = config_status(cli, distribution);
    assert_eq!(report["state"], "pending_activation");
    assert_eq!(report["desired_revision"], 2);
    assert_eq!(report["last_activated_revision"], 1);
    start_and_observe_activation(cli, distribution, &policy, 2);
    let report = config_status(cli, distribution);
    assert_eq!(report["state"], "last_activated");
    assert_eq!(report["last_activated_revision"], 2);
    let missing_policy = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(distribution)
        .arg("--check")
        .output()
        .unwrap();
    assert!(!missing_policy.status.success());
    assert!(String::from_utf8_lossy(&missing_policy.stderr).contains("configuration-policy"));
}

fn write_external_snapshot(path: &std::path::Path, revision: u64, toml: &str) {
    fs::write(path, serde_json::to_vec(&serde_json::json!({
        "schema": "lenso.plugin-configuration-snapshot.v1",
        "revision": revision,
        "configurations": [{"plugin_id": "example.web", "instance_key": "default", "toml": toml}]
    })).unwrap()).unwrap();
}

fn sync_external_snapshot(
    cli: &str,
    distribution: &std::path::Path,
    policy: &std::path::Path,
) -> std::process::Output {
    Command::new(cli)
        .args(["app", "config-sync", "--root"])
        .arg(distribution)
        .arg("--policy")
        .arg(policy)
        .output()
        .unwrap()
}

fn start_and_observe_activation(
    cli: &str,
    distribution: &std::path::Path,
    policy: &std::path::Path,
    revision: u64,
) {
    let state_path = distribution.join("intent/.lenso/configuration-source-state.json");
    let mut host = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(distribution)
        .arg("--configuration-policy")
        .arg(policy)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let state: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        if state["last_activated"]["revision"] == revision {
            assert!(
                host.try_wait().unwrap().is_none(),
                "Host exited before activation was observed"
            );
            assert_eq!(state["last_activated"]["revision"], revision);
            assert_eq!(
                state["last_activated"]["plugin_root_revision"],
                state["desired"]["candidate_plugin_root_revision"]
            );
            break;
        }
        if let Some(status) = host.try_wait().unwrap() {
            panic!("Host exited before activation: {status}");
        }
        if Instant::now() >= deadline {
            host.kill().unwrap();
            host.wait().unwrap();
            panic!("Host did not activate within 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    host.kill().unwrap();
    host.wait().unwrap();
}

fn config_status(cli: &str, distribution: &std::path::Path) -> serde_json::Value {
    let output = Command::new(cli)
        .args(["app", "config-status", "--root"])
        .arg(distribution)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
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

fn modified_linked_source_cannot_unadopt(cli: &str, root: &std::path::Path) {
    let source = root.join("vendor/lenso/example.web/0.4.5/src/lib.rs");
    let original = fs::read(&source).unwrap();
    fs::write(&source, b"pub fn user_change() {}\n").unwrap();
    let rejected = Command::new(cli)
        .args(["app", "unadopt", "example.web@0.4.5", "--root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("source has user changes"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    assert!(source.exists());
    assert!(root.join("plugins/example.web/default.toml").exists());
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("lenso.toml")).unwrap()).unwrap();
    assert_eq!(
        config["plugin_sources"][0].as_str(),
        Some("vendor/lenso/example.web/0.4.5")
    );
    fs::write(source, original).unwrap();
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
        .arg(output.join("intent"))
        .output()
        .unwrap();
    assert!(checked.status.success());
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["plugin_instances"], 0);
    let shown = Command::new(cli)
        .args(["app", "show", "--json", "--root"])
        .arg(output.join("intent"))
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
    modified_linked_source_cannot_unadopt(cli, &root);
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
