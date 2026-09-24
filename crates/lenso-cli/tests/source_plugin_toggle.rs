use std::{fs, path::Path, process::Command};

fn source_candidate(root: &Path, directory: &str, plugin_id: &str) {
    let project = root.join(directory);
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        format!(
            "[package]\nname = 'fixture'\nversion = '1.0.0'\n[package.metadata.lenso]\nplugin-id = '{plugin_id}'\n[package.metadata.lenso-cli]\nruntime = 'process'\n"
        ),
    )
    .unwrap();
}

fn toggle(root: &Path, action: &str, plugin_id: &str, instance: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["plugins", action, plugin_id, instance, "--root"])
        .arg(root)
        .output()
        .unwrap()
}

#[test]
fn source_toggle_writes_only_app_owned_default_for_next_build() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let distribution = temp.path().join("dist");
    let cli = env!("CARGO_BIN_EXE_lenso");

    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&source)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(&source)
        .arg("--out")
        .arg(&distribution)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let host_before = fs::read(distribution.join(".lenso/host-build.json")).unwrap();
    assert!(!source.join(".lenso/host-build.json").exists());
    source_candidate(&source, "app/local.example", "local.example");

    let marker = source.join("plugins/local.example/default.disabled");
    for _ in 0..2 {
        let result = toggle(&source, "disable", "local.example", "default");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("next build"));
        let metadata = fs::symlink_metadata(&marker).unwrap();
        assert!(metadata.file_type().is_file());
        assert_eq!(metadata.len(), 0);
    }
    assert!(!distribution.join("plugins/local.example").exists());
    assert_eq!(
        fs::read(distribution.join(".lenso/host-build.json")).unwrap(),
        host_before
    );

    let enabled = toggle(&source, "enable", "local.example", "default");
    assert!(
        enabled.status.success(),
        "{}",
        String::from_utf8_lossy(&enabled.stderr)
    );
    assert!(!marker.exists());

    let unknown = toggle(&source, "disable", "lenso.web-ingress", "default");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("not an App-owned local Plugin"));
    assert!(!source.join("plugins/lenso.web-ingress").exists());

    source_candidate(&source, "shared", "shared.example");
    fs::write(source.join("lenso.toml"), "plugin_sources = ['shared']\n").unwrap();
    let shared = toggle(&source, "disable", "shared.example", "default");
    assert!(!shared.status.success());
    assert!(String::from_utf8_lossy(&shared.stderr).contains("not an App-owned local Plugin"));
    assert!(!source.join("plugins/shared.example").exists());

    let other = toggle(&source, "disable", "local.example", "second");
    assert!(!other.status.success());
    assert!(String::from_utf8_lossy(&other.stderr).contains("only the App-owned `default`"));
    assert!(
        !source
            .join("plugins/local.example/second.disabled")
            .exists()
    );
}

#[test]
fn root_cargo_app_default_can_be_disabled_without_a_built_host() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let created = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["app", "create"])
        .arg(&source)
        .args(["--lang", "rust", "--no-install"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(source.join("Cargo.toml").is_file());
    assert!(!source.join(".lenso/host-build.json").exists());

    let disabled = toggle(&source, "disable", "local.starter", "default");
    assert!(
        disabled.status.success(),
        "{}",
        String::from_utf8_lossy(&disabled.stderr)
    );
    assert!(
        fs::read(source.join("plugins/local.starter/default.disabled"))
            .unwrap()
            .is_empty()
    );
    let enabled = toggle(&source, "enable", "local.starter", "default");
    assert!(
        enabled.status.success(),
        "{}",
        String::from_utf8_lossy(&enabled.stderr)
    );
    assert!(
        !source
            .join("plugins/local.starter/default.disabled")
            .exists()
    );
}

#[test]
fn root_cargo_app_without_optional_app_directory_can_toggle() {
    let temp = tempfile::tempdir().unwrap();
    source_candidate(temp.path(), ".", "local.root");

    let disabled = toggle(temp.path(), "disable", "local.root", "default");
    assert!(
        disabled.status.success(),
        "{}",
        String::from_utf8_lossy(&disabled.stderr)
    );
    assert!(
        temp.path()
            .join("plugins/local.root/default.disabled")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn source_toggle_refuses_symlinked_plugin_root_directory() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let outside = temp.path().join("outside");
    fs::create_dir_all(source.join("app")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    source_candidate(&source, "app/local.example", "local.example");
    std::os::unix::fs::symlink(&outside, source.join("plugins")).unwrap();

    let result = toggle(&source, "disable", "local.example", "default");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("regular directory"));
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}
