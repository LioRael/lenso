use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

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
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command
        .arg("test")
        .arg("--manifest-path")
        .arg(consumer.path().join("Cargo.toml"));
    // Only compiled dependencies are reusable. Every invocation still resolves
    // and compiles the fresh consumer and executes its original assertions.
    // A separate target avoids the outer cargo test's workspace target lock.
    if let Some(root) = std::env::var_os("LENSO_CARGO_FIXTURE_CACHE_DIR") {
        let root = PathBuf::from(root);
        assert!(
            root.is_absolute(),
            "consumer fixture cache must be absolute"
        );
        command
            .arg("--target-dir")
            .arg(root.join("registered-snapshot"));
    }
    let started = Instant::now();
    let output = command.output().unwrap();
    // Bounded phase evidence survives successful libtest output capture. Do not
    // infer server/Host waits from time spent compiling a consumer fixture.
    let compiled = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.contains("Compiling "))
        .count();
    let _ = writeln!(
        std::io::stderr().lock(),
        "[registered-snapshot-consumer] elapsed={:.3}s compiled={compiled}",
        started.elapsed().as_secs_f64()
    );
    assert!(
        output.status.success(),
        "registered snapshot consumer failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
