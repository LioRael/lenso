use super::*;
fn event(path: &str) -> notify::Event {
    notify::Event::new(notify::EventKind::Any).add_path(PathBuf::from(path))
}

#[tokio::test]
async fn debounce_keeps_backend_edits_after_a_frontend_event_and_reports_errors() {
    let (sender, mut receiver) = mpsc::channel(8);
    let mut batch = Batch::new(event("/app/frontend/page.tsx"));
    sender.send(Ok(event("/app/src/plugin.rs"))).await.unwrap();
    batch.collect(&mut receiver).await.unwrap();
    assert_eq!(batch.work(Path::new("/app"), true), Work::Rust);
    sender
        .send(Err(notify::Error::generic("watch failed")))
        .await
        .unwrap();
    assert!(batch.collect(&mut receiver).await.is_err());
}

#[test]
fn intent_membership_dependencies_and_resources_are_structural() {
    let root = Path::new("/app");
    assert_eq!(
        Batch::new(event("/app/plugins/a/default.toml")).work(root, false),
        Work::Configuration
    );
    for path in [
        "/app/plugins/a/default.disabled",
        "/app/plugins/a/default/resource.toml",
        "/app/plugins/.dependencies.json",
        "/app/Cargo.toml",
    ] {
        assert_eq!(Batch::new(event(path)).work(root, true), Work::Generation);
    }
    assert_eq!(
        Batch::new(event("/app/frontend/page.tsx")).work(root, true),
        Work::Frontend
    );
    assert_eq!(
        Batch::new(event("/app/frontend/page.tsx")).work(root, false),
        Work::TypeScript
    );
}

fn configuration_fixture() -> (tempfile::TempDir, tempfile::TempDir, Inputs) {
    use lenso_app_plan::authoring::{HostCatalog, HostPluginRelease, HostSlot, PluginDescriptor};
    let root = tempfile::tempdir().unwrap();
    let current = tempfile::tempdir().unwrap();
    for directory in [root.path(), current.path()] {
        fs::create_dir_all(directory.join("plugins/example.greeting")).unwrap();
        fs::write(
            directory.join("plugins/example.greeting/default.toml"),
            "message = 'before'\n",
        )
        .unwrap();
    }
    fs::create_dir_all(current.path().join("intent/plugins/example.greeting")).unwrap();
    fs::write(
        current
            .path()
            .join("intent/plugins/example.greeting/default.toml"),
        "message = 'before'\n",
    )
    .unwrap();
    fs::create_dir(current.path().join(".lenso")).unwrap();
    let descriptor = PluginDescriptor::new("example.greeting", "1.0.0", "web")
        .with_configuration_schema(serde_json::json!({"type":"object","properties":{"message":{"type":"string"}},"additionalProperties":false}));
    let catalog = HostCatalog::new(
        [HostSlot::many("web")],
        [HostPluginRelease::new(descriptor)],
        [],
    );
    fs::write(
        current.path().join(".lenso/host-catalog.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    fs::write(current.path().join(".lenso/distribution.lock.json"), serde_json::to_vec(&serde_json::json!({"schema":"lenso.local-host-distribution.v1","files":[{"path":".lenso/host-catalog.json"}]})).unwrap()).unwrap();
    let inputs = Inputs::capture(root.path(), false).unwrap().unwrap();
    (root, current, inputs)
}

#[test]
fn configuration_reuse_resolves_new_values_without_mutating_previous_intent() {
    let (root, current, inputs) = configuration_fixture();
    let path = root.path().join("plugins/example.greeting/default.toml");
    fs::write(&path, "message = 'after'\n").unwrap();
    fs::write(
        current.path().join("runtime-created-database"),
        "must not cross into the next Generation",
    )
    .unwrap();
    let output = root.path().join(".lenso/candidate");
    fs::create_dir(root.path().join(".lenso")).unwrap();
    let batch = Batch::new(notify::Event::new(notify::EventKind::Any).add_path(path));
    assert!(
        configuration_candidate(root.path(), current.path(), &output, &batch, &inputs, false)
            .unwrap()
    );
    assert!(!output.join("runtime-created-database").exists());
    assert!(
        fs::read_to_string(output.join("intent/plugins/example.greeting/default.toml"))
            .unwrap()
            .contains("after")
    );
    assert!(
        fs::read_to_string(current.path().join("plugins/example.greeting/default.toml"))
            .unwrap()
            .contains("before")
    );
}

#[test]
fn implementation_reuse_cannot_hide_config_dependency_or_membership_changes() {
    let (root, current, _) = configuration_fixture();
    let source = root.path().join("plugin.ts");
    fs::write(&source, "export const value='before';\n").unwrap();
    let before = Inputs::capture(root.path(), false).unwrap().unwrap();
    fs::write(&source, "export const value='after';\n").unwrap();
    let next = Inputs::capture(root.path(), false).unwrap().unwrap();
    assert_eq!(
        before
            .implementation_edits(&next, current.path(), root.path())
            .unwrap(),
        Some(vec![source])
    );
    let configuration = root.path().join("plugins/example.greeting/default.toml");
    fs::write(&configuration, "message='also edited'\n").unwrap();
    assert!(
        before
            .implementation_edits(&next, current.path(), root.path())
            .unwrap()
            .is_none()
    );
    fs::write(configuration, "message = 'before'\n").unwrap();
    fs::write(root.path().join("bun.lock"), "new dependency membership").unwrap();
    let next = Inputs::capture(root.path(), false).unwrap().unwrap();
    assert!(
        before
            .implementation_edits(&next, current.path(), root.path())
            .unwrap()
            .is_none()
    );
}

#[test]
fn schema_rejection_retains_previous_intent_and_source_changes_force_a_build() {
    let (root, current, inputs) = configuration_fixture();
    let path = root.path().join("plugins/example.greeting/default.toml");
    fs::write(&path, "message = 42\n").unwrap();
    fs::create_dir(root.path().join(".lenso")).unwrap();
    let output = root.path().join(".lenso/candidate");
    let batch = Batch::new(notify::Event::new(notify::EventKind::Any).add_path(path));
    assert!(
        configuration_candidate(root.path(), current.path(), &output, &batch, &inputs, false)
            .is_err()
    );
    assert!(
        fs::read_to_string(current.path().join("plugins/example.greeting/default.toml"))
            .unwrap()
            .contains("before")
    );
    fs::write(
        root.path().join("new-source.rs"),
        "// newly discovered implementation\n",
    )
    .unwrap();
    assert!(
        !configuration_candidate(
            root.path(),
            current.path(),
            &root.path().join(".lenso/next"),
            &batch,
            &inputs,
            false
        )
        .unwrap()
    );
    assert!(!root.path().join(".lenso/next").exists());
}
