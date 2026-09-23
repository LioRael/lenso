use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Availability, Documentation, Trust, digest,
    linked_cargo::{LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot, sign, verify},
    verify as verify_portable,
};

fn snapshot() -> LinkedCargoSnapshot {
    LinkedCargoSnapshot::new(
        "catalog".into(),
        1,
        100,
        200,
        vec![LinkedCargoRelease {
            plugin_id: "example.notes".into(),
            version: "1.2.3".into(),
            publisher_id: "example".into(),
            title: "Notes".into(),
            summary: "Linked Notes Web Plugin".into(),
            source_url: "https://github.com/example/notes".into(),
            source_revision: "a".repeat(40),
            license: "MIT".into(),
            package: "example-notes-plugin".into(),
            registry_url: "https://crates.io".into(),
            crate_digest: digest(b"exact crate archive"),
            integration: LinkedCargoIntegration::LinkedPlugin,
            targets: vec!["aarch64-apple-darwin".into()],
            availability: Availability::Listed,
            documentation: vec![Documentation {
                id: "quickstart".into(),
                revision: "1".into(),
                language: "en".into(),
                topic: "quickstart".into(),
                target: None,
                url: "https://example.test/docs/notes/1.2.3.md".into(),
                digest: digest(b"docs"),
                size: 4,
                media_type: "text/markdown".into(),
            }],
        }],
    )
}

fn keys() -> (SigningKey, Trust) {
    let key = SigningKey::from_bytes(&[23; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), key.verifying_key())]),
    };
    (key, trust)
}

#[test]
fn source_only_release_is_signed_but_is_not_a_portable_bundle() {
    let (key, trust) = keys();
    let bytes = sign(&snapshot(), "key", &key).unwrap();
    let verified = verify(&bytes, &trust, None, 150).unwrap();
    let selected = verified.select("example.notes", "1.2.3", 150).unwrap();
    assert_eq!(selected.package, "example-notes-plugin");
    assert_eq!(selected.crate_digest, digest(b"exact crate archive"));
    assert!(verify_portable(&bytes, &trust, None, 150).is_err());
    assert!(verified.select("example.notes", "1.2.3", 201).is_err());
}

#[test]
fn immutable_crate_input_and_document_revisions_fail_closed() {
    let (key, trust) = keys();
    let first = verify(&sign(&snapshot(), "key", &key).unwrap(), &trust, None, 150).unwrap();
    let mut changed = snapshot();
    changed.revision = 2;
    changed.releases[0].crate_digest = digest(b"different crate archive");
    let bytes = sign(&changed, "key", &key).unwrap();
    assert!(verify(&bytes, &trust, Some(first.checkpoint()), 150).is_err());

    let mut changed = snapshot();
    changed.revision = 2;
    changed.releases[0].integration = LinkedCargoIntegration::HostProvided;
    let bytes = sign(&changed, "key", &key).unwrap();
    assert!(verify(&bytes, &trust, Some(first.checkpoint()), 150).is_err());

    let mut changed = snapshot();
    changed.revision = 2;
    changed.releases[0].documentation[0].url = "https://example.test/replaced.md".into();
    let bytes = sign(&changed, "key", &key).unwrap();
    assert!(verify(&bytes, &trust, Some(first.checkpoint()), 150).is_err());

    let mut errata = snapshot();
    errata.revision = 2;
    errata.releases[0].documentation[0].revision = "2".into();
    errata.releases[0].documentation[0].url = "https://example.test/replaced.md".into();
    let bytes = sign(&errata, "key", &key).unwrap();
    assert!(verify(&bytes, &trust, Some(first.checkpoint()), 150).is_ok());
}

#[test]
fn availability_and_revision_are_fenced_independently_of_payload_identity() {
    let (key, trust) = keys();
    let first = verify(&sign(&snapshot(), "key", &key).unwrap(), &trust, None, 150).unwrap();
    let mut yanked = snapshot();
    yanked.revision = 2;
    yanked.releases[0].availability = Availability::Yanked;
    let bytes = sign(&yanked, "key", &key).unwrap();
    let second = verify(&bytes, &trust, Some(first.checkpoint()), 150).unwrap();
    assert!(second.select("example.notes", "1.2.3", 150).is_err());
    assert!(
        verify(
            &sign(&snapshot(), "key", &key).unwrap(),
            &trust,
            Some(second.checkpoint()),
            150
        )
        .is_err()
    );
    let mut invalid = snapshot();
    invalid.revision = 2;
    invalid.releases[0].crate_digest = "sha256:invalid".into();
    assert!(sign(&invalid, "key", &key).is_err());
}
