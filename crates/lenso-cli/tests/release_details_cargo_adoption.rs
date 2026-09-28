use std::{
    fs,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Artifact, Availability, Distribution, DistributionKind, Release, ReleaseDetails,
    ReleaseDetailsSnapshot, Snapshot, digest, sign, sign_release_details,
};

fn crate_archive() -> Vec<u8> {
    // This is a local candidate Host proof until the root workspace cohort is published.
    let crates = fs::canonicalize(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap(),
    )
    .unwrap();
    let manifest = format!(
        "[package]\nname='example-web-plugin'\nversion='0.4.5'\nedition='2024'\n[package.metadata.lenso]\nplugin-id='example.web'\nroot-slot='tools'\n[dependencies]\nlenso={{ version='=0.5.27', path={:?} }}\nlenso-runner={{ version='=0.2.19', path={:?} }}\nlenso-native-adapter={{ version='=0.3.18', path={:?} }}\n",
        crates.join("lenso").display().to_string(),
        crates.join("lenso-runner").display().to_string(),
        crates.join("lenso-native-adapter").display().to_string(),
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
            .append_data(
                &mut header,
                format!("example-web-plugin-0.4.5/{name}"),
                bytes,
            )
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn add(
    cli: &str,
    root: &std::path::Path,
    portable: &std::path::Path,
    details: &std::path::Path,
    trust: &std::path::Path,
    archive: &std::path::Path,
) -> Output {
    Command::new(cli)
        .args(["app", "add", "example.web@0.4.5", "--root"])
        .arg(root)
        .arg("--portable-snapshot")
        .arg(portable)
        .arg("--release-details")
        .arg(details)
        .arg("--distribution")
        .arg("cargo-native")
        .arg("--trust")
        .arg(trust)
        .arg("--crate")
        .arg(archive)
        .output()
        .unwrap()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "dual-distribution identity, native build, and rollback checks require one signed release progression"
)]
fn signed_dual_distribution_cargo_adopts_and_builds_exact_host() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("app");
    let wrong_root = temporary.path().join("wrong-app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    for app in [&root, &wrong_root] {
        let created = Command::new(cli)
            .args(["app", "create"])
            .arg(app)
            .args(["--runtime", "empty"])
            .output()
            .unwrap();
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
    }

    let archive_bytes = crate_archive();
    let archive = temporary.path().join("example-web-plugin-0.4.5.crate");
    fs::write(&archive, &archive_bytes).unwrap();
    let key = SigningKey::from_bytes(&[87; 32]);
    let trust = temporary.path().join("trust.json");
    fs::write(
        &trust,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id": "dual-distribution-test",
            "key_id": "test-key",
            "public_key_hex": hex::encode(key.verifying_key().to_bytes())
        }))
        .unwrap(),
    )
    .unwrap();
    let portable_artifact = Artifact {
        url: "https://example.com/example-web-0.4.5.lenso-plugin".into(),
        digest: digest(b"portable-fixture"),
        size: 16,
        manifest_digest: digest(b"portable-manifest-fixture"),
    };
    let base = Release {
        plugin_id: "example.web".into(),
        version: "0.4.5".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Dual distribution fixture".into(),
        description: String::new(),
        presentation: None,
        source_url: "https://example.com/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        artifact: portable_artifact.clone(),
        availability: Availability::Listed,
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let portable = temporary.path().join("portable.json");
    fs::write(
        &portable,
        sign(
            &Snapshot::new(
                "dual-distribution-test".into(),
                2,
                now - 5,
                now + 3600,
                vec![base.clone()],
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let details = temporary.path().join("release-details.json");
    let release = ReleaseDetails {
        plugin_id: base.plugin_id.clone(),
        version: base.version.clone(),
        base_release_identity: base.immutable_identity().unwrap(),
        distributions: vec![
            Distribution {
                id: "portable".into(),
                kind: DistributionKind::PortableBundle,
                package: "example-web-plugin".into(),
                version: base.version.clone(),
                integrity: None,
                registry_url: None,
                artifact: Some(portable_artifact),
                targets: vec![],
            },
            Distribution {
                id: "cargo-native".into(),
                kind: DistributionKind::CargoPackage,
                package: "example-web-plugin".into(),
                version: base.version.clone(),
                integrity: Some(digest(&archive_bytes)),
                registry_url: Some("https://crates.io".into()),
                artifact: None,
                targets: vec![lenso_engine_authoring::native_host_target().into()],
            },
        ],
        documentation: vec![],
    };
    let details_snapshot = ReleaseDetailsSnapshot::new(
        "dual-distribution-test".into(),
        2,
        now - 5,
        now + 3600,
        vec![release.clone()],
    );
    let sign_details = |snapshot: &ReleaseDetailsSnapshot| {
        fs::write(
            &details,
            sign_release_details(snapshot, "test-key", &key).unwrap(),
        )
        .unwrap();
    };

    let mut wrong_base = details_snapshot.clone();
    wrong_base.releases[0].base_release_identity = digest(b"wrong-base");
    sign_details(&wrong_base);
    let rejected = add(cli, &wrong_root, &portable, &details, &trust, &archive);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("immutable base release"));
    assert!(!wrong_root.join("vendor/lenso/example.web").exists());

    sign_details(&details_snapshot);
    let wrong_archive = temporary.path().join("wrong.crate");
    fs::write(&wrong_archive, b"different archive").unwrap();
    let rejected = add(cli, &root, &portable, &details, &trust, &wrong_archive);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("digest does not match signed release details")
    );
    assert!(!root.join("vendor/lenso/example.web").exists());

    let accepted = add(cli, &root, &portable, &details, &trust, &archive);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(
        root.join("vendor/lenso/example.web/0.4.5/.lenso-linked-source.json")
            .is_file()
    );

    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(&root)
        .arg("--trust-linked-build")
        .arg(format!("example.web@0.4.5={}", digest(&archive_bytes)))
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(root.join("dist/.lenso/host-build.json").is_file());
    assert!(root.join("dist/bundles.json").is_file());
    let started = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(root.join("dist"))
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );

    let mut earlier = details_snapshot.clone();
    earlier.revision = 1;
    sign_details(&earlier);
    let rejected = add(cli, &root, &portable, &details, &trust, &archive);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("rollback"));

    sign_details(&details_snapshot);
    let mut revoked = base.clone();
    revoked.availability = Availability::Revoked;
    fs::write(
        &portable,
        sign(
            &Snapshot::new(
                "dual-distribution-test".into(),
                3,
                now - 5,
                now + 3600,
                vec![revoked],
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let rejected = add(cli, &root, &portable, &details, &trust, &archive);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("not available"));
    fs::write(
        &portable,
        sign(
            &Snapshot::new(
                "dual-distribution-test".into(),
                2,
                now - 5,
                now + 3600,
                vec![base],
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let rejected = add(cli, &root, &portable, &details, &trust, &archive);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("rollback"));
}
