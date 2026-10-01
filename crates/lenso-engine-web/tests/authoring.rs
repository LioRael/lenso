use lenso_engine::Snapshot;
use lenso_engine_web::{Route, RouteSet, WebOptions, build, compile, read_sources};
use std::fs;

#[test]
fn filesystem_and_explicit_styles_lower_together_with_scoped_middleware() {
    let mut snapshot = Snapshot::default();
    snapshot.insert("src/app/(api)/orders/[id]/route.rs".into(), b"#[get] #[route_id(\"orders.read\")] #[middleware(local)] async fn read(&self, Path(path): Path<OrderPath>) -> Json<String> { Json(path.id) }".to_vec()).unwrap();
    snapshot
        .insert(
            "src/app/route.rs".into(),
            b"#[get] async fn home(&self) {}".to_vec(),
        )
        .unwrap();
    snapshot
        .insert(
            "src/app/files/[...rest]/route.rs".into(),
            b"#[post] async fn files(&self) {}".to_vec(),
        )
        .unwrap();
    snapshot
        .insert(
            "src/routes/health.rs".into(),
            b"#[get(\"/health\")] async fn health(&self) {}".to_vec(),
        )
        .unwrap();
    let generation = compile(
        snapshot,
        WebOptions {
            filesystem_roots: vec!["src/app".into()],
            middleware: vec!["global".into()],
            scopes: std::collections::BTreeMap::from([
                ("src/app".into(), vec!["outer".into()]),
                ("src/app/(api)".into(), vec!["inner".into()]),
            ]),
            ..Default::default()
        },
    )
    .unwrap();
    let routes: RouteSet =
        serde_json::from_value(generation.outputs["web/routes"]["routes"].value.clone()).unwrap();
    assert!(
        routes
            .routes
            .iter()
            .any(|route| route.id == "orders.read" && route.path == "/orders/{id}")
    );
    assert!(routes.routes.iter().any(|route| route.path == "/"));
    assert!(
        routes
            .routes
            .iter()
            .any(|route| route.path == "/files/{*rest}" && route.method == "POST")
    );
    let output: lenso_engine::publication::FileResource =
        serde_json::from_value(generation.outputs["web/routes"]["bindings"].value.clone()).unwrap();
    let source = String::from_utf8(output.bytes).unwrap();
    assert!(source.contains("middleware (global)"));
    assert!(source.contains("middleware (outer , inner)"));
    assert!(source.contains("middleware (local)"));
    assert!(source.contains("Path < OrderPath >"));
}

#[test]
fn mixed_style_collisions_and_ambiguous_filesystem_paths_fail_with_sources() {
    let options = WebOptions {
        filesystem_roots: vec!["src/app".into()],
        ..Default::default()
    };
    for path in [
        "src/app/[[...all]]/route.rs",
        "src/app/[...all]/more/route.rs",
        "src/app/[id]/[id]/route.rs",
        "src/app/(bad(group)/route.rs",
    ] {
        let mut snapshot = Snapshot::default();
        snapshot
            .insert(path.into(), b"#[get] async fn invalid(&self) {}".to_vec())
            .unwrap();
        assert!(compile(snapshot, options.clone()).is_err(), "{path}");
    }
    let mut snapshot = Snapshot::default();
    snapshot
        .insert(
            "src/app/orders/[id]/route.rs".into(),
            b"#[get] async fn read(&self) {}".to_vec(),
        )
        .unwrap();
    snapshot
        .insert(
            "src/routes/read.rs".into(),
            b"#[get(\"explicit\", \"/orders/{name}\")] async fn other(&self) {}".to_vec(),
        )
        .unwrap();
    let error = format!("{:#}", compile(snapshot, options).unwrap_err());
    assert!(
        error.contains("src/app/orders/[id]/route.rs") && error.contains("src/routes/read.rs"),
        "{error}"
    );
}

#[test]
fn shared_epoch_deduplicates_overlapping_selection_and_refreshes_membership() {
    use lenso_engine::discovery::DiscoverySession;
    use lenso_engine_web::read_sources_in;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src/routes/nested")).unwrap();
    fs::write(
        root.path().join("src/routes/nested/a.rs"),
        "#[get(\"a\", \"/a\")] async fn a(&self) {}",
    )
    .unwrap();
    let mut session = DiscoverySession::new(root.path()).unwrap();
    let options = WebOptions {
        roots: vec!["src/routes".into(), "src/routes/nested".into()],
        ..Default::default()
    };
    assert_eq!(
        read_sources_in(&mut session, &options)
            .unwrap()
            .files()
            .len(),
        1
    );
    read_sources_in(&mut session, &options).unwrap();
    assert_eq!(session.stats().directory_reads, 2);
    assert_eq!(session.stats().file_reads, 1);
    fs::write(
        root.path().join("src/routes/nested/b.rs"),
        "#[get(\"b\", \"/b\")] async fn b(&self) {}",
    )
    .unwrap();
    session.invalidate("src/routes/nested/b.rs").unwrap();
    assert_eq!(
        read_sources_in(&mut session, &options)
            .unwrap()
            .files()
            .len(),
        2
    );
    assert_eq!(session.stats().file_reads, 2);
    let options = WebOptions {
        exclude: vec!["src/routes/nested".into()],
        ..options
    };
    assert!(
        read_sources_in(&mut session, &options)
            .unwrap()
            .files()
            .is_empty()
    );
}

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
