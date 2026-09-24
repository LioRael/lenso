use std::{
    fs,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use lenso_app_plan::{ExecutionClassId, authoring::PluginContract};
use lenso_plugin_bundle::{
    ExecutionAdmissionRequirementV6, SourcePluginImplementationGroupV6, SourcePluginReleaseBuildV6,
    SourcePluginVariantInputV6, SourcePluginVariantV6, build_source_plugin_release_bundle_v6,
};
use lenso_plugin_catalog::{
    Availability,
    linked_cargo::{LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot, sign},
};

fn crate_archive(source: &str) -> Vec<u8> {
    let package = "example-web-plugin";
    let version = "0.4.5";
    let manifest = format!(
        "[package]\nname={package:?}\nversion={version:?}\nedition='2024'\n[package.metadata.lenso]\nplugin-id='example.web'\nroot-slot='tools'\n[dependencies]\nlenso='=0.5.25'\n"
    );
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("src/lib.rs", source.as_bytes()),
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

fn signed_snapshot(root: &Path, archive: &[u8]) -> (std::path::PathBuf, std::path::PathBuf) {
    let key = SigningKey::from_bytes(&[82; 32]);
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
        source_url: "https://example.test/web".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-web-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: lenso_plugin_catalog::digest(archive),
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

fn bundle(
    root: &Path,
    archive: &[u8],
    variant_target: &str,
    runtime_profile: &str,
    native_variants: usize,
    requirements: &[ExecutionAdmissionRequirementV6],
) -> std::path::PathBuf {
    let source = root.join("example-web-plugin-0.4.5.crate");
    fs::write(&source, archive).unwrap();
    let script = root.join("plugin.js");
    fs::write(&script, b"export default {};").unwrap();
    let mut variants = (0..native_variants)
        .map(|index| SourcePluginVariantV6 {
            id: format!("native-{index}"),
            host_targets: vec![variant_target.to_owned()],
            input: SourcePluginVariantInputV6::CargoBuildInput {
                path: source.clone(),
                bundle_path: format!("implementations/portable/native-{index}.crate"),
                package: "example-web-plugin".to_owned(),
                version: "0.4.5".to_owned(),
            },
            entrypoint: "linked-factory".to_owned(),
            execution_class: ExecutionClassId::new("lenso.native-rust@1"),
            runtime_profile: runtime_profile.to_owned(),
            required_target_capabilities: Vec::new(),
            execution_requirements: requirements.to_vec(),
        })
        .collect::<Vec<_>>();
    variants.push(SourcePluginVariantV6 {
        id: "quickjs".to_owned(),
        host_targets: vec!["*".to_owned()],
        input: SourcePluginVariantInputV6::Artifact {
            path: script,
            bundle_path: "implementations/portable/plugin.js".to_owned(),
            media_type: "application/javascript".to_owned(),
            target: "javascript-es2023".to_owned(),
        },
        entrypoint: "plugin.js".to_owned(),
        execution_class: ExecutionClassId::new("lenso.quickjs@1"),
        runtime_profile: "lenso.quickjs@1".to_owned(),
        required_target_capabilities: Vec::new(),
        execution_requirements: Vec::new(),
    });
    let output = root.join("release.lenso-plugin");
    build_source_plugin_release_bundle_v6(&SourcePluginReleaseBuildV6 {
        contract: PluginContract::new("example.web", "0.4.5", "tools").with_authoring_version(2),
        implementations: vec![SourcePluginImplementationGroupV6 {
            id: "portable".to_owned(),
            variants,
        }],
        output: output.clone(),
    })
    .unwrap();
    output
}

fn create_app(cli: &str, root: &Path) {
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
}

fn add(
    cli: &str,
    root: &Path,
    snapshot: &Path,
    trust: &Path,
    bundle: &Path,
    version: &str,
) -> std::process::Output {
    Command::new(cli)
        .args(["app", "add", version, "--root"])
        .arg(root)
        .arg("--linked-snapshot")
        .arg(snapshot)
        .arg("--trust")
        .arg(trust)
        .arg("--bundle")
        .arg(bundle)
        .output()
        .unwrap()
}

fn assert_unmodified(root: &Path) {
    assert!(!root.join("vendor/lenso/example.web").exists());
    assert!(!root.join("plugins/example.web").exists());
    assert!(!root.join("lenso.toml").exists());
}

fn assert_v6_adopted(cli: &str, app: &Path) {
    let vendor = app.join("vendor/lenso/example.web/0.4.5");
    assert!(vendor.join("Cargo.toml").exists());
    let source_lock: serde_json::Value =
        serde_json::from_slice(&fs::read(vendor.join(".lenso-linked-source.json")).unwrap())
            .unwrap();
    assert_eq!(source_lock["v6"]["implementation_id"], "portable");
    assert_eq!(source_lock["v6"]["variant_id"], "native-0");
    assert!(
        source_lock["v6"]["bundle_manifest_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(app.join("plugins/example.web/default.toml").exists());
    let discovery = Command::new(cli)
        .args(["app", "discover", "--json", "--root"])
        .arg(app)
        .output()
        .unwrap();
    assert!(
        discovery.status.success(),
        "{}",
        String::from_utf8_lossy(&discovery.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&discovery.stdout).unwrap();
    assert_eq!(report["candidates"][0]["plugin_id"], "example.web");
    assert_eq!(report["candidates"].as_array().unwrap().len(), 1);
}

fn prove_host_build_when_requested(cli: &str, app: &Path) {
    if std::env::var_os("LENSO_LINKED_BUILD_PROOF").is_none() {
        return;
    }
    let output_dir = app.parent().unwrap().join("dist-enabled");
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(app)
        .arg("--out")
        .arg(&output_dir)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for action in ["check", "show"] {
        let output = Command::new(cli)
            .args(["app", action, "--json", "--root"])
            .arg(output_dir.join("intent"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        if action == "check" {
            assert_eq!(report["plugin_instances"], 1);
        } else {
            assert_eq!(report["instances"].as_array().unwrap().len(), 1);
        }
    }
}

fn prove_absent_from_host_when_requested(cli: &str, app: &Path, state: &str) {
    if std::env::var_os("LENSO_LINKED_BUILD_PROOF").is_none() {
        return;
    }
    let output_dir = app.parent().unwrap().join(format!("dist-{state}"));
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(app)
        .arg("--out")
        .arg(&output_dir)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let checked = Command::new(cli)
        .args(["app", "check", "--json", "--root"])
        .arg(output_dir.join("intent"))
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(report["plugin_instances"], 0);
}

#[test]
fn v6_bundle_cargo_input_uses_exact_signed_release_and_host_build_path() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let cli = env!("CARGO_BIN_EXE_lenso");
    let archive =
        crate_archive("#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n");
    let (snapshot, trust) = signed_snapshot(root, &archive);
    let bundle = bundle(
        root,
        &archive,
        lenso_engine_authoring::native_host_target(),
        "lenso.native-rust@1",
        1,
        &[],
    );
    let app = root.join("app");
    create_app(cli, &app);
    let wrong_version = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.6");
    assert!(!wrong_version.status.success());
    assert_unmodified(&app);
    let added = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    assert!(String::from_utf8_lossy(&added.stdout).contains("Host compilation"));
    assert_v6_adopted(cli, &app);

    // The catalog signs the .crate, not arbitrary Bundle metadata. A second
    // valid V6 wrapper around the same archive cannot silently replace the
    // previously reviewed exact variant selection.
    let manifest_path = bundle.join(lenso_plugin_bundle::MANIFEST_FILE);
    let original_manifest = fs::read(&manifest_path).unwrap();
    let mut alternate: serde_json::Value = serde_json::from_slice(&original_manifest).unwrap();
    alternate["implementations"][0]["variants"][1]["runtime"]["runtime_profile"] =
        "lenso.quickjs@2".into();
    fs::write(&manifest_path, serde_json::to_vec(&alternate).unwrap()).unwrap();
    let changed_wrapper = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
    assert!(!changed_wrapper.status.success());
    assert!(
        String::from_utf8_lossy(&changed_wrapper.stderr)
            .contains("existing linked Cargo source differs")
    );
    fs::write(&manifest_path, original_manifest).unwrap();

    prove_host_build_when_requested(cli, &app);
    let disabled = app.join("plugins/example.web/default.disabled");
    fs::write(&disabled, "").unwrap();
    let repeated = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
    assert!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert!(
        disabled.exists(),
        "re-adoption must preserve disabled intent"
    );
    prove_absent_from_host_when_requested(cli, &app, "disabled");
    let removed = Command::new(cli)
        .args(["app", "unadopt", "example.web@0.4.5", "--root"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!app.join("vendor/lenso/example.web/0.4.5").exists());
    assert!(!app.join("plugins/example.web").exists());
    prove_absent_from_host_when_requested(cli, &app, "removed");
}

#[test]
fn v6_bundle_ignores_incompatible_linked_abi_before_ambiguity() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let cli = env!("CARGO_BIN_EXE_lenso");
    let archive =
        crate_archive("#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n");
    let (snapshot, trust) = signed_snapshot(root, &archive);
    let bundle = bundle(
        root,
        &archive,
        lenso_engine_authoring::native_host_target(),
        "lenso.native-rust@1",
        2,
        &[],
    );
    let manifest_path = bundle.join(lenso_plugin_bundle::MANIFEST_FILE);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["implementations"][0]["variants"][1]["runtime"]["runtime_profile"] =
        "lenso.native-rust@2".into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let app = root.join("app");
    create_app(cli, &app);
    let added = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    assert_v6_adopted(cli, &app);
}

#[test]
fn v6_bundle_does_not_select_another_variant_after_signed_input_mismatch() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let cli = env!("CARGO_BIN_EXE_lenso");
    let archive =
        crate_archive("#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n");
    let (snapshot, trust) = signed_snapshot(root, &archive);
    let bundle = bundle(
        root,
        &archive,
        lenso_engine_authoring::native_host_target(),
        "lenso.native-rust@1",
        2,
        &[],
    );
    let other_archive = crate_archive("pub fn changed() {}");
    let other_digest = lenso_plugin_catalog::digest(&other_archive);
    fs::write(
        bundle.join("implementations/portable/native-1.crate"),
        &other_archive,
    )
    .unwrap();
    let manifest_path = bundle.join(lenso_plugin_bundle::MANIFEST_FILE);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let variant = &mut manifest["implementations"][0]["variants"][1];
    variant["input"]["build_input"]["digest"] = other_digest.clone().into();
    variant["input"]["build_input"]["size"] = other_archive.len().into();
    variant["runtime"]["runtime_package_revision"] = other_digest.into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    lenso_plugin_bundle::verify_bundle_directory(&bundle).unwrap();

    let app = root.join("app");
    create_app(cli, &app);
    let rejected = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("differs from signed release"));
    assert_unmodified(&app);
}

#[test]
fn v6_bundle_does_not_relax_unverified_controls_to_another_variant() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let cli = env!("CARGO_BIN_EXE_lenso");
    let archive =
        crate_archive("#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n");
    let (snapshot, trust) = signed_snapshot(root, &archive);
    let bundle = bundle(
        root,
        &archive,
        lenso_engine_authoring::native_host_target(),
        "lenso.native-rust@1",
        2,
        &[],
    );
    let manifest_path = bundle.join(lenso_plugin_bundle::MANIFEST_FILE);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["implementations"][0]["variants"][1]["execution_requirements"] =
        serde_json::json!([{ "kind": "os_sandbox" }]);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let app = root.join("app");
    create_app(cli, &app);
    let rejected = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("verified Host enforcement"));
    assert_unmodified(&app);
}

#[test]
fn v6_bundle_rejections_leave_app_unchanged_before_adoption() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let cli = env!("CARGO_BIN_EXE_lenso");
    let archive =
        crate_archive("#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n");
    let (snapshot, trust) = signed_snapshot(root, &archive);
    for (name, source, target, profile, count, requirements, expected) in [
        (
            "digest",
            "pub fn changed() {}",
            lenso_engine_authoring::native_host_target(),
            "lenso.native-rust@1",
            1,
            vec![],
            "digest",
        ),
        (
            "target",
            "pub fn unchanged() {}",
            "unsupported-target",
            "lenso.native-rust@1",
            1,
            vec![],
            "Host target",
        ),
        (
            "profile",
            "pub fn unchanged() {}",
            lenso_engine_authoring::native_host_target(),
            "lenso.native-rust@2",
            1,
            vec![],
            "Host ABI",
        ),
        (
            "ambiguous",
            "pub fn unchanged() {}",
            lenso_engine_authoring::native_host_target(),
            "lenso.native-rust@1",
            2,
            vec![],
            "ambiguous",
        ),
        (
            "sandbox",
            "pub fn unchanged() {}",
            lenso_engine_authoring::native_host_target(),
            "lenso.native-rust@1",
            1,
            vec![ExecutionAdmissionRequirementV6::OsSandbox],
            "verified Host enforcement",
        ),
    ] {
        let case = root.join(name);
        fs::create_dir(&case).unwrap();
        let bytes = if name == "digest" {
            crate_archive(source)
        } else {
            archive.clone()
        };
        let bundle = bundle(&case, &bytes, target, profile, count, &requirements);
        let app = case.join("app");
        create_app(cli, &app);
        let rejected = add(cli, &app, &snapshot, &trust, &bundle, "example.web@0.4.5");
        assert!(!rejected.status.success(), "{name} unexpectedly adopted");
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains(expected),
            "{name}: {}",
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert_unmodified(&app);
    }

    let case = root.join("tampered-archive");
    fs::create_dir(&case).unwrap();
    let tampered_bundle = bundle(
        &case,
        &archive,
        lenso_engine_authoring::native_host_target(),
        "lenso.native-rust@1",
        1,
        &[],
    );
    fs::write(
        tampered_bundle.join("implementations/portable/native-0.crate"),
        b"changed after Bundle verification",
    )
    .unwrap();
    let app = case.join("app");
    create_app(cli, &app);
    let rejected = add(
        cli,
        &app,
        &snapshot,
        &trust,
        &tampered_bundle,
        "example.web@0.4.5",
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("verify V6 Bundle"));
    assert_unmodified(&app);
}
