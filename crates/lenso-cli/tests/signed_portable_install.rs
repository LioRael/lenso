//! Public CLI proof for an exact signed Portable release.

use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use lenso_app_plan::authoring::{HostCatalog, HostSlot};
use lenso_engine_authoring::bundle_archive::{PluginArchiveIdentity, VerifiedPluginArchive};
use lenso_plugin_catalog::{Artifact, Availability, Release, Snapshot, digest, sign};

#[test]
#[ignore = "requires LENSO_TEST_PLUGIN_ARCHIVE from a real CLI pack"]
fn exact_signed_portable_archive_is_adopted_and_checked() {
    let archive_path = std::env::var("LENSO_TEST_PLUGIN_ARCHIVE").unwrap();
    let bytes = fs::read(&archive_path).unwrap();
    let verified = VerifiedPluginArchive::read(
        bytes.as_slice(),
        &PluginArchiveIdentity {
            size: bytes.len() as u64,
            sha256: digest(&bytes),
        },
    )
    .unwrap();
    let plugin_id = verified.bundle().plugin_id.clone();
    let version = verified.bundle().release_version.clone();
    let manifest_digest = verified.bundle().manifest_digest.clone();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("app");
    fs::create_dir_all(root.join(".lenso")).unwrap();
    let host = HostCatalog::new([HostSlot::many("tool-providers")], [], []);
    fs::write(
        root.join(".lenso/host-catalog.json"),
        serde_json::to_vec(&host).unwrap(),
    )
    .unwrap();
    let key = SigningKey::from_bytes(&[41; 32]);
    let trust = temporary.path().join("trust.json");
    fs::write(
        &trust,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id":"portable-test", "key_id":"test-key",
            "public_key_hex": hex::encode(key.verifying_key().as_bytes())
        }))
        .unwrap(),
    )
    .unwrap();
    let release = Release {
        plugin_id: plugin_id.clone(),
        version: version.clone(),
        publisher_id: "test".into(),
        title: "Portable test".into(),
        summary: "Exact signed archive".into(),
        description: String::new(),
        presentation: None,
        source_url: "https://example.com/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        availability: Availability::Listed,
        artifact: Artifact {
            url: "https://example.com/plugin.lenso-plugin".into(),
            digest: digest(&bytes),
            size: bytes.len() as u64,
            manifest_digest,
        },
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let snapshot = temporary.path().join("snapshot.json");
    fs::write(
        &snapshot,
        sign(
            &Snapshot::new(
                "portable-test".into(),
                2,
                now - 60,
                now + 600,
                vec![release.clone()],
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();

    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args([
                "plugins",
                "signed-install",
                &plugin_id,
                "--version",
                &version,
                "--snapshot",
                snapshot.to_str().unwrap(),
                "--trust",
                trust.to_str().unwrap(),
                "--archive",
                &archive_path,
                "--root",
                root.to_str().unwrap(),
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    let installed = run(&[]);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert!(
        root.join("plugins")
            .join(&plugin_id)
            .join("plugin.lenso-plugin/lenso-plugin.json")
            .is_file()
    );
    let configured = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args([
            "plugins",
            "configure",
            &plugin_id,
            "default",
            "--root",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        configured.status.success(),
        "{}",
        String::from_utf8_lossy(&configured.stderr)
    );
    let checked = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "check", "--root", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let shown = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "show", "--root", root.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        shown.status.success(),
        "{}",
        String::from_utf8_lossy(&shown.stderr)
    );
    assert!(String::from_utf8_lossy(&shown.stdout).contains(&plugin_id));

    let retry = run(&[]);
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert!(String::from_utf8_lossy(&retry.stdout).contains("already installed"));

    let earlier = Snapshot::new(
        "portable-test".into(),
        1,
        now - 60,
        now + 600,
        vec![release.clone()],
    );
    fs::write(&snapshot, sign(&earlier, "test-key", &key).unwrap()).unwrap();
    let rejected = run(&["--replace"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("rollback"));

    fs::write(
        &snapshot,
        sign(
            &Snapshot::new(
                "portable-test".into(),
                2,
                now - 60,
                now + 600,
                vec![release.clone()],
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let source_root = temporary.path().join("source-app");
    fs::create_dir(&source_root).unwrap();
    let source = format!("{plugin_id}@{version}");
    let add = || {
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "add", &source, "--portable-snapshot"])
            .arg(&snapshot)
            .arg("--trust")
            .arg(&trust)
            .arg("--archive")
            .arg(&archive_path)
            .arg("--root")
            .arg(&source_root)
            .output()
            .unwrap()
    };
    let adopted = add();
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    assert!(
        source_root
            .join("vendor/lenso/portable")
            .join(&plugin_id)
            .join(format!("{version}.lenso-plugin"))
            .is_file()
    );
    let discovered = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "discover", "--root"])
        .arg(&source_root)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        discovered.status.success(),
        "{}",
        String::from_utf8_lossy(&discovered.stderr)
    );
    assert!(String::from_utf8_lossy(&discovered.stdout).contains(&plugin_id));
    let retry = add();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );

    let config_before = fs::read(source_root.join("lenso.toml")).unwrap();
    let intent_before = fs::read(
        source_root
            .join("plugins")
            .join(&plugin_id)
            .join("default.toml"),
    )
    .unwrap();
    let retry = add();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(
        fs::read(source_root.join("lenso.toml")).unwrap(),
        config_before
    );
    assert_eq!(
        fs::read(
            source_root
                .join("plugins")
                .join(&plugin_id)
                .join("default.toml")
        )
        .unwrap(),
        intent_before
    );

    #[cfg(unix)]
    let source_for_build = {
        let alias = temporary.path().join("source-app-alias");
        std::os::unix::fs::symlink(&source_root, &alias).unwrap();
        alias
    };
    #[cfg(not(unix))]
    let source_for_build = source_root.clone();
    let distribution = temporary.path().join("portable-distribution");
    let built = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "build", "--root"])
        .arg(&source_for_build)
        .arg("--out")
        .arg(&distribution)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "source App build: {}\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let built_root = distribution.join("intent");
    let built_check = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "check", "--root"])
        .arg(&built_root)
        .output()
        .unwrap();
    assert!(
        built_check.status.success(),
        "built App check: {}",
        String::from_utf8_lossy(&built_check.stderr)
    );
    let built_show = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "show", "--root"])
        .arg(&built_root)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        built_show.status.success(),
        "built App show: {}",
        String::from_utf8_lossy(&built_show.stderr)
    );
    assert!(String::from_utf8_lossy(&built_show.stdout).contains(&plugin_id));
    if std::env::var_os("LENSO_TEST_SKIP_READY").is_none() {
        #[cfg(target_os = "linux")]
        let mut ready_command = {
            let mut command = Command::new("timeout");
            command.args(["--signal=TERM", "60s", env!("CARGO_BIN_EXE_lenso")]);
            command
        };
        #[cfg(not(target_os = "linux"))]
        let mut ready_command = Command::new(env!("CARGO_BIN_EXE_lenso"));
        let ready = ready_command
            .args(["app", "start", "--from"])
            .arg(&distribution)
            .arg("--check")
            .output()
            .unwrap();
        assert!(
            ready.status.success(),
            "Host Ready: {}\n{}",
            String::from_utf8_lossy(&ready.stdout),
            String::from_utf8_lossy(&ready.stderr)
        );
    }

    let adopted_archive = source_root
        .join("vendor/lenso/portable")
        .join(&plugin_id)
        .join(format!("{version}.lenso-plugin"));
    let adopted_bytes = fs::read(&adopted_archive).unwrap();
    let mut changed_bytes = adopted_bytes.clone();
    let last = changed_bytes.len() - 1;
    changed_bytes[last] ^= 1;
    fs::write(&adopted_archive, changed_bytes).unwrap();
    let drift_output = temporary.path().join("drift-distribution");
    let archive_drift = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "build", "--root"])
        .arg(&source_root)
        .arg("--out")
        .arg(&drift_output)
        .output()
        .unwrap();
    assert!(
        !archive_drift.status.success(),
        "modified archive was accepted"
    );
    assert!(!drift_output.exists());
    fs::write(&adopted_archive, &adopted_bytes).unwrap();

    let source_lock = adopted_archive
        .parent()
        .unwrap()
        .join(format!("{version}.lenso-plugin.lock.json"));
    let lock_bytes = fs::read(&source_lock).unwrap();
    let mut changed_lock: serde_json::Value = serde_json::from_slice(&lock_bytes).unwrap();
    changed_lock["artifact"]["digest"] = format!("sha256:{}", "0".repeat(64)).into();
    fs::write(&source_lock, serde_json::to_vec(&changed_lock).unwrap()).unwrap();
    let lock_drift = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "build", "--root"])
        .arg(&source_root)
        .arg("--out")
        .arg(&drift_output)
        .output()
        .unwrap();
    assert!(
        !lock_drift.status.success(),
        "modified source lock was accepted"
    );
    assert!(!drift_output.exists());
    fs::write(&source_lock, lock_bytes).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let outside = temporary.path().join("outside-portable-source");
        let source_directory = adopted_archive.parent().unwrap();
        fs::rename(source_directory, &outside).unwrap();
        symlink(&outside, source_directory).unwrap();
        let escaped = Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "build", "--root"])
            .arg(&source_root)
            .arg("--out")
            .arg(&drift_output)
            .output()
            .unwrap();
        assert!(!escaped.status.success());
        assert!(!drift_output.exists());
        fs::remove_file(source_directory).unwrap();
        fs::rename(&outside, source_directory).unwrap();

        let outside_archive = temporary.path().join("outside-portable.lenso-plugin");
        fs::rename(&adopted_archive, &outside_archive).unwrap();
        symlink(&outside_archive, &adopted_archive).unwrap();
        let escaped = Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "build", "--root"])
            .arg(&source_root)
            .arg("--out")
            .arg(&drift_output)
            .output()
            .unwrap();
        assert!(!escaped.status.success());
        assert!(!drift_output.exists());
        fs::remove_file(&adopted_archive).unwrap();
        fs::rename(outside_archive, &adopted_archive).unwrap();
    }

    let intent_path = source_root
        .join("plugins")
        .join(&plugin_id)
        .join("default.toml");
    fs::write(&intent_path, "# App-owned edit\n").unwrap();
    let config_before_unadopt = fs::read(source_root.join("lenso.toml")).unwrap();
    let refused_unadopt = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "unadopt", &source, "--portable", "--root"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(!refused_unadopt.status.success());
    assert_eq!(
        fs::read(source_root.join("lenso.toml")).unwrap(),
        config_before_unadopt
    );
    assert_eq!(
        fs::read_to_string(&intent_path).unwrap(),
        "# App-owned edit\n"
    );
    fs::write(&intent_path, &intent_before).unwrap();

    let unadopted = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "unadopt", &source, "--portable", "--root"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(
        unadopted.status.success(),
        "{}",
        String::from_utf8_lossy(&unadopted.stderr)
    );
    let discovered_after = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "discover", "--root"])
        .arg(&source_root)
        .arg("--json")
        .output()
        .unwrap();
    assert!(discovered_after.status.success());
    assert!(!String::from_utf8_lossy(&discovered_after.stdout).contains(&plugin_id));
    let without_portable = temporary.path().join("distribution-without-portable");
    let rebuilt = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "build", "--root"])
        .arg(&source_root)
        .arg("--out")
        .arg(&without_portable)
        .output()
        .unwrap();
    assert!(
        rebuilt.status.success(),
        "unadopted App build: {}\n{}",
        String::from_utf8_lossy(&rebuilt.stdout),
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    let checked_without = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "check", "--root"])
        .arg(without_portable.join("intent"))
        .output()
        .unwrap();
    assert!(checked_without.status.success());
    let shown_without = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "show", "--root"])
        .arg(without_portable.join("intent"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(shown_without.status.success());
    assert!(!String::from_utf8_lossy(&shown_without.stdout).contains(&plugin_id));
    let unadopt_retry = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "unadopt", &source, "--portable", "--root"])
        .arg(&source_root)
        .output()
        .unwrap();
    assert!(unadopt_retry.status.success());

    let signed =
        |candidate: Release, revision: u64, issued_at: u64, expires_at: u64, key_id: &str| {
            sign(
                &Snapshot::new(
                    "portable-test".into(),
                    revision,
                    issued_at,
                    expires_at,
                    vec![candidate],
                ),
                key_id,
                &key,
            )
            .unwrap()
        };
    let good = signed(release.clone(), 2, now - 60, now + 600, "test-key");
    let mut changed_artifact = release.clone();
    changed_artifact.artifact.digest = format!("sha256:{}", "0".repeat(64));
    let mut changed_manifest = release.clone();
    changed_manifest.artifact.manifest_digest = format!("sha256:{}", "0".repeat(64));
    let mut revoked = release.clone();
    revoked.availability = Availability::Revoked;
    let mut raw_tamper: serde_json::Value = serde_json::from_slice(&good).unwrap();
    let mut payload = raw_tamper["payload_base64"].as_str().unwrap().to_owned();
    payload.replace_range(0..1, if payload.starts_with('e') { "f" } else { "e" });
    raw_tamper["payload_base64"] = payload.into();
    let invalid = [
        (
            "raw-payload-tamper",
            serde_json::to_vec(&raw_tamper).unwrap(),
        ),
        (
            "unknown-key",
            signed(release.clone(), 2, now - 60, now + 600, "unknown-key"),
        ),
        (
            "expired",
            signed(release.clone(), 2, now - 3600, now - 1, "test-key"),
        ),
        (
            "artifact-digest",
            signed(changed_artifact, 2, now - 60, now + 600, "test-key"),
        ),
        (
            "manifest-digest",
            signed(changed_manifest, 2, now - 60, now + 600, "test-key"),
        ),
        (
            "revoked",
            signed(revoked, 2, now - 60, now + 600, "test-key"),
        ),
    ];
    for (name, snapshot_bytes) in invalid {
        let invalid_root = temporary.path().join(name);
        fs::create_dir(&invalid_root).unwrap();
        fs::write(&snapshot, snapshot_bytes).unwrap();
        let attempt = Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "add", &source, "--portable-snapshot"])
            .arg(&snapshot)
            .arg("--trust")
            .arg(&trust)
            .arg("--archive")
            .arg(&archive_path)
            .arg("--root")
            .arg(&invalid_root)
            .output()
            .unwrap();
        assert!(
            !attempt.status.success(),
            "{name} unexpectedly accepted: {}",
            String::from_utf8_lossy(&attempt.stdout)
        );
        for relative in [".lenso", "vendor", "plugins", "lenso.toml"] {
            assert!(
                !invalid_root.join(relative).exists(),
                "{name} left {relative} behind: {}",
                String::from_utf8_lossy(&attempt.stderr)
            );
        }
    }
}
