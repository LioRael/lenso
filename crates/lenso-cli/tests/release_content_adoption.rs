use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Artifact, Availability, Distribution, DistributionKind, Release, Snapshot as PortableSnapshot,
    digest,
    linked_cargo::{self, LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot},
    package::{self, PackageRelease, PackageSnapshot},
    release_content::{self, BaseKind, Content, ContentKind, ReleaseContent, Snapshot},
};

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for &(name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "signed snapshots, exact archive adoption, and non-installation checks form one end-to-end scenario"
)]
fn portable_base_can_anchor_content_without_installing_its_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let key = SigningKey::from_bytes(&[48; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let base = Release {
        plugin_id: "example.portable".into(),
        version: "2.0.0".into(),
        publisher_id: "example".into(),
        title: "Portable".into(),
        summary: "Portable Plugin".into(),
        description: String::new(),
        presentation: None,
        source_url: "https://example.test/portable".into(),
        source_revision: "b".repeat(40),
        license: "MIT".into(),
        artifact: Artifact {
            url: "https://example.test/portable.lenso-plugin".into(),
            digest: digest(b"portable runtime"),
            size: 16,
            manifest_digest: digest(b"manifest"),
        },
        availability: Availability::Listed,
    };
    let portable =
        PortableSnapshot::new("catalog".into(), 1, now - 1, now + 3600, vec![base.clone()]);
    let portable_path = temp.path().join("portable.json");
    fs::write(
        &portable_path,
        lenso_plugin_catalog::sign(&portable, "key", &key).unwrap(),
    )
    .unwrap();
    let bytes = archive(&[("README.md", b"Portable release template.\n")]);
    let archive_path = temp.path().join("content.tar.gz");
    fs::write(&archive_path, &bytes).unwrap();
    let content = Snapshot::new(
        "catalog".into(),
        1,
        now - 1,
        now + 3600,
        vec![ReleaseContent {
            plugin_id: "example.portable".into(),
            version: "2.0.0".into(),
            base_kind: BaseKind::Portable,
            base_release_identity: base.immutable_identity().unwrap(),
            content: vec![Content {
                id: "readme-template".into(),
                kind: ContentKind::EditableTemplate,
                url: "https://example.test/content.tar.gz".into(),
                digest: digest(&bytes),
                size: bytes.len() as u64,
            }],
        }],
    );
    let content_path = temp.path().join("content.json");
    fs::write(
        &content_path,
        release_content::sign(&content, "key", &key).unwrap(),
    )
    .unwrap();
    let trust_path = temp.path().join("trust.json");
    fs::write(
        &trust_path,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id":"catalog", "key_id":"key",
            "public_key_hex":hex::encode(key.verifying_key().to_bytes())
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(cli)
        .args(["app", "add", "example.portable@2.0.0", "--root"])
        .arg(&root)
        .arg("--portable-snapshot")
        .arg(&portable_path)
        .arg("--trust")
        .arg(&trust_path)
        .arg("--content-snapshot")
        .arg(&content_path)
        .args(["--content-id", "readme-template"])
        .arg("--content-archive")
        .arg(&archive_path)
        .args(["--content-destination", "frontend/readme"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(root.join("frontend/readme/README.md")).unwrap(),
        b"Portable release template.\n"
    );
    assert!(!root.join("vendor/lenso/portable").exists());
}

fn run(cli: &str, root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(cli)
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "two signed source-only content paths share one end-to-end copy scenario"
)]
fn pure_content_and_npm_only_release_copy_without_runtime_adoption() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let key = SigningKey::from_bytes(&[49; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let trust_path = temp.path().join("trust.json");
    fs::write(
        &trust_path,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id":"catalog", "key_id":"key",
            "public_key_hex":hex::encode(key.verifying_key().to_bytes())
        }))
        .unwrap(),
    )
    .unwrap();

    let template = archive(&[("README.md", b"Editable, app-owned source.\n")]);
    let template_path = temp.path().join("template.tar.gz");
    fs::write(&template_path, &template).unwrap();
    let extension = archive(&[
        ("package.json", br#"{"name":"example-dev","version":"1.0.0","type":"module","lenso":{"pluginId":"example.template","runtime":"bun","rootSlot":"tools","source":"index.ts","conventions":[{"id":"example.template.compiler","entries":["page.tsx"],"compiler":{"program":"bun","args":["compiler.mjs"]}}]}}"#),
        ("index.ts", b"export {};\n"),
        ("compiler.mjs", b"export {};\n"),
    ]);
    let extension_path = temp.path().join("extension.tar.gz");
    fs::write(&extension_path, &extension).unwrap();
    let npm = PackageRelease {
        plugin_id: "example.npm".into(),
        version: "1.0.0".into(),
        publisher_id: "example".into(),
        title: "Npm".into(),
        summary: "Npm plugin".into(),
        source_url: "https://example.test/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        distributions: vec![Distribution {
            id: "npm".into(),
            kind: DistributionKind::NpmPackage,
            package: "@example/npm".into(),
            version: "1.0.0".into(),
            integrity: Some(digest(b"npm tarball")),
            registry_url: Some("https://registry.npmjs.org".into()),
            artifact: None,
            targets: Vec::new(),
        }],
        availability: Availability::Listed,
        documentation: Vec::new(),
    };
    let package_path = temp.path().join("package.json");
    fs::write(
        &package_path,
        package::sign(
            &PackageSnapshot::new("catalog".into(), 1, now - 1, now + 3600, vec![npm.clone()]),
            "key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let mut pure = ReleaseContent {
        plugin_id: "example.template".into(),
        version: "1.0.0".into(),
        base_kind: BaseKind::ContentOnly,
        base_release_identity: digest(b"placeholder"),
        content: vec![Content {
            id: "dev-extension".into(),
            kind: ContentKind::DevelopmentExtension,
            url: "https://example.test/extension.tar.gz".into(),
            digest: digest(&extension),
            size: extension.len() as u64,
        }],
    };
    pure.base_release_identity = pure.content_only_identity().unwrap();
    let content_path = temp.path().join("content.json");
    fs::write(
        &content_path,
        release_content::sign(
            &Snapshot::new(
                "catalog".into(),
                1,
                now - 1,
                now + 3600,
                vec![
                    pure,
                    ReleaseContent {
                        plugin_id: "example.npm".into(),
                        version: "1.0.0".into(),
                        base_kind: BaseKind::Package,
                        base_release_identity: npm.immutable_identity().unwrap(),
                        content: vec![Content {
                            id: "editable".into(),
                            kind: ContentKind::EditableTemplate,
                            url: "https://example.test/template.tar.gz".into(),
                            digest: digest(&template),
                            size: template.len() as u64,
                        }],
                    },
                ],
            ),
            "key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();

    let pure_args = [
        "app",
        "add",
        "example.template@1.0.0",
        "--root",
        root.to_str().unwrap(),
        "--trust",
        trust_path.to_str().unwrap(),
        "--content-snapshot",
        content_path.to_str().unwrap(),
        "--content-id",
        "dev-extension",
        "--content-archive",
        extension_path.to_str().unwrap(),
        "--content-destination",
        "extensions/example-template",
    ];
    let preview = run(
        cli,
        &root,
        &[pure_args.as_slice(), &["--content-preview"]].concat(),
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(!root.join("extensions").exists());
    let copied = run(cli, &root, &pure_args);
    assert!(
        copied.status.success(),
        "{}",
        String::from_utf8_lossy(&copied.stderr)
    );
    assert!(
        root.join("extensions/example-template/package.json")
            .exists()
    );
    assert!(!root.join("plugins/example.template").exists());

    let npm_args = [
        "app",
        "add",
        "example.npm@1.0.0",
        "--root",
        root.to_str().unwrap(),
        "--package-snapshot",
        package_path.to_str().unwrap(),
        "--trust",
        trust_path.to_str().unwrap(),
        "--content-snapshot",
        content_path.to_str().unwrap(),
        "--content-id",
        "editable",
        "--content-archive",
        template_path.to_str().unwrap(),
        "--content-destination",
        "frontend/npm-example",
    ];
    let copied = run(cli, &root, &npm_args);
    assert!(
        copied.status.success(),
        "{}",
        String::from_utf8_lossy(&copied.stderr)
    );
    assert_eq!(
        fs::read(root.join("frontend/npm-example/README.md")).unwrap(),
        b"Editable, app-owned source.\n"
    );
    assert!(!root.join("vendor/lenso/npm/example.npm").exists());
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "preview, opt-in selection, and user-edit protections share one signed content lifecycle"
)]
fn exact_signed_content_previews_copies_and_preserves_user_edits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let key = SigningKey::from_bytes(&[47; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let linked = LinkedCargoRelease {
        plugin_id: "example.web".into(),
        version: "1.2.3".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Web Plugin".into(),
        source_url: "https://example.test/web".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-web-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: digest(b"exact linked crate"),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec![lenso_engine_authoring::native_host_target().into()],
        availability: Availability::Listed,
        documentation: Vec::new(),
    };
    let linked_snapshot = LinkedCargoSnapshot::new(
        "catalog".into(),
        1,
        now - 1,
        now + 3600,
        vec![linked.clone()],
    );
    let linked_path = temp.path().join("linked.json");
    fs::write(
        &linked_path,
        linked_cargo::sign(&linked_snapshot, "key", &key).unwrap(),
    )
    .unwrap();

    let bytes = archive(&[
        (
            "src/App.tsx",
            b"export const App = () => 'signed template';\n",
        ),
        ("README.md", b"Editable frontend source.\n"),
    ]);
    let extension = archive(&[
        ("package.json", br#"{"name":"example-web-extension","version":"1.2.3","type":"module","lenso":{"pluginId":"example.web","runtime":"bun","rootSlot":"tools","source":"index.ts","conventions":[{"id":"example.web.compiler","entries":["page.tsx"],"compiler":{"program":"bun","args":["compiler.mjs"]}}]}}"#),
        ("index.ts", b"export {};\n"),
        ("compiler.mjs", b"throw new Error('selected extension fixture');\n"),
    ]);
    let archive_path = temp.path().join("template.tar.gz");
    fs::write(&archive_path, &bytes).unwrap();
    let extension_path = temp.path().join("extension.tar.gz");
    fs::write(&extension_path, &extension).unwrap();
    let content_snapshot = Snapshot::new(
        "catalog".into(),
        1,
        now - 1,
        now + 3600,
        vec![ReleaseContent {
            plugin_id: "example.web".into(),
            version: "1.2.3".into(),
            base_kind: BaseKind::LinkedCargo,
            base_release_identity: linked.immutable_identity().unwrap(),
            content: vec![
                Content {
                    id: "frontend-template".into(),
                    kind: ContentKind::EditableTemplate,
                    url: "https://example.test/template.tar.gz".into(),
                    digest: digest(&bytes),
                    size: bytes.len() as u64,
                },
                Content {
                    id: "dev-extension".into(),
                    kind: ContentKind::DevelopmentExtension,
                    url: "https://example.test/extension.tar.gz".into(),
                    digest: digest(&extension),
                    size: extension.len() as u64,
                },
            ],
        }],
    );
    let content_path = temp.path().join("content.json");
    fs::write(
        &content_path,
        release_content::sign(&content_snapshot, "key", &key).unwrap(),
    )
    .unwrap();
    let trust_path = temp.path().join("trust.json");
    fs::write(
        &trust_path,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id":"catalog", "key_id":"key",
            "public_key_hex":hex::encode(key.verifying_key().to_bytes())
        }))
        .unwrap(),
    )
    .unwrap();

    let base = vec![
        "app",
        "add",
        "example.web@1.2.3",
        "--root",
        root.to_str().unwrap(),
        "--linked-snapshot",
        linked_path.to_str().unwrap(),
        "--trust",
        trust_path.to_str().unwrap(),
        "--content-snapshot",
        content_path.to_str().unwrap(),
        "--content-id",
        "frontend-template",
        "--content-archive",
        archive_path.to_str().unwrap(),
        "--content-destination",
        "frontend/from-release",
    ];
    let mut preview_args = base.clone();
    preview_args.push("--content-preview");
    let preview = run(cli, &root, &preview_args);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(report["content_id"], "frontend-template");
    assert_eq!(report["execution"], "not_selected");
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
    assert!(!root.join("frontend").exists());
    assert!(!root.join(".lenso").exists());

    let adopted = run(cli, &root, &base);
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    let destination = root.join("frontend/from-release");
    let original = fs::read(destination.join("src/App.tsx")).unwrap();
    assert_eq!(original, b"export const App = () => 'signed template';\n");
    let provenance: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join(".lenso-release-content.json")).unwrap())
            .unwrap();
    assert_eq!(provenance["plugin_id"], "example.web");
    assert_eq!(provenance["version"], "1.2.3");
    assert_eq!(provenance["content_digest"], digest(&bytes));
    assert!(!root.join("vendor/lenso/example.web").exists());
    assert!(!root.join("plugins/example.web").exists());

    let extension_args = base
        .iter()
        .map(|value| match *value {
            "frontend-template" => "dev-extension",
            "frontend/from-release" => "extensions/example-web",
            other if other == archive_path.to_str().unwrap() => extension_path.to_str().unwrap(),
            other => other,
        })
        .collect::<Vec<_>>();
    let adopted_extension = run(cli, &root, &extension_args);
    assert!(
        adopted_extension.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted_extension.stderr)
    );
    assert!(
        String::from_utf8_lossy(&adopted_extension.stdout)
            .contains("explicitly run `lenso app add")
    );
    assert!(!root.join("plugins/example.web").exists());
    let before_select = run(
        cli,
        &root,
        &[
            "app",
            "discover",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
    );
    assert!(before_select.status.success());
    let report: serde_json::Value = serde_json::from_slice(&before_select.stdout).unwrap();
    assert!(report["candidates"].as_array().unwrap().is_empty());

    let extension_source = root.join("extensions/example-web");
    let selected = Command::new(cli)
        .args(["app", "add"])
        .arg(&extension_source)
        .args(["--root", root.to_str().unwrap(), "--no-install"])
        .output()
        .unwrap();
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let after_select = run(
        cli,
        &root,
        &[
            "app",
            "discover",
            "--json",
            "--root",
            root.to_str().unwrap(),
        ],
    );
    assert!(after_select.status.success());
    let report: serde_json::Value = serde_json::from_slice(&after_select.stdout).unwrap();
    assert_eq!(report["candidates"][0]["plugin_id"], "example.web");
    assert_eq!(report["candidates"][0]["release_version"], "1.2.3");

    let changed = b"user's edited component\n";
    fs::write(destination.join("src/App.tsx"), changed).unwrap();
    let conflict = run(cli, &root, &base);
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("never overwritten"));
    assert_eq!(fs::read(destination.join("src/App.tsx")).unwrap(), changed);

    let wrong_version = run(
        cli,
        &root,
        &base
            .iter()
            .map(|value| {
                if *value == "example.web@1.2.3" {
                    "example.web@1.2.4"
                } else {
                    *value
                }
            })
            .collect::<Vec<_>>(),
    );
    assert!(!wrong_version.status.success());
    let bad_path = run(
        cli,
        &root,
        &base
            .iter()
            .map(|value| {
                if *value == "frontend/from-release" {
                    "../escape"
                } else {
                    *value
                }
            })
            .collect::<Vec<_>>(),
    );
    assert!(!bad_path.status.success());
    assert!(!temp.path().join("escape").exists());

    fs::write(&archive_path, b"changed archive").unwrap();
    let bad_digest = run(cli, &root, &base);
    assert!(!bad_digest.status.success());
    assert_eq!(fs::read(destination.join("src/App.tsx")).unwrap(), changed);
}
