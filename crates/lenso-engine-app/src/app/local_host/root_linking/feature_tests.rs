use super::*;

fn intent(root: &Path) {
    let directory = root.join("plugins/example.worker-auth");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("default.toml"), "").unwrap();
}

#[test]
fn selected_alias_validates_features_without_requesting_metadata_union() {
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
    assert_eq!(dependency["features"], json!([]));
    for invalid in [
        json!({"id":"another-owner#1.0.0","features":["workers"]}),
        json!({"id":package["id"],"features":["undeclared"]}),
        json!({"id":package["id"],"features":[true]}),
        json!({"id":package["id"],"features":null}),
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
    "workspace":{},"dependencies":{
        "local_plugin_0":{"package":"selected-workers-app","path":root.path().join("app")},
        "root_plugin_2":sources.dependency("example.worker-auth").unwrap()
    }});
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

fn select_native_root(root: &Path, project: &str, sources: &mut Sources) -> Value {
    let output = cargo(
        &root.join(project).join("Cargo.toml"),
        &[
            "metadata",
            "--format-version=1",
            "--filter-platform",
            lenso_app_authoring::native_host_target(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
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
    sources.select(root, package, node).unwrap();
    node["features"].clone()
}

fn check_project(root: &Path, project: &str) {
    let output = cargo(&root.join(project).join("Cargo.toml"), &["check"]);
    assert!(
        output.status.success(),
        "{project}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_consumer(root: &Path, sources: &Sources, mut dependencies: Value, source: &str) {
    dependencies["root_plugin"] = sources.dependency("example.worker-auth").unwrap();
    let manifest = json!({
        "package":{"name":"generated-feature-host","version":"1.0.0","edition":"2024"},
        "workspace":{},"dependencies":dependencies
    });
    fs::write(
        root.join("consumer/Cargo.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(root.join("consumer/src/lib.rs"), source).unwrap();
}

fn assert_root_alias_does_not_promote_features(extra: &str) {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    intent(root);
    write_workers_project(root);
    fs::write(
            root.join("owner/src/lib.rs"),
            "pub struct Token;\npub const PG: bool = cfg!(feature=\"pg\");\npub const WORKERS: bool = cfg!(feature=\"workers\");\n",
        )
        .unwrap();
    let manifest_path = root.join("app/Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(&manifest_path, format!(
            "{manifest}\n{extra}\nworker-auth-owner={{path=\"../owner\",default-features=false,features=[\"pg\"]}}\n"
        )).unwrap();
    fs::write(
            root.join("app/src/lib.rs"),
            "const _: () = assert!(!worker_auth_owner::PG && worker_auth_owner::WORKERS);\npub fn token() -> worker_auth_owner::Token { worker_auth_owner::Token }",
        )
        .unwrap();
    if extra == "[build-dependencies]" {
        fs::write(
            root.join("app/build.rs"),
            "fn main(){assert!(worker_auth_owner::PG);}",
        )
        .unwrap();
    }
    check_project(root, "app");
    let mut sources = Sources::default();
    let features = select_native_root(root, "app", &mut sources);
    assert!(features.as_array().unwrap().contains(&json!("pg")));
    write_consumer(
        root,
        &sources,
        json!({"local_plugin_0":{"package":"selected-workers-app","path":root.join("app")}}),
        "const _: () = assert!(!root_plugin::PG && root_plugin::WORKERS);\npub fn typed() { let _: root_plugin::Token = local_plugin_0::token(); }",
    );
    check_project(root, "consumer");
}

#[test]
fn root_alias_does_not_promote_build_features() {
    assert_root_alias_does_not_promote_features("[build-dependencies]");
}

#[test]
fn root_alias_does_not_promote_dev_features() {
    assert_root_alias_does_not_promote_features("[dev-dependencies]");
}

#[test]
fn root_alias_does_not_promote_inactive_target_features() {
    assert_root_alias_does_not_promote_features("[target.'wasm32-unknown-unknown'.dependencies]");
}

#[test]
fn root_alias_preserves_multiple_normal_roots_and_current_target_features() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    intent(root);
    write_workers_project(root);
    let owner = root.join("owner/Cargo.toml");
    fs::write(
        &owner,
        format!(
            "{}\nmetrics=[]\ncurrent=[]\n",
            fs::read_to_string(&owner).unwrap()
        ),
    )
    .unwrap();
    fs::write(root.join("owner/src/lib.rs"),
        "pub struct Token;\npub const PG: bool=cfg!(feature=\"pg\");\npub const WORKERS: bool=cfg!(feature=\"workers\");\npub const METRICS: bool=cfg!(feature=\"metrics\");\npub const CURRENT: bool=cfg!(feature=\"current\");").unwrap();
    let app = root.join("app/Cargo.toml");
    fs::write(&app,format!(
        "{}\n[target.'{}'.dependencies]\nworker-auth-owner={{path=\"../owner\",default-features=false,features=[\"current\"]}}\n",
        fs::read_to_string(&app).unwrap(),lenso_app_authoring::native_host_target()
    )).unwrap();
    fs::create_dir_all(root.join("second/src")).unwrap();
    fs::write(root.join("second/Cargo.toml"),
        "[package]\nname=\"second-app\"\nversion=\"1.0.0\"\nedition=\"2024\"\n[workspace]\n[features]\ndefault=[\"enabled\"]\nenabled=[\"dep:worker-auth-owner\",\"worker-auth-owner/metrics\"]\n[dependencies]\nworker-auth-owner={path=\"../owner\",optional=true,default-features=false}\n").unwrap();
    for project in ["app", "second"] {
        fs::write(
            root.join(project).join("src/lib.rs"),
            "pub fn token() -> worker_auth_owner::Token { worker_auth_owner::Token }",
        )
        .unwrap();
        check_project(root, project);
    }
    let mut sources = Sources::default();
    select_native_root(root, "app", &mut sources);
    select_native_root(root, "second", &mut sources);
    write_consumer(
        root,
        &sources,
        json!({
            "local_plugin_0":{"package":"selected-workers-app","path":root.join("app")},
            "local_plugin_1":{"package":"second-app","path":root.join("second")}
        }),
        "const _: () = assert!(root_plugin::WORKERS && root_plugin::METRICS && root_plugin::CURRENT && !root_plugin::PG);\npub fn typed() { let _: root_plugin::Token = local_plugin_0::token(); let _: root_plugin::Token = local_plugin_1::token(); }",
    );
    check_project(root, "consumer");
}

#[test]
fn root_alias_preserves_defaults_requested_by_an_original_normal_edge() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    intent(root);
    write_workers_project(root);
    let app = root.join("app/Cargo.toml");
    fs::write(
        &app,
        fs::read_to_string(&app)
            .unwrap()
            .replace("default-features=false,", ""),
    )
    .unwrap();
    fs::write(root.join("owner/src/lib.rs"),
        "pub struct Token;\npub const PG: bool=cfg!(feature=\"pg\");\npub const WORKERS: bool=cfg!(feature=\"workers\");").unwrap();
    fs::write(root.join("app/src/lib.rs"),
        "const _: () = assert!(worker_auth_owner::PG && worker_auth_owner::WORKERS);\npub fn token() -> worker_auth_owner::Token { worker_auth_owner::Token }").unwrap();
    check_project(root, "app");
    let mut sources = Sources::default();
    select_native_root(root, "app", &mut sources);
    write_consumer(
        root,
        &sources,
        json!({"local_plugin_0":{"package":"selected-workers-app","path":root.join("app")}}),
        "const _: () = assert!(root_plugin::PG && root_plugin::WORKERS);\npub fn typed() { let _: root_plugin::Token = local_plugin_0::token(); }",
    );
    check_project(root, "consumer");
}
