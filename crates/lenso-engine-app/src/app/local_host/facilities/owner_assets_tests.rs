use super::*;

fn fixture() -> (tempfile::TempDir, Value, Value) {
    let root = tempfile::tempdir().unwrap();
    let wrapper_root = root.path().join("wrapper");
    let owner_root = root.path().join("owner");
    fs::create_dir_all(&wrapper_root).unwrap();
    fs::create_dir_all(owner_root.join("src/workers")).unwrap();
    fs::write(
        owner_root.join("src/workers/journal.mjs"),
        "export function create() { return {}; }",
    )
    .unwrap();
    let wrapper = json!({"id":"wrapper-id","name":"wrapper","version":"1.0.0",
    "manifest_path":wrapper_root.join("Cargo.toml"),
    "metadata":{"lenso":{"host-facilities":{"journal":{
        "workers":"host_facilities::journal",
        "workers-adapter-package":"journal-owner",
        "workers-adapter":"src/workers/journal.mjs"
    }}}}});
    let owner = json!({"id":"owner-id","name":"journal-owner","version":"2.0.0",
        "source":"git+https://example.com/owner?rev=exact#exact",
        "manifest_path":owner_root.join("Cargo.toml")});
    let graph = json!({"packages":[wrapper,owner],"resolve":{"nodes":[
        {"id":"wrapper-id","deps":[{"pkg":"owner-id","dep_kinds":[{"kind":null}]}]},
        {"id":"owner-id","deps":[]}
    ]}});
    (root, wrapper, graph)
}

fn select(root: &Path, wrapper: &Value, graph: &Value) -> anyhow::Result<Sources> {
    let mut sources = Sources::default();
    sources.select(
        "selected",
        "example.journal",
        wrapper,
        &root.join("wrapper"),
        Some(graph),
    )?;
    Ok(sources)
}

#[test]
fn explicit_reachable_owner_is_copied_with_its_exact_cargo_provenance() {
    let (root, wrapper, graph) = fixture();
    let sources = select(root.path(), &wrapper, &graph).unwrap();
    let plan = lenso_app_plan::ResolvedAppPlan::new(
        vec![lenso_app_plan::PluginInstancePlan::new(
            "journal/default",
            "example.journal",
        )],
        vec![],
    );
    let grants = json!({"schema":"lenso.host-facilities.v1","instances":{
        "journal/default":{"journal":{"binding":"JOURNAL_D1","configuration":{}}}
    }});
    let stage = tempfile::tempdir().unwrap();
    let (_, imports, evidence) =
        render_workers(&sources.0, &grants, &plan, stage.path(), &json!({})).unwrap();
    let expected = fs::read(root.path().join("owner/src/workers/journal.mjs")).unwrap();
    assert_eq!(
        fs::read(stage.path().join("facilities/owner_0.mjs")).unwrap(),
        expected
    );
    assert!(imports.contains("./facilities/owner_0.mjs"));
    assert_eq!(evidence[0]["owner"]["cargo_id"], "owner-id");
    assert_eq!(evidence[0]["owner"]["version"], "2.0.0");
    assert_eq!(
        evidence[0]["owner"]["source"],
        graph["packages"][1]["source"]
    );
    assert_eq!(
        evidence[0]["owner"]["relative_path"],
        "src/workers/journal.mjs"
    );
    assert_eq!(
        evidence[0]["digest"],
        super::super::digest(&stage.path().join("facilities/owner_0.mjs")).unwrap()
    );
}

#[test]
fn unselected_and_build_only_owners_cannot_supply_a_worker_adapter() {
    let (root, wrapper, mut graph) = fixture();
    graph["resolve"]["nodes"][0]["deps"][0]["dep_kinds"][0]["kind"] = json!("build");
    assert!(
        select(root.path(), &wrapper, &graph)
            .unwrap_err()
            .to_string()
            .contains("reachable runtime Cargo owner")
    );
    graph["resolve"]["nodes"][0]["deps"] = json!([]);
    assert!(select(root.path(), &wrapper, &graph).is_err());
}

#[test]
fn same_named_reachable_package_versions_are_ambiguous() {
    let (root, wrapper, mut graph) = fixture();
    let mut second = graph["packages"][1].clone();
    second["id"] = json!("other-owner-id");
    second["version"] = json!("3.0.0");
    graph["packages"].as_array_mut().unwrap().push(second);
    graph["resolve"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"other-owner-id","deps":[]}));
    graph["resolve"]["nodes"][0]["deps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pkg":"other-owner-id","dep_kinds":[{"kind":null}]}));
    assert!(
        select(root.path(), &wrapper, &graph)
            .unwrap_err()
            .to_string()
            .contains("one reachable")
    );
}

#[test]
fn owner_relative_paths_and_module_sizes_remain_bounded() {
    let (root, mut wrapper, graph) = fixture();
    wrapper["metadata"]["lenso"]["host-facilities"]["journal"]["workers-adapter"] =
        json!("../outside.mjs");
    assert!(select(root.path(), &wrapper, &graph).is_err());
    wrapper["metadata"]["lenso"]["host-facilities"]["journal"]["workers-adapter"] =
        json!("src/workers/journal.mjs");
    let module = root.path().join("owner/src/workers/journal.mjs");
    fs::write(&module, vec![b' '; MAX_ADAPTER_BYTES]).unwrap();
    assert!(select(root.path(), &wrapper, &graph).is_ok());
    fs::write(&module, vec![b' '; MAX_ADAPTER_BYTES + 1]).unwrap();
    assert!(
        select(root.path(), &wrapper, &graph)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
}

#[cfg(unix)]
#[test]
fn adapter_symlinks_cannot_escape_the_explicit_owner_root() {
    let (root, wrapper, graph) = fixture();
    let module = root.path().join("owner/src/workers/journal.mjs");
    fs::remove_file(&module).unwrap();
    let outside = root.path().join("outside.mjs");
    fs::write(&outside, "export function create() {}").unwrap();
    std::os::unix::fs::symlink(outside, module).unwrap();
    assert!(
        select(root.path(), &wrapper, &graph)
            .unwrap_err()
            .to_string()
            .contains("escapes its owning source")
    );
}
