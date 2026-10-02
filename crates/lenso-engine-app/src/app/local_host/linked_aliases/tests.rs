use super::*;
use serde_json::json;
use std::{collections::BTreeMap, fs};

fn graph(kind: Option<&str>, proc_macro: bool) -> Value {
    json!({"packages":[
        {"id":"app","targets":[{"kind":["lib"]}]},
        {"id":"middle","targets":[{"kind":[if proc_macro {"proc-macro"} else {"lib"}]}]},
        {"id":"owner","targets":[{"kind":["lib"]}]}
    ],"resolve":{"root":"host","nodes":[
        {"id":"host","deps":[{"name":"local_plugin_0","pkg":"app"},{"name":"root_plugin_0","pkg":"owner"}]},
        {"id":"app","deps":[{"pkg":"middle","dep_kinds":[{"kind":kind}]}]},
        {"id":"middle","deps":[{"pkg":"owner","dep_kinds":[{"kind":null}]}]},
        {"id":"owner","deps":[]}
    ]}})
}

#[test]
fn aliases_must_retain_identity_and_a_target_normal_path() {
    let expected = BTreeMap::from([("root_plugin_0".into(), "owner".into())]);
    verify(&graph(None, false), &expected).unwrap();
    for (kind, proc_macro) in [(Some("build"), false), (Some("dev"), false), (None, true)] {
        assert!(verify(&graph(kind, proc_macro), &expected).is_err());
    }
    let changed = BTreeMap::from([("root_plugin_0".into(), "same-name-other-source".into())]);
    assert!(verify(&graph(None, false), &changed).is_err());
    let mut absent = graph(None, false);
    absent["resolve"]["nodes"][0]["deps"][0]["name"] = json!("local_web_contract");
    assert!(verify(&absent, &expected).is_err());
}

fn command(directory: &Path, program: &str, args: &[&str]) -> String {
    let output = crate::app::build_command(program)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", directory.join("target"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn write_package(root: &Path, name: &str, extra: &str, source: &str) {
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
fn real_cargo_rejects_selector_and_root_patch_identity_drift() {
    let fixture = tempfile::tempdir().unwrap();
    let repository = fixture.path().join("owner");
    write_package(
        &repository,
        "alias-owner",
        "[features]\nworkers=[]\n",
        "pub struct Token;\n",
    );
    command(&repository, "git", &["init", "--initial-branch=main"]);
    command(&repository, "git", &["add", "."]);
    command(
        &repository,
        "git",
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
    );
    let rev = command(&repository, "git", &["rev-parse", "HEAD"]);
    let git = format!("file://{}", repository.display());
    let patched = fixture.path().join("patched-owner");
    write_package(
        &patched,
        "alias-owner",
        "[features]\nworkers=[]\n",
        "pub struct Token;\n",
    );
    for (case, selector, patch) in [
        ("full", format!("rev={rev:?}"), false),
        ("branch", "branch=\"main\"".into(), false),
        ("short", format!("rev={:?}", &rev[..12]), false),
        ("patch", format!("rev={rev:?}"), true),
    ] {
        let app = fixture.path().join(case).join("app");
        let extra = format!(
            "[dependencies]\nalias-owner={{git={git:?},{selector},features=[\"workers\"]}}\n{}",
            if patch {
                format!(
                    "[patch.{git:?}]\nalias-owner={{path={:?}}}\n",
                    patched.to_str().unwrap()
                )
            } else {
                String::new()
            }
        );
        write_package(
            &app,
            &format!("native-{case}"),
            &extra,
            "pub use alias_owner::Token;\n",
        );
        let output = command(&app, "cargo", &["metadata", "--format-version=1"]);
        let metadata: Value = serde_json::from_str(&output).unwrap();
        let owner = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"] == "alias-owner")
            .unwrap();
        let expected = BTreeMap::from([(
            "root_plugin_0".into(),
            owner["id"].as_str().unwrap().to_owned(),
        )]);
        let mut alias = super::super::dependency(owner).unwrap();
        alias["default-features"] = json!(false);
        let host = fixture.path().join(case).join("host");
        write_package(
            &host,
            "alias-host",
            "",
            "pub fn typed(value: local_plugin_0::Token) -> root_plugin_0::Token { value }\n",
        );
        let manifest = json!({"package":{"name":"alias-host","version":"1.0.0","edition":"2024"},"workspace":{},
            "dependencies":{"local_plugin_0":{"package":format!("native-{case}"),"path":app},"root_plugin_0":alias}});
        fs::write(host.join("Cargo.toml"), toml::to_string(&manifest).unwrap()).unwrap();
        let result = resolve(&host.join("Cargo.toml"), &expected);
        if case == "full" {
            result.unwrap();
            command(&host, "cargo", &["check", "--locked", "--offline"]);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("target-normal dependency path")
            );
        }
    }
}

#[test]
fn real_cargo_distinguishes_host_only_and_target_optional_paths() {
    let fixture = tempfile::tempdir().unwrap();
    let owner = fixture.path().join("owner");
    write_package(
        &owner,
        "optional-owner",
        "[features]\nworkers=[]\n",
        "pub const WORKERS: bool=cfg!(feature=\"workers\"); pub struct Token;\n",
    );
    let bridge = fixture.path().join("bridge");
    write_package(
        &bridge,
        "shared-bridge",
        "[features]\nwith-owner=[\"dep:owner\",\"owner/workers\"]\n[dependencies]\nowner={package=\"optional-owner\",path=\"../owner\",optional=true,default-features=false}\n",
        "pub const BASE: bool=true;\n#[cfg(feature=\"with-owner\")] pub fn token() -> owner::Token { assert!(owner::WORKERS); owner::Token }\n",
    );
    for normal_requests_owner in [false, true] {
        let case = fixture.path().join(if normal_requests_owner {
            "normal-optional"
        } else {
            "host-only-optional"
        });
        let app = case.join("app");
        write_package(
            &app,
            "optional-app",
            &format!(
                "[dependencies]\nbridge={{package=\"shared-bridge\",path={bridge:?},default-features=false,features={features}}}\n[build-dependencies]\nbridge={{package=\"shared-bridge\",path={bridge:?},default-features=false,features=[\"with-owner\"]}}\n",
                bridge = bridge.to_str().unwrap(),
                features = if normal_requests_owner {
                    "[\"with-owner\"]"
                } else {
                    "[]"
                },
            ),
            if normal_requests_owner {
                "pub use bridge::token;\n"
            } else {
                "pub const BASE: bool=bridge::BASE;\n"
            },
        );
        fs::write(app.join("build.rs"), "fn main() { bridge::token(); }\n").unwrap();
        command(&app, "cargo", &["check"]);
        let host = case.join("host");
        write_package(
            &host,
            "optional-host",
            "",
            "const _: ()=assert!(root_plugin_0::WORKERS);\npub fn typed() { let _: root_plugin_0::Token=local_plugin_0::token(); }\n",
        );
        let manifest = json!({"package":{"name":"optional-host","version":"1.0.0","edition":"2024"},"workspace":{},
        "dependencies":{
            "local_plugin_0":{"package":"optional-app","path":app},
            "root_plugin_0":{"package":"optional-owner","path":owner,"default-features":false}
        }});
        fs::write(host.join("Cargo.toml"), toml::to_string(&manifest).unwrap()).unwrap();
        let output = command(
            &host,
            "cargo",
            &[
                "metadata",
                "--format-version=1",
                "--filter-platform",
                lenso_app_authoring::native_host_target(),
            ],
        );
        let metadata: Value = serde_json::from_str(&output).unwrap();
        let owner_id = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"] == "optional-owner")
            .unwrap()["id"]
            .as_str()
            .unwrap();
        let expected = BTreeMap::from([("root_plugin_0".into(), owner_id.to_owned())]);
        // Both graphs pass metadata's normal-edge projection. Only the second
        // has an actual target dependency supplying the requested features.
        verify(&metadata, &expected).unwrap();
        let result = resolve(&host.join("Cargo.toml"), &expected);
        if normal_requests_owner {
            result.unwrap();
            command(&host, "cargo", &["check", "--locked"]);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("after Cargo feature resolution")
            );
        }
    }
}
