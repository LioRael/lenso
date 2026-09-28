use std::collections::BTreeMap;

use base64::Engine as _;
use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Artifact, Availability, Distribution, DistributionKind, Envelope, Release,
    Snapshot as PortableSnapshot, Trust, digest,
    linked_cargo::{self, LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot},
    package::{self, PackageRelease, PackageSnapshot},
    release_content::{self, BaseKind, Content, ContentKind, ReleaseContent, Snapshot},
};

fn trust() -> (SigningKey, Trust) {
    let signing = SigningKey::from_bytes(&[43; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    (signing, trust)
}

fn portable() -> Release {
    Release {
        plugin_id: "example.web".into(),
        version: "1.2.3".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Web plugin".into(),
        description: String::new(),
        presentation: None,
        source_url: "https://example.test/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        artifact: Artifact {
            url: "https://example.test/portable.lenso-plugin".into(),
            digest: digest(b"portable"),
            size: 8,
            manifest_digest: digest(b"manifest"),
        },
        availability: Availability::Listed,
    }
}

fn linked() -> LinkedCargoRelease {
    LinkedCargoRelease {
        plugin_id: "example.web".into(),
        version: "1.2.3".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Web plugin".into(),
        source_url: "https://example.test/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-web-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: digest(b"crate"),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec!["aarch64-apple-darwin".into()],
        availability: Availability::Listed,
        documentation: Vec::new(),
    }
}

fn content(base_kind: BaseKind, base_release_identity: String) -> Snapshot {
    Snapshot::new(
        "catalog".into(),
        1,
        100,
        200,
        vec![ReleaseContent {
            plugin_id: "example.web".into(),
            version: "1.2.3".into(),
            base_kind,
            base_release_identity,
            content: vec![
                Content {
                    id: "react-template".into(),
                    kind: ContentKind::EditableTemplate,
                    url: "https://example.test/template.tar.gz".into(),
                    digest: digest(b"template"),
                    size: 8,
                },
                Content {
                    id: "compiler-extension".into(),
                    kind: ContentKind::DevelopmentExtension,
                    url: "https://example.test/compiler.tar.gz".into(),
                    digest: digest(b"extension"),
                    size: 9,
                },
            ],
        }],
    )
}

fn package() -> PackageRelease {
    PackageRelease {
        plugin_id: "example.web".into(),
        version: "1.2.3".into(),
        publisher_id: "example".into(),
        title: "Web".into(),
        summary: "Web plugin".into(),
        source_url: "https://example.test/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        distributions: vec![Distribution {
            id: "npm".into(),
            kind: DistributionKind::NpmPackage,
            package: "@example/web".into(),
            version: "1.2.3".into(),
            integrity: Some(digest(b"exact npm archive")),
            registry_url: Some("https://registry.npmjs.org".into()),
            artifact: None,
            targets: Vec::new(),
        }],
        availability: Availability::Listed,
        documentation: Vec::new(),
    }
}

#[test]
fn package_only_release_can_anchor_exact_content_without_portable_artifact() {
    let (signing, trust) = trust();
    let base = PackageSnapshot::new("catalog".into(), 1, 100, 200, vec![package()]);
    let verified_base = package::verify(
        &package::sign(&base, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let content = content(
        BaseKind::Package,
        base.releases[0].immutable_identity().unwrap(),
    );
    let verified_content = release_content::verify(
        &release_content::sign(&content, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    assert!(
        verified_content
            .select_package(&verified_base, "example.web", "1.2.3", 150)
            .is_ok()
    );
    assert!(
        verified_content
            .select_package(&verified_base, "example.web", "1.2.4", 150)
            .is_err()
    );
    assert!(
        verified_content
            .select_content_only("example.web", "1.2.3", 150)
            .is_err()
    );
    let mut revoked = package();
    revoked.availability = Availability::Revoked;
    let revoked = PackageSnapshot::new("catalog".into(), 2, 100, 200, vec![revoked]);
    let revoked = package::verify(
        &package::sign(&revoked, "key", &signing).unwrap(),
        &trust,
        Some(verified_base.checkpoint()),
        150,
    )
    .unwrap();
    assert!(
        verified_content
            .select_package(&revoked, "example.web", "1.2.3", 150)
            .is_err()
    );
}

#[test]
fn content_only_release_self_binds_exact_ordered_source_references() {
    let (signing, trust) = trust();
    let mut snapshot = content(BaseKind::ContentOnly, digest(b"placeholder"));
    snapshot.releases[0].base_release_identity =
        snapshot.releases[0].content_only_identity().unwrap();
    let signed = release_content::sign(&snapshot, "key", &signing).unwrap();
    let verified = release_content::verify(&signed, &trust, None, 150).unwrap();
    assert!(
        verified
            .select_content_only("example.web", "1.2.3", 150)
            .is_ok()
    );
    let mut changed = snapshot.clone();
    changed.releases[0].content.swap(0, 1);
    assert!(release_content::sign(&changed, "key", &signing).is_err());
    changed.releases[0].base_release_identity =
        changed.releases[0].content_only_identity().unwrap();
    let changed = release_content::sign(&changed, "key", &signing).unwrap();
    assert!(release_content::verify(&changed, &trust, Some(verified.checkpoint()), 150).is_err());
}

#[test]
fn joins_only_signed_current_exact_portable_or_linked_base() {
    let (signing, trust) = trust();
    let portable = PortableSnapshot::new("catalog".into(), 1, 100, 200, vec![portable()]);
    let verified_portable = lenso_plugin_catalog::verify(
        &lenso_plugin_catalog::sign(&portable, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let portable_content = content(
        BaseKind::Portable,
        portable.releases[0].immutable_identity().unwrap(),
    );
    let verified = release_content::verify(
        &release_content::sign(&portable_content, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    assert_eq!(
        verified
            .select_portable(&verified_portable, "example.web", "1.2.3", 150)
            .unwrap()
            .select("react-template")
            .unwrap()
            .kind,
        ContentKind::EditableTemplate
    );
    assert!(
        verified
            .select_portable(&verified_portable, "example.web", "1.2.4", 150)
            .is_err()
    );
    assert!(
        verified
            .select_portable(&verified_portable, "example.web", "1.2.3", 200)
            .is_err()
    );

    let linked = LinkedCargoSnapshot::new("catalog".into(), 1, 100, 200, vec![linked()]);
    let verified_linked = linked_cargo::verify(
        &linked_cargo::sign(&linked, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let linked_content = content(
        BaseKind::LinkedCargo,
        linked.releases[0].immutable_identity().unwrap(),
    );
    let verified = release_content::verify(
        &release_content::sign(&linked_content, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    assert!(
        verified
            .select_linked(&verified_linked, "example.web", "1.2.3", 150)
            .is_ok()
    );
    assert!(
        verified
            .select_portable(&verified_portable, "example.web", "1.2.3", 150)
            .is_err()
    );
}

#[test]
fn old_v1_signatures_and_wire_have_no_content_field() {
    let (signing, trust) = trust();
    let portable = PortableSnapshot::new("catalog".into(), 1, 100, 200, vec![portable()]);
    let linked = LinkedCargoSnapshot::new("catalog".into(), 1, 100, 200, vec![linked()]);
    let portable_bytes = lenso_plugin_catalog::sign(&portable, "key", &signing).unwrap();
    let linked_bytes = linked_cargo::sign(&linked, "key", &signing).unwrap();
    for bytes in [&portable_bytes, &linked_bytes] {
        let envelope: Envelope = serde_json::from_slice(bytes).unwrap();
        let payload = base64::engine::general_purpose::STANDARD
            .decode(envelope.payload_base64)
            .unwrap();
        let document: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert!(document["releases"][0].get("content").is_none());
    }
    lenso_plugin_catalog::verify(&portable_bytes, &trust, None, 150).unwrap();
    linked_cargo::verify(&linked_bytes, &trust, None, 150).unwrap();
}

#[test]
fn rejects_tampering_duplicate_identity_and_history_changes() {
    let (signing, trust) = trust();
    let mut snapshot = content(
        BaseKind::LinkedCargo,
        linked().immutable_identity().unwrap(),
    );
    let first = release_content::sign(&snapshot, "key", &signing).unwrap();
    let verified = release_content::verify(&first, &trust, None, 150).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_slice(&first).unwrap();
    envelope["payload_base64"] = serde_json::json!("AAAA");
    assert!(
        release_content::verify(&serde_json::to_vec(&envelope).unwrap(), &trust, None, 150)
            .is_err()
    );
    snapshot.revision = 2;
    snapshot.releases[0].content[0].digest = digest(b"changed");
    let changed = release_content::sign(&snapshot, "key", &signing).unwrap();
    assert!(release_content::verify(&changed, &trust, Some(verified.checkpoint()), 150).is_err());
    snapshot.releases[0].content[1].id = "react-template".into();
    assert!(release_content::sign(&snapshot, "key", &signing).is_err());
    snapshot.releases[0].content[1].id = "../escape".into();
    assert!(release_content::sign(&snapshot, "key", &signing).is_err());
    snapshot.releases[0].content[1].id = "compiler-extension".into();
    snapshot.releases[0].content[1].url = "http://127.0.0.1/extension.tar.gz".into();
    assert!(release_content::sign(&snapshot, "key", &signing).is_err());
}

#[test]
fn content_bytes_must_match_signed_size_and_digest() {
    let reference = content(BaseKind::Portable, digest(b"base"));
    let item = &reference.releases[0].content[0];
    item.verify_bytes(b"template").unwrap();
    assert!(item.verify_bytes(b"templatf").is_err());
    assert!(item.verify_bytes(b"template-more").is_err());
}

#[test]
fn accepts_marketplace_publisher_wire_without_resigning() {
    // Produced by the Marketplace publisher's release-content.v2 test, not this crate.
    const ENVELOPE: &str = r#"{"key_id":"key","payload_base64":"eyJzY2hlbWEiOiJsZW5zby5tYXJrZXRwbGFjZS5yZWxlYXNlLWNvbnRlbnQudjIiLCJjYXRhbG9nX2lkIjoiY2F0YWxvZyIsInJldmlzaW9uIjoxLCJpc3N1ZWRfYXQiOjEwMywiZXhwaXJlc19hdCI6MjAwLCJyZWxlYXNlcyI6W3sicGx1Z2luX2lkIjoiZXhhbXBsZS5lZGl0b3IiLCJ2ZXJzaW9uIjoiMS4wLjAiLCJiYXNlX2tpbmQiOiJsaW5rZWRfY2FyZ28iLCJiYXNlX3JlbGVhc2VfaWRlbnRpdHkiOiJzaGEyNTY6MWU3ZGExNDBiMGIxMmI3YmE3Zjg2MDU4MjI1YmQ0M2M4YTQyMjhjNjNjMmUwM2QyMjNiNzkzMDYxZmI0ZGE3OSIsImNvbnRlbnQiOlt7ImlkIjoicmVhY3Qtc3RhcnRlciIsImtpbmQiOiJlZGl0YWJsZV90ZW1wbGF0ZSIsInVybCI6Imh0dHBzOi8vZXhhbXBsZS5jb20vY29udGVudC9yZWFjdC1zdGFydGVyLnRhci5neiIsImRpZ2VzdCI6InNoYTI1Njo4OGIzN2ViYzRlYjFiZmYyZGM0MjVhNDg2ZTgyNjFjNTYxOGViNTY4NjRjOWJmMmYwMzEwOTRmZTc3ZDQ4M2JjIiwic2l6ZSI6MTA3fV19XX0=","signature_base64":"LASplCjzvkyn9xp7JUbIOeC9ejkgzn5fwTwL1ZOfKJWPGn9NvD9YpDPGB+mBjQXGvXcfQBGhvg3Cinem7lipCw=="}"#;
    let signing = SigningKey::from_bytes(&[7; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let verified = release_content::verify(ENVELOPE.as_bytes(), &trust, None, 150).unwrap();
    let release = &verified.snapshot().releases[0];
    assert_eq!(release.plugin_id, "example.editor");
    assert_eq!(release.version, "1.0.0");
    assert_eq!(release.base_kind, BaseKind::LinkedCargo);
    assert_eq!(release.content[0].id, "react-starter");
}
