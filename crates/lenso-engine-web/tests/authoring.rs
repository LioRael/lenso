use lenso_engine::Snapshot;
use lenso_engine_web::{Route, RouteSet, WebOptions, build, compile, read_sources};
use std::fs;

#[test]
fn custom_roots_multiple_handlers_exclusion_and_provider_lower_through_endpoint() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("api/internal")).unwrap();
    fs::write(root.path().join("api/public.rs"), "#[get(\"health\", \"/health\")] async fn health(&self) {}\n#[post(\"save\", \"/items/{id}\")] async fn save(&self) {}").unwrap();
    fs::write(root.path().join("api/internal/invalid.rs"), "invalid Rust").unwrap();
    let options = WebOptions {
        provider: "Gateway".into(),
        roots: vec!["api".into()],
        exclude: vec!["api/internal".into()],
        output: "generated/routes.rs".into(),
        register_plugin: false,
        ..Default::default()
    };
    let inventory = build(root.path(), output.path(), options).unwrap();
    assert_eq!(inventory.routes.len(), 2);
    let source = fs::read_to_string(output.path().join("generated/routes.rs")).unwrap();
    assert!(source.contains("impl Gateway"));
    assert!(source.contains("standalone"));
}
#[test]
fn explicit_snapshot_selection_has_no_filename_semantics() {
    let mut snapshot = Snapshot::default();
    snapshot
        .insert(
            "custom/data.input".into(),
            b"#[get(\"selected\", \"/selected\")] async fn selected(&self) {}".to_vec(),
        )
        .unwrap();
    snapshot
        .insert("src/routes/broken.rs".into(), b"invalid".to_vec())
        .unwrap();
    let generated = compile(
        snapshot,
        WebOptions {
            entries: vec!["custom/data.input".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        generated.outputs["web/routes"]["routes"].value["routes"][0]["id"],
        "selected"
    );
}
#[test]
fn conflicting_param_shapes_invalid_params_and_ids_have_source_evidence() {
    let route = |id: &str, path: &str, source: &str| Route {
        id: id.into(),
        method: "GET".into(),
        path: path.into(),
        source: source.into(),
    };
    let error = RouteSet {
        routes: vec![
            route("a", "/items/{id}", "first.rs"),
            route("b", "/items/{name}", "second.rs"),
        ],
    }
    .validate()
    .unwrap_err()
    .to_string();
    assert!(error.contains("first.rs") && error.contains("second.rs"));
    for path in ["relative", "/{id}/{id}", "/{*all}/next", "/items?query"] {
        assert!(
            RouteSet {
                routes: vec![route("a", path, "bad.rs")]
            }
            .validate()
            .is_err()
        );
    }
}
#[cfg(unix)]
#[test]
fn selected_symlink_is_rejected_and_excluded_tree_is_not_read() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src/routes")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("src/routes/external")).unwrap();
    assert!(read_sources(root.path(), &WebOptions::default()).is_err());
    let options = WebOptions {
        exclude: vec!["src/routes/external".into()],
        ..Default::default()
    };
    assert!(
        read_sources(root.path(), &options)
            .unwrap()
            .files()
            .is_empty()
    );
}
