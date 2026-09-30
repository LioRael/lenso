use std::{fs, path::Path, process::Command};

#[test]
fn default_macro_compiles_with_registered_snapshot_and_codegen() {
    let consumer = tempfile::tempdir().unwrap();
    fs::create_dir(consumer.path().join("src")).unwrap();
    let manifest = format!(
        r#"[package]
name = "registered-snapshot-consumer"
version = "0.1.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
lenso-contract-authoring = "=0.1.1"
lenso-contract-codegen = "=0.10.0"
lenso-contract-authoring-macros = {{ path = {:?}, version = "=0.1.1" }}
schemars = "1"
tempfile = "3"
"#,
        Path::new(env!("CARGO_MANIFEST_DIR")),
    );
    fs::write(consumer.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(
        consumer.path().join("src/lib.rs"),
        include_str!("fixtures/registered_snapshot.rs"),
    )
    .unwrap();
    let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .arg("test")
        .arg("--manifest-path")
        .arg(consumer.path().join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "registered snapshot consumer failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
