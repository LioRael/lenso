use super::dependency;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Output};

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn cargo(root: &Path, package: &str, arguments: &[&str]) -> Output {
    crate::app::cargo_command()
        .args(arguments)
        .arg("--manifest-path")
        .arg(root.join(package).join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .unwrap()
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn contract(root: &Path, directory: &str, name: &str) {
    write(
        &root.join(directory).join("Cargo.toml"),
        &format!(
            r#"[package]
name="{name}"
version="1.0.0"
edition="2024"
[features]
default=["default-only"]
default-only=[]
normal=[]
current=[]
build-only=[]
dev-only=[]
inactive=[]
"#
        ),
    );
    write(
        &root.join(directory).join("src/lib.rs"),
        r#"pub struct Token;
pub const FLAGS: [bool; 6] = [
    cfg!(feature="default-only"), cfg!(feature="normal"),
    cfg!(feature="current"), cfg!(feature="build-only"),
    cfg!(feature="dev-only"), cfg!(feature="inactive"),
];
#[cfg(feature="normal")]
pub fn normal(_: Token) {}
#[cfg(feature="current")]
pub fn current(_: Token) {}
"#,
    );
}

fn metadata_package(root: &Path, package: &str, name: &str) -> Value {
    let output = cargo(
        root,
        package,
        &[
            "metadata",
            "--offline",
            "--format-version=1",
            "--filter-platform",
            lenso_app_authoring::native_host_target(),
        ],
    );
    succeeded(&output);
    let graph: Value = serde_json::from_slice(&output.stdout).unwrap();
    graph["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == name)
        .unwrap()
        .clone()
}

fn host(root: &Path, dependencies: Value, code: &str) {
    let manifest = json!({
        "package":{"name":"generated-host","version":"1.0.0","edition":"2024"},
        "workspace":{"resolver":"2"},
        "dependencies":dependencies
    });
    write(
        &root.join("host/Cargo.toml"),
        &toml::to_string(&manifest).unwrap(),
    );
    write(&root.join("host/src/lib.rs"), code);
}

fn preserves_native_features(defaults: bool) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    contract(root, "contract", "typed-contract");
    let active = if cfg!(windows) { "windows" } else { "unix" };
    let inactive = if cfg!(windows) { "unix" } else { "windows" };
    write(
        &root.join("plugin/Cargo.toml"),
        &format!(
            r#"[package]
name="native-plugin"
version="1.0.0"
edition="2024"
[workspace]
resolver="2"
[dependencies]
typed-contract={{path="../contract",default-features={defaults},features=["normal"]}}
[target.'cfg({active})'.dependencies]
typed-contract={{path="../contract",default-features=false,features=["current"]}}
[build-dependencies]
typed-contract={{path="../contract",default-features=false,features=["build-only"]}}
[dev-dependencies]
typed-contract={{path="../contract",default-features=false,features=["dev-only"]}}
[target.'cfg({inactive})'.dependencies]
typed-contract={{path="../contract",default-features=false,features=["inactive"]}}
"#
        ),
    );
    write(
        &root.join("plugin/src/lib.rs"),
        "pub use typed_contract::Token;",
    );
    write(
        &root.join("plugin/build.rs"),
        "fn main() { assert!(typed_contract::FLAGS[3]); }",
    );
    let package = metadata_package(root, "plugin", "typed-contract");
    let identity = super::super::dependency(&package).unwrap();
    let alias = dependency(identity).unwrap();
    host(
        root,
        json!({"native-plugin":{"path":"../plugin"},"local-contract":alias}),
        &format!(
            r#"const _: () = {{
    assert!(local_contract::FLAGS[0] == {defaults});
    assert!(local_contract::FLAGS[1]);
    assert!(local_contract::FLAGS[2]);
    assert!(!local_contract::FLAGS[3]);
    assert!(!local_contract::FLAGS[4]);
    assert!(!local_contract::FLAGS[5]);
}};
pub fn typed_facade() {{
    local_contract::normal(native_plugin::Token);
    local_contract::current(native_plugin::Token);
}}
"#
        ),
    );
    succeeded(&cargo(root, "host", &["check", "--offline"]));
    succeeded(&cargo(root, "host", &["check", "--offline", "--locked"]));
}

#[test]
fn neutral_alias_retains_normal_and_active_target_features_without_defaults() {
    preserves_native_features(false);
}

#[test]
fn neutral_alias_preserves_defaults_requested_by_a_real_normal_edge() {
    preserves_native_features(true);
}

fn git(root: &Path, arguments: &[&str]) -> Output {
    let output = crate::app::build_command("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .unwrap();
    succeeded(&output);
    output
}

#[test]
fn raw_endpoint_identity_keeps_git_ingress_native_defaults() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let repository = root.join("web");
    contract(&repository, "endpoint", "lenso-capability-http-endpoint");
    write(
        &repository.join("Cargo.toml"),
        "[workspace]\nmembers=[\"endpoint\",\"ingress\"]\nresolver=\"2\"\n",
    );
    write(
        &repository.join("ingress/Cargo.toml"),
        r#"[package]
name="lenso-web-ingress-plugin"
version="0.4.11"
edition="2024"
[features]
default=["native"]
native=[]
[dependencies]
lenso-capability-http-endpoint={path="../endpoint",default-features=false}
"#,
    );
    write(
        &repository.join("ingress/src/lib.rs"),
        "#[cfg(feature=\"native\")] pub struct WebIngressFactory;",
    );
    git(&repository, &["init", "-b", "main"]);
    git(&repository, &["add", "Cargo.toml", "endpoint", "ingress"]);
    git(
        &repository,
        &[
            "-c",
            "user.name=Alias Fixture",
            "-c",
            "user.email=alias@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ],
    );
    let revision = git(&repository, &["rev-parse", "HEAD"]);
    let revision = String::from_utf8(revision.stdout).unwrap();
    let source = url::Url::from_directory_path(&repository).unwrap();
    let manifest = json!({
        "package":{"name":"native-plugin","version":"1.0.0","edition":"2024"},
        "workspace":{"resolver":"2"},
        "dependencies":{"lenso-capability-http-endpoint":{
            "git":source.as_str(), "rev":revision.trim(),
            "default-features":false, "features":["normal"]
        }}
    });
    write(
        &root.join("plugin/Cargo.toml"),
        &toml::to_string(&manifest).unwrap(),
    );
    write(
        &root.join("plugin/src/lib.rs"),
        "pub use lenso_capability_http_endpoint::Token;",
    );
    // Cargo's first fetch is required even for a local file:// repository.
    // These disposable manifests have no registry or remote Git dependencies.
    succeeded(&cargo(root, "plugin", &["fetch"]));
    let package = metadata_package(root, "plugin", "lenso-capability-http-endpoint");
    let identity = super::super::dependency(&package).unwrap();
    let alias = dependency(identity.clone()).unwrap();
    let ingress = super::super::web_ingress_dependency(&identity).unwrap();
    host(
        root,
        json!({
            "native-plugin":{"path":"../plugin"},
            "local-web-contract":alias,
            "lenso-web-ingress-plugin":ingress
        }),
        r#"const _: () = assert!(!local_web_contract::FLAGS[0]);
pub fn typed_ingress() {
    local_web_contract::normal(native_plugin::Token);
    let _ = lenso_web_ingress_plugin::WebIngressFactory;
}
"#,
    );
    succeeded(&cargo(root, "host", &["check", "--offline"]));
}
