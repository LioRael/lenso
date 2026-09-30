use super::*;

fn intent(root: &Path) {
    let directory = root.join("plugins/example.worker-auth");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("default.toml"), "").unwrap();
}

#[test]
fn selected_features_union_without_enabling_defaults_or_unknown_features() {
    let root = tempfile::tempdir().unwrap();
    intent(root.path());
    let package = json!({
        "id":"owner#1.0.0","name":"owner","version":"1.0.0","source":null,
        "manifest_path":"/immutable/owner/Cargo.toml","targets":[{"kind":["lib"]}],
        "metadata":{"lenso":{"plugin-id":"example.worker-auth"}},
        "features":{"default":["pg"],"pg":[],"workers":[],"metrics":[]}
    });
    let mut sources = Sources::default();
    for features in [json!(["workers"]), json!(["metrics", "workers"])] {
        sources
            .select(
                root.path(),
                &package,
                &json!({"id":package["id"],"features":features}),
            )
            .unwrap();
    }
    let dependency = sources.dependency("example.worker-auth").unwrap();
    assert_eq!(dependency["default-features"], false);
    assert_eq!(dependency["features"], json!(["metrics", "workers"]));
    for invalid in [
        json!({"id":"another-owner#1.0.0","features":["workers"]}),
        json!({"id":package["id"],"features":["undeclared"]}),
    ] {
        assert!(sources.select(root.path(), &package, &invalid).is_err());
    }
}

fn cargo(manifest: &Path, arguments: &[&str]) -> std::process::Output {
    crate::app::cargo_command()
        .args(arguments)
        .arg("--manifest-path")
        .arg(manifest)
        .output()
        .unwrap()
}

fn write_workers_project(root: &Path) {
    for name in ["owner", "app", "consumer"] {
        fs::create_dir_all(root.join(name).join("src")).unwrap();
    }
    fs::write(
        root.join("owner/Cargo.toml"),
        r#"
[package]
name="worker-auth-owner"
version="1.0.0"
edition="2024"
[package.metadata.lenso]
plugin-id="example.worker-auth"
[features]
default=["pg"]
pg=[]
workers=[]
"#,
    )
    .unwrap();
    fs::write(
        root.join("owner/src/lib.rs"),
        r#"
#[cfg(feature="pg")]
compile_error!("a Workers root must not enable the Native PG default");
#[cfg(feature="workers")]
pub const WORKERS_SELECTED: bool = true;
"#,
    )
    .unwrap();
    fs::write(
        root.join("app/Cargo.toml"),
        r#"
[package]
name="selected-workers-app"
version="1.0.0"
edition="2024"
[workspace]
[dependencies]
worker-auth-owner={path="../owner",default-features=false,features=["workers"]}
"#,
    )
    .unwrap();
    fs::write(root.join("app/src/lib.rs"), "").unwrap();
}

#[test]
fn actual_cargo_workers_selection_survives_a_new_root_alias_without_native_pg() {
    let root = tempfile::tempdir().unwrap();
    intent(root.path());
    write_workers_project(root.path());
    let metadata = cargo(
        &root.path().join("app/Cargo.toml"),
        &["metadata", "--format-version=1"],
    );
    assert!(
        metadata.status.success(),
        "{}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "worker-auth-owner")
        .unwrap();
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == package["id"])
        .unwrap();
    let mut sources = Sources::default();
    sources.select(root.path(), package, node).unwrap();
    let manifest = json!({"package":{"name":"generated-workers-host","version":"1.0.0","edition":"2024"},
        "workspace":{},"dependencies":{"root_plugin_2":sources.dependency("example.worker-auth").unwrap()}});
    fs::write(
        root.path().join("consumer/Cargo.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        root.path().join("consumer/src/lib.rs"),
        "pub const AUTH_WORKERS: bool = root_plugin_2::WORKERS_SELECTED;",
    )
    .unwrap();
    let compiled = cargo(
        &root.path().join("consumer/Cargo.toml"),
        &["check", "--target", "wasm32-unknown-unknown"],
    );
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
}
