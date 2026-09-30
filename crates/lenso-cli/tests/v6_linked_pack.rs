use std::{
    fmt::Write as _,
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use lenso_plugin_bundle::{PluginManifest, PluginVariantInputV6, read_bundle_manifest};
use lenso_plugin_catalog::{
    Availability,
    linked_cargo::{LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot, sign},
};

fn package_archive(root: &std::path::Path) -> std::path::PathBuf {
    let package = "example-linked-web";
    let version = "0.4.5";
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .unwrap();
    assert!(revision.status.success());
    let revision = String::from_utf8(revision.stdout).unwrap();
    let revision = revision.trim();
    let url = format!("file://{}", repository.display());
    let mut patches = String::new();
    for name in [
        "lenso",
        "lenso-app-plan",
        "lenso-kernel",
        "lenso-native-adapter",
        "lenso-native-adapter-macros",
        "lenso-web-host",
        "lenso-capability-http-endpoint",
        "lenso-capability-http-stream-endpoint",
        "lenso-capability-websocket-endpoint",
        "lenso-web-ingress-plugin",
        "lenso-runner",
        "lenso-contract-runtime",
    ] {
        writeln!(&mut patches, "{name}={{git='{url}',rev='{revision}'}}").unwrap();
    }
    let manifest = format!(
        "[package]\nname={package:?}\nversion={version:?}\nedition='2024'\n[package.metadata.lenso]\nplugin-id='example.linked-web'\nroot-slot='web'\n[dependencies]\nlenso={{version='=0.5.28',git='{url}',rev='{revision}'}}\n[patch.crates-io]\n{patches}"
    );
    let source = b"#[lenso::plugin(consumer)]\n#[derive(Clone, Debug, Default)]\nstruct Web { value: std::rc::Rc<std::cell::Cell<u8>> }\npub fn link() { link_plugin(); }\n";
    fs::write(root.join("Cargo.toml"), &manifest).unwrap();
    let archive = root.join(format!("{package}-{version}.crate"));
    let file = fs::File::create(&archive).unwrap();
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
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
    builder.into_inner().unwrap().finish().unwrap();
    archive
}

fn signed_snapshot(
    root: &std::path::Path,
    archive: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let key = SigningKey::from_bytes(&[93; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let release = LinkedCargoRelease {
        plugin_id: "example.linked-web".into(),
        version: "0.4.5".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Linked Web Plugin".into(),
        source_url: "https://example.test/web".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-linked-web".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: lenso_plugin_catalog::digest(&fs::read(archive).unwrap()),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec![lenso_engine_authoring::native_host_target().into()],
        availability: Availability::Listed,
        documentation: Vec::new(),
    };
    let snapshot =
        LinkedCargoSnapshot::new("test-catalog".into(), 1, now - 1, now + 3600, vec![release]);
    let snapshot_path = root.join("snapshot.json");
    fs::write(&snapshot_path, sign(&snapshot, "test-key", &key).unwrap()).unwrap();
    let trust_path = root.join("trust.json");
    fs::write(
        &trust_path,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id": "test-catalog", "key_id": "test-key",
            "public_key_hex": hex::encode(key.verifying_key().to_bytes()),
        }))
        .unwrap(),
    )
    .unwrap();
    (snapshot_path, trust_path)
}

fn assert_mismatched_contract_fails_build(
    root: &Path,
    output: &Path,
    snapshot: &Path,
    trust: &Path,
    crate_digest: &str,
) {
    let mismatched = root.join("mismatched-contract");
    let input = mismatched.join("implementations/native/example-linked-web-0.4.5.crate");
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    lenso_app_authoring::bundle_archive::with_bundle_directory(output, |directory| {
        fs::copy(
            directory.join(lenso_plugin_bundle::MANIFEST_FILE),
            mismatched.join(lenso_plugin_bundle::MANIFEST_FILE),
        )?;
        fs::copy(
            directory.join("implementations/native/example-linked-web-0.4.5.crate"),
            &input,
        )?;
        Ok(())
    })
    .unwrap();
    let manifest_path = mismatched.join(lenso_plugin_bundle::MANIFEST_FILE);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["contract"]["configuration_schema"] = serde_json::json!({
        "type": "object", "properties": {"ghost": {"type": "string"}}
    });
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    lenso_plugin_bundle::verify_bundle_directory(&mismatched).unwrap();

    let forged_app = root.join("forged-app");
    let created = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "create"])
        .arg(&forged_app)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let adopted = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "add", "example.linked-web@0.4.5", "--root"])
        .arg(&forged_app)
        .arg("--linked-snapshot")
        .arg(snapshot)
        .arg("--trust")
        .arg(trust)
        .arg("--bundle")
        .arg(&mismatched)
        .output()
        .unwrap();
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    let built = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .current_dir(root)
        .args(["app", "build", "--root", "forged-app", "--out"])
        .arg(root.join("forged-output"))
        .arg("--trust-linked-build")
        .arg(format!("example.linked-web@0.4.5={crate_digest}"))
        .output()
        .unwrap();
    assert!(!built.status.success());
    assert!(
        String::from_utf8_lossy(&built.stderr).contains("V6 Bundle Contract"),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
}

#[test]
fn ordinary_plugin_pack_emits_a_verified_native_cargo_input_from_exact_crate() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let archive = package_archive(&source);
    let output = root.path().join("release.lenso-plugin");
    let packed = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["plugin", "pack", "--repo-root"])
        .arg(&source)
        .arg("--linked-crate")
        .arg(&archive)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        packed.status.success(),
        "{}",
        String::from_utf8_lossy(&packed.stderr)
    );
    let manifest =
        lenso_app_authoring::bundle_archive::with_bundle_directory(&output, |directory| {
            read_bundle_manifest(directory).map_err(Into::into)
        })
        .unwrap();
    let PluginManifest::V6(manifest) = manifest else {
        panic!("linked Cargo pack must emit V6");
    };
    assert_eq!(manifest.contract.plugin_id(), "example.linked-web");
    assert_eq!(manifest.contract.release_version(), "0.4.5");
    assert_eq!(manifest.contract.root_slot(), "web");
    assert_eq!(
        manifest.contract.configuration_schema().unwrap()["type"],
        "object"
    );
    assert_eq!(manifest.implementations.len(), 1);
    let variant = &manifest.implementations[0].variants[0];
    assert_eq!(variant.runtime.entrypoint(), "default");
    assert_eq!(variant.runtime.runtime_profile(), "lenso.native-rust@1");
    assert!(variant.execution_requirements.is_empty());
    let PluginVariantInputV6::CargoBuildInput { build_input } = &variant.input else {
        panic!("linked Cargo input must not be a runtime Artifact");
    };
    assert_eq!(build_input.package, "example-linked-web");
    assert_eq!(build_input.version, "0.4.5");
    assert_eq!(build_input.size, fs::metadata(&archive).unwrap().len());
    assert_eq!(
        build_input.digest,
        lenso_plugin_catalog::digest(&fs::read(&archive).unwrap())
    );

    let (snapshot, trust) = signed_snapshot(root.path(), &archive);
    let app = root.path().join("app");
    let created = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "create"])
        .arg(&app)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let adopted = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "add", "example.linked-web@0.4.5", "--root"])
        .arg(&app)
        .arg("--linked-snapshot")
        .arg(&snapshot)
        .arg("--trust")
        .arg(&trust)
        .arg("--bundle")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    let source_lock: serde_json::Value = serde_json::from_slice(
        &fs::read(app.join("vendor/lenso/example.linked-web/0.4.5/.lenso-linked-source.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(source_lock["v6"]["implementation_id"], "native");
    assert_eq!(source_lock["v6"]["variant_id"], "cargo");

    assert_mismatched_contract_fails_build(
        root.path(),
        &output,
        &snapshot,
        &trust,
        &build_input.digest,
    );
}

#[test]
fn linked_pack_rejects_invalid_archive_before_any_host_build() {
    let root = tempfile::tempdir().unwrap();
    let archive = package_archive(root.path());
    fs::write(&archive, b"not a Cargo archive").unwrap();
    let output = root.path().join("release.lenso-plugin");
    let packed = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["plugin", "pack", "--repo-root"])
        .arg(root.path())
        .arg("--linked-crate")
        .arg(&archive)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!packed.status.success());
    assert!(
        String::from_utf8_lossy(&packed.stderr)
            .contains("verify exact linked Cargo archive before compiling it")
    );
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn linked_pack_rejects_a_symlinked_archive() {
    let root = tempfile::tempdir().unwrap();
    let archive = package_archive(root.path());
    let link = root.path().join("linked.crate");
    std::os::unix::fs::symlink(archive, &link).unwrap();
    let packed = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["plugin", "pack", "--repo-root"])
        .arg(root.path())
        .arg("--linked-crate")
        .arg(&link)
        .output()
        .unwrap();
    assert!(!packed.status.success());
    assert!(
        !root
            .path()
            .join("dist/example.linked-web-0.4.5.lenso-plugin")
            .exists()
    );
}
