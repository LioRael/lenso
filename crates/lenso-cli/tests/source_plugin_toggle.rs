use std::{fs, process::Command};

#[test]
fn built_source_app_toggle_explains_authority_boundary_without_writing_marker() {
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
    assert!(distribution.join(".lenso/host-build.json").is_file());
    assert!(!source.join(".lenso/host-build.json").exists());

    for action in ["disable", "enable"] {
        if action == "enable" {
            fs::create_dir_all(source.join("plugins/local.example")).unwrap();
            fs::write(source.join("plugins/local.example/default.disabled"), []).unwrap();
        }
        let result = Command::new(cli)
            .args(["plugins", action, "local.example", "default", "--root"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains("source App has no Host authority"),
            "{error}"
        );
        assert!(error.contains("lenso app discover"), "{error}");
        assert!(error.contains("App-owned"), "{error}");
        assert!(error.contains("lenso app build"), "{error}");
        assert!(error.contains("built distribution"), "{error}");
        assert_eq!(
            source
                .join("plugins/local.example/default.disabled")
                .exists(),
            action == "enable"
        );
    }
}
