//! Exercise the production generator, Cargo build, and Host descriptor process.
use super::*;

fn package(root: &Path, name: &str, extra: &str, source: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname={name:?}\nversion=\"1.0.0\"\nedition=\"2024\"\n[workspace]\n{extra}"
        ),
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
}

#[test]
#[ignore = "builds and runs a generated release Host; run explicitly for link-graph changes"]
fn generated_host_preserves_normal_features_and_excludes_host_only_plugins() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path();
    let facade = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lenso")
        .canonicalize()
        .unwrap();
    package(
        &root.join("contract"),
        "fixture-contract",
        &format!(
            r#"
[package.metadata.lenso.contract]
descriptor="capability.json"
projection="rust-runtime"
[features]
default=["unrequested"]
unrequested=[]
normal=[]
[dependencies]
lenso-runtime-codec={{path={:?}}}
"#,
            facade
                .parent()
                .unwrap()
                .join("lenso-runtime-codec")
                .to_str()
                .unwrap()
        ),
        r#"
pub struct Token;
pub const NORMAL: bool = cfg!(feature="normal");
pub const UNREQUESTED: bool = cfg!(feature="unrequested");
"#,
    );
    // A selected metadata wrapper reexports one canonical Plugin from a local
    // aggregate. Cargo retains the aggregate's other inventory registrations.
    package(
        &root.join("aggregate"),
        "fixture-aggregate",
        &format!(
            "[dependencies]\nlenso={{path={:?}}}\n",
            facade.to_str().unwrap()
        ),
        r#"
pub mod chosen {
    #[lenso::plugin(id="example.chosen", root_slot="tools", consumer)]
    #[derive(Debug)]
    pub struct Plugin {}
}
pub mod sibling {
    #[lenso::plugin(id="example.unselected", root_slot="tools", consumer)]
    #[derive(Debug)]
    pub struct Plugin {}
}
"#,
    );
    package(
        &root.join("wrapper"),
        "fixture-wrapper",
        r#"
[package.metadata.lenso]
plugin-id="example.chosen"
[package.metadata.lenso-cli]
runtime="native-linked"
[dependencies]
aggregate={package="fixture-aggregate",path="../aggregate"}
"#,
        "pub use aggregate::chosen::{Plugin, link_plugin};\n",
    );
    fs::write(
        root.join("contract/capability.json"),
        r#"{"id":"example.echo@1"}"#,
    )
    .unwrap();
    package(
        &root.join("owner"),
        "fixture-owner",
        &format!(
            r#"
[package.metadata.lenso]
plugin-id="example.root-owner"
[features]
default=["unrequested"]
unrequested=[]
normal=[]
current=[]
build-only=[]
dev-only=[]
inactive=[]
[dependencies]
lenso={{path={:?}}}
contract={{package="fixture-contract",path="../contract",default-features=false,features=["normal"]}}
"#,
            facade.to_str().unwrap()
        ),
        r#"
pub mod authoring {
    #[lenso::plugin(id="example.root-owner", root_slot="tools", consumer)]
    #[derive(Debug)]
    pub struct Plugin {}
}
pub use contract::Token;
pub fn link_plugin() {
    assert!(contract::NORMAL && !contract::UNREQUESTED);
    assert!(cfg!(feature="normal"));
    assert!(cfg!(feature="current"));
    assert!(!cfg!(feature="unrequested"));
    assert!(!cfg!(feature="build-only"));
    assert!(!cfg!(feature="dev-only"));
    assert!(!cfg!(feature="inactive"));
    <authoring::Plugin as lenso::NativePluginDefinition>::link();
}
"#,
    );
    package(
        &root.join("facade"),
        "fixture-facade",
        r#"
[features]
default=["owner/normal"]
[dependencies]
owner={package="fixture-owner",path="../owner",default-features=false}
[target.'cfg(unix)'.dependencies]
owner={package="fixture-owner",path="../owner",default-features=false,features=["current"]}
[target.'cfg(windows)'.dependencies]
owner={package="fixture-owner",path="../owner",default-features=false,features=["current"]}
"#,
        "pub use owner::Token;\n",
    );
    package(
        &root.join("host-owner"),
        "fixture-host-owner",
        r#"
[package.metadata.lenso]
plugin-id="example.host-only"
"#,
        "pub struct HostOnly;\n",
    );
    package(
        &root.join("macros"),
        "fixture-macros",
        r#"
[lib]
proc-macro=true
[dependencies]
fixture-host-owner={path="../host-owner"}
"#,
        "extern crate proc_macro;\n",
    );
    let app = root.join("app");
    package(
        &app,
        "fixture-app",
        &format!(
            r#"
[package.metadata.lenso]
plugin-id="example.app"
[dependencies]
lenso={{path={:?}}}
lenso-runner={{path={:?}}}
fixture-facade={{path="../facade"}}
owner={{package="fixture-owner",path="../owner",default-features=false}}
fixture-macros={{path="../macros"}}
[build-dependencies]
owner={{package="fixture-owner",path="../owner",default-features=false,features=["build-only"]}}
[dev-dependencies]
owner={{package="fixture-owner",path="../owner",default-features=false,features=["dev-only"]}}
[target.'cfg(target_arch="wasm32")'.dependencies]
owner={{package="fixture-owner",path="../owner",default-features=false,features=["inactive"]}}
"#,
            facade.to_str().unwrap(),
            facade
                .parent()
                .unwrap()
                .join("lenso-runner")
                .to_str()
                .unwrap()
        ),
        r#"
mod authoring {
    #[lenso::plugin(id="example.app", root_slot="tools", consumer)]
    #[derive(Debug)]
    pub struct Plugin {}
}
pub fn link_plugin() { <authoring::Plugin as lenso::NativePluginDefinition>::link(); }
pub fn typed(value: fixture_facade::Token) -> owner::Token { value }
"#,
    );
    fs::write(app.join("build.rs"), "fn main() {}\n").unwrap();
    let stage = root.join("stage");
    fs::create_dir_all(stage.join(".lenso")).unwrap();
    for id in ["example.root-owner", "example.host-only"] {
        fs::create_dir_all(stage.join("plugins").join(id)).unwrap();
        fs::write(stage.join("plugins").join(id).join("default.toml"), "").unwrap();
    }
    let candidate = Candidate {
        native_link: None,
        surface_owner: None,
        composite: None,
        plugin_id: "example.app".into(),
        release_version: "1.0.0".into(),
        project: app.clone(),
        metadata: app.join("Cargo.toml"),
        format: "cargo".into(),
        role: lenso_app_authoring::discovery::SourceRole::Shared,
        implementations: vec![lenso_app_authoring::discovery::Implementation {
            id: "native".into(),
            runtime: "native-linked".into(),
            project: app,
        }],
        published_resources: vec![],
        evidence: "generated Host regression".into(),
    };
    let inputs = lenso_engine::discovery::DiscoverySession::new(root).unwrap();
    let wrapper = lenso_app_authoring::discovery::discover_in(&root.join("wrapper"), &inputs)
        .unwrap()
        .candidates
        .pop()
        .unwrap();
    assert!(wrapper.native_link.is_none());
    assert!(is_native(&wrapper));
    let descriptors = generate_in(
        &stage,
        &root.join("cache"),
        &[candidate, wrapper],
        AdapterSet::default(),
        &inputs,
    )
    .unwrap();
    assert_eq!(
        descriptors
            .iter()
            .map(|d| d.plugin_id())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["example.app", "example.chosen", "example.root-owner"])
    );
    assert!(stage.join(".lenso/host").is_file());
    assert!(stage.join(".lenso/generated-host/Cargo.lock").is_file());
    let recorded: Value =
        serde_json::from_slice(&fs::read(stage.join(".lenso/root-linked-sources.json")).unwrap())
            .unwrap();
    assert_eq!(recorded.as_array().unwrap().len(), 1);
    assert_eq!(recorded[0]["dependency"]["default-features"], false);
}

// This protects a real Cargo failure: two generated packages with the same
// name/version reused the other App's build-script cfg in a shared target dir.
#[test]
fn shared_cargo_target_does_not_mix_generated_app_adapter_cfgs() {
    let fixture = tempfile::tempdir().unwrap();
    let shared = fixture.path().join("target");
    let roots = [fixture.path().join("one"), fixture.path().join("two")];
    for (index, root) in roots.iter().enumerate() {
        fs::create_dir_all(root.join("src")).unwrap();
        let name = generated_host_package(&root.canonicalize().unwrap());
        fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname={name:?}\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n"),
        )
        .unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"{}\", cfg!(generated_bun_adapter)); }\n",
        )
        .unwrap();
        fs::write(
            root.join("build.rs"),
            generated_host_build_script(
                AdapterSet {
                    bun: index == 0,
                    ..AdapterSet::default()
                },
                "modern",
            ),
        )
        .unwrap();
    }
    for (index, expected) in [(0, "true"), (1, "false"), (0, "true")] {
        let output = super::super::cargo_command()
            .args(["run", "--quiet", "--offline", "--manifest-path"])
            .arg(roots[index].join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", &shared)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    }
}
