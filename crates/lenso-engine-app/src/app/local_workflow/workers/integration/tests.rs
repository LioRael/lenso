use super::*;

struct Fixture {
    dir: tempfile::TempDir,
    profile: Value,
    plugin: String,
    key: String,
    manifest: String,
    artifact: String,
}

impl Fixture {
    fn new(plugin: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let manifest = digest(b"manifest");
        let artifact = digest(b"component");
        let worker = b"export default {};";
        let readme = b"Trusted Host integration.";
        fs::write(dir.path().join("worker.mjs"), worker).unwrap();
        fs::write(dir.path().join("README.md"), readme).unwrap();
        Self {
            dir,
            profile: json!({
                "schema": "lenso.workers-integration.v1",
                "plugin_id": plugin,
                "instance_key": "default",
                "authoring_version": 2,
                "world": "example:portable/entry@1.0.0",
                "manifest_digest": manifest,
                "artifact_digest": artifact,
                "runtime_version": "0.1.5",
                "files": {"worker.mjs": digest(worker), "README.md": digest(readme)},
            }),
            plugin: plugin.to_owned(),
            key: format!("{plugin}/default"),
            manifest,
            artifact,
        }
    }

    fn expected(&self) -> Expected<'_> {
        Expected {
            plugin_id: &self.plugin,
            instance_key: &self.key,
            authoring_version: 2,
            manifest_digest: &self.manifest,
            artifact_digest: &self.artifact,
            runtime_version: "0.1.5",
        }
    }

    fn write(&self) -> (PathBuf, String) {
        let path = self.dir.path().join("integration.json");
        let bytes = serde_json::to_vec(&self.profile).unwrap();
        fs::write(&path, &bytes).unwrap();
        (path, digest(&bytes))
    }

    fn load(&self) -> anyhow::Result<Integration> {
        let (path, pin) = self.write();
        Integration::load(&path, &pin, self.expected())
    }
}

#[test]
fn unrelated_plugin_owners_use_the_same_validation_and_receipt() {
    for plugin in ["org.example.weather", "net.other.inventory"] {
        let fixture = Fixture::new(plugin);
        let integration = fixture.load().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::write(destination.path().join("artifact.mjs"), "Host-owned").unwrap();
        let receipt = integration.stage(destination.path()).unwrap();
        assert_eq!(receipt["plugin_id"], plugin);
        assert_eq!(receipt["instance_key"], "default");
        assert_eq!(receipt["module_digests"], fixture.profile["files"]);
        assert_eq!(receipt["world"], integration.world());
        assert_eq!(receipt["profile_digest"], integration.profile_digest);
        assert_eq!(receipt["profile_file"], STAGED_PROFILE);
        let staged_profile = fs::read(destination.path().join(STAGED_PROFILE)).unwrap();
        assert_eq!(staged_profile, integration.profile_bytes);
        assert_eq!(digest(&staged_profile), integration.profile_digest);
        assert_eq!(receipt["manifest_digest"], fixture.manifest);
        assert_eq!(receipt["artifact_digest"], fixture.artifact);
        assert_eq!(receipt["authoring_version"], 2);
        assert_eq!(receipt["runtime_version"], "0.1.5");
        assert_eq!(
            fs::read(destination.path().join("worker.mjs")).unwrap(),
            b"export default {};"
        );
        integration.recheck().unwrap();
    }
}

#[test]
fn both_supported_authoring_and_runtime_versions_are_accepted() {
    for authoring in [1, 2] {
        for runtime in ["0.1.4", "0.1.5"] {
            let mut fixture = Fixture::new("org.example.weather");
            fixture.profile["authoring_version"] = json!(authoring);
            fixture.profile["runtime_version"] = json!(runtime);
            let (path, pin) = fixture.write();
            let mut expected = fixture.expected();
            expected.authoring_version = authoring;
            expected.runtime_version = runtime;
            Integration::load(&path, &pin, expected).unwrap();
        }
    }
}

#[test]
fn pin_is_checked_before_json_is_parsed() {
    let fixture = Fixture::new("org.example.weather");
    let (path, _) = fixture.write();
    fs::write(&path, b"not json").unwrap();
    let error = Integration::load(&path, &digest(b"other"), fixture.expected())
        .err()
        .unwrap();
    assert!(error.to_string().contains("operator's trusted digest"));
    assert!(Integration::load(&path, &digest(b"not json"), fixture.expected()).is_err());
}

#[test]
fn malformed_pins_are_rejected() {
    let fixture = Fixture::new("org.example.weather");
    let (path, _) = fixture.write();
    for pin in [
        "",
        "sha256:00",
        &format!("sha256:{}", "G".repeat(64)),
        &"a".repeat(64),
    ] {
        assert!(
            Integration::load(&path, pin, fixture.expected()).is_err(),
            "{pin}"
        );
    }
}

#[test]
fn mismatched_and_malformed_profile_fields_fail_closed() {
    for (field, value) in [
        ("schema", json!("lenso.workers-integration.v2")),
        ("plugin_id", json!("net.other.inventory")),
        ("instance_key", json!("other")),
        ("instance_key", json!("org.example.weather/default")),
        ("instance_key", json!("../default")),
        ("authoring_version", json!(1)),
        ("authoring_version", json!(3)),
        ("authoring_version", json!("2")),
        ("manifest_digest", json!(digest(b"wrong"))),
        ("manifest_digest", json!("sha256:bad")),
        ("artifact_digest", json!(digest(b"wrong"))),
        ("artifact_digest", json!("sha256:bad")),
        ("runtime_version", json!("0.1.4")),
        ("runtime_version", json!("0.1.6")),
        ("world", json!("")),
        ("world", json!(" ")),
        ("world", json!("a".repeat(257))),
        ("world", json!(null)),
        ("source", json!("export default {}")),
        ("trusted_digest", json!(digest(b"self authorization"))),
        ("files", json!([])),
    ] {
        let mut fixture = Fixture::new("org.example.weather");
        fixture.profile[field] = value.clone();
        assert!(fixture.load().is_err(), "{field}={value}");
    }
}

#[test]
fn every_required_profile_field_is_required() {
    let fields: Vec<_> = Fixture::new("org.example.weather")
        .profile
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    for field in fields {
        let mut fixture = Fixture::new("org.example.weather");
        fixture.profile.as_object_mut().unwrap().remove(&field);
        assert!(fixture.load().is_err(), "{field}");
    }
}

#[test]
fn unsupported_versions_fail_even_if_host_expected_them() {
    let mut fixture = Fixture::new("org.example.weather");
    fixture.profile["authoring_version"] = json!(3);
    let (path, pin) = fixture.write();
    let mut expected = fixture.expected();
    expected.authoring_version = 3;
    assert!(Integration::load(&path, &pin, expected).is_err());
    fixture.profile["authoring_version"] = json!(2);
    fixture.profile["runtime_version"] = json!("0.1.6");
    let (path, pin) = fixture.write();
    let mut expected = fixture.expected();
    expected.runtime_version = "0.1.6";
    assert!(Integration::load(&path, &pin, expected).is_err());
}

#[test]
fn missing_unlisted_changed_and_directory_assets_are_rejected() {
    for mutation in ["missing", "unlisted", "changed", "directory"] {
        let fixture = Fixture::new("org.example.weather");
        match mutation {
            "missing" => fs::remove_file(fixture.dir.path().join("worker.mjs")).unwrap(),
            "unlisted" => fs::write(fixture.dir.path().join("extra.mjs"), b"extra").unwrap(),
            "changed" => fs::write(fixture.dir.path().join("worker.mjs"), b"changed").unwrap(),
            "directory" => fs::create_dir(fixture.dir.path().join("extra")).unwrap(),
            _ => unreachable!(),
        }
        assert!(fixture.load().is_err(), "{mutation}");
    }
}

#[test]
fn missing_required_assets_and_malformed_module_digest_are_rejected() {
    for name in ["worker.mjs", "README.md"] {
        let mut fixture = Fixture::new("org.example.weather");
        fixture.profile["files"]
            .as_object_mut()
            .unwrap()
            .remove(name);
        assert!(fixture.load().is_err(), "{name}");
    }
    let mut fixture = Fixture::new("org.example.weather");
    fixture.profile["files"]["worker.mjs"] = json!("not a digest");
    assert!(fixture.load().is_err());
}

#[test]
fn unsafe_and_host_reserved_names_are_rejected() {
    for name in [
        "../escape.mjs",
        "/absolute.mjs",
        "sub/file.mjs",
        r"sub\file.mjs",
        "a..b.mjs",
        "méta.mjs",
        ".mjs",
        "module.js",
        "guest.mjs",
        "guest.core.mjs",
        "plan.mjs",
        "descriptor-digests.mjs",
        "artifact.mjs",
        "component-requests.mjs",
        "component-admission.mjs",
        "workers-http.mjs",
    ] {
        let mut fixture = Fixture::new("org.example.weather");
        fixture.profile["files"][name] = json!(digest(b"extra"));
        let error = fixture.load().err().unwrap().to_string();
        assert!(
            error.contains("filename") || error.contains("reserved"),
            "{name}: {error}"
        );
    }
}

#[test]
fn profile_and_module_size_limits_and_file_count_are_enforced() {
    let mut fixture = Fixture::new("org.example.weather");
    let (path, _) = fixture.write();
    let oversized = vec![b' '; MAX_PROFILE as usize + 1];
    fs::write(&path, &oversized).unwrap();
    assert!(Integration::load(&path, &digest(&oversized), fixture.expected()).is_err());
    let oversized = vec![0; MAX_FILE as usize + 1];
    fs::write(fixture.dir.path().join("worker.mjs"), &oversized).unwrap();
    fixture.profile["files"]["worker.mjs"] = json!(digest(&oversized));
    assert!(fixture.load().is_err());
    let mut fixture = Fixture::new("org.example.weather");
    for i in 0..15 {
        fixture.profile["files"][format!("extra-{i}.mjs")] = json!(digest(b"extra"));
    }
    assert!(fixture.load().is_err());
}

#[test]
fn recheck_detects_profile_module_and_closure_mutation() {
    for name in [
        "integration.json",
        "worker.mjs",
        "README.md",
        "unlisted.mjs",
    ] {
        let fixture = Fixture::new("org.example.weather");
        let integration = fixture.load().unwrap();
        fs::write(fixture.dir.path().join(name), b"changed").unwrap();
        assert!(integration.recheck().is_err(), "{name}");
    }
}

#[test]
fn stage_uses_captured_bytes_and_never_overwrites_existing_output() {
    let fixture = Fixture::new("org.example.weather");
    let integration = fixture.load().unwrap();
    fs::write(fixture.dir.path().join("worker.mjs"), b"changed").unwrap();
    let destination = tempfile::tempdir().unwrap();
    integration.stage(destination.path()).unwrap();
    assert_eq!(
        fs::read(destination.path().join("worker.mjs")).unwrap(),
        b"export default {};"
    );
    assert!(integration.stage(destination.path()).is_err());
    assert!(integration.recheck().is_err());
}

#[test]
fn stage_checks_all_collisions_before_writing_any_modules() {
    let fixture = Fixture::new("org.example.weather");
    let integration = fixture.load().unwrap();
    for name in ["worker.mjs", STAGED_PROFILE] {
        let destination = tempfile::tempdir().unwrap();
        fs::write(destination.path().join(name), b"existing").unwrap();
        assert!(integration.stage(destination.path()).is_err());
        assert!(!destination.path().join("README.md").exists());
        assert_eq!(
            fs::read(destination.path().join(name)).unwrap(),
            b"existing"
        );
    }
}

#[cfg(unix)]
#[test]
fn source_and_destination_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    for name in ["integration.json", "worker.mjs", "README.md"] {
        let fixture = Fixture::new("org.example.weather");
        let (path, pin) = fixture.write();
        let target = tempfile::NamedTempFile::new().unwrap();
        fs::copy(fixture.dir.path().join(name), target.path()).unwrap();
        fs::remove_file(fixture.dir.path().join(name)).unwrap();
        symlink(target.path(), fixture.dir.path().join(name)).unwrap();
        assert!(
            Integration::load(&path, &pin, fixture.expected()).is_err(),
            "{name}"
        );
    }
    let fixture = Fixture::new("org.example.weather");
    let integration = fixture.load().unwrap();
    let destination = tempfile::tempdir().unwrap();
    symlink("missing", destination.path().join("worker.mjs")).unwrap();
    assert!(integration.stage(destination.path()).is_err());
}
