use std::process::Command;

#[test]
fn bun_app_starter_can_reenter_the_root_host_for_plugin_scaffolding() {
    let temporary = tempfile::tempdir().expect("temporary App parent");
    let app = temporary.path().join("starter");
    let output = Command::new(env!("CARGO_BIN_EXE_lenso-engine-host"))
        .args([
            "app",
            "create",
            app.to_str().expect("UTF-8 temporary App path"),
            "--runtime",
            "bun",
            "--no-install",
        ])
        .output()
        .expect("run Engine Host app creator");

    assert!(
        output.status.success(),
        "app create failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(app.join("app/local.starter/package.json").is_file());
    assert!(app.join("plugins").is_dir());
}
