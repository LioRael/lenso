use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Artifact, Availability, Distribution, DistributionKind, Documentation, Release, ReleaseDetails,
    ReleaseDetailsSnapshot, Snapshot, Trust, digest,
    linked_cargo::{self, LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot},
    sign, sign_release_details, verify, verify_release_details,
};

fn artifact() -> Artifact {
    Artifact {
        url: "https://example.test/notes.lenso-plugin".into(),
        digest: digest(b"bundle"),
        size: 6,
        manifest_digest: digest(b"manifest"),
    }
}

fn details() -> ReleaseDetailsSnapshot {
    let base = base();
    ReleaseDetailsSnapshot::new(
        "catalog".into(),
        1,
        100,
        200,
        vec![ReleaseDetails {
            plugin_id: "example.notes".into(),
            version: "1.2.3".into(),
            base_release_identity: base.releases[0].immutable_identity().unwrap(),
            distributions: vec![
                Distribution {
                    id: "portable".into(),
                    kind: DistributionKind::PortableBundle,
                    package: "example.notes".into(),
                    version: "1.2.3".into(),
                    integrity: None,
                    registry_url: None,
                    artifact: Some(artifact()),
                    targets: vec!["wasm32-wasip2".into()],
                },
                Distribution {
                    id: "linked-rust".into(),
                    kind: DistributionKind::CargoPackage,
                    package: "example-notes-plugin".into(),
                    version: "1.2.3".into(),
                    integrity: Some(digest(b"crate-package")),
                    registry_url: Some("https://crates.io".into()),
                    artifact: None,
                    targets: vec!["aarch64-apple-darwin".into()],
                },
            ],
            documentation: vec![Documentation {
                id: "quickstart-en".into(),
                revision: "1".into(),
                language: "en".into(),
                topic: "quickstart".into(),
                target: Some("aarch64-apple-darwin".into()),
                url: "https://example.test/docs/1.2.3/quickstart.md".into(),
                digest: digest(b"docs"),
                size: 4,
                media_type: "text/markdown".into(),
            }],
        }],
    )
}

fn base() -> Snapshot {
    Snapshot::new(
        "catalog".into(),
        1,
        100,
        200,
        vec![Release {
            plugin_id: "example.notes".into(),
            version: "1.2.3".into(),
            publisher_id: "example".into(),
            title: "Notes".into(),
            summary: "Notes".into(),
            description: String::new(),
            presentation: None,
            source_url: "https://example.test/source".into(),
            source_revision: "a".repeat(40),
            license: "MIT".into(),
            artifact: artifact(),
            availability: Availability::Listed,
        }],
    )
}

fn linked_release() -> LinkedCargoRelease {
    LinkedCargoRelease {
        plugin_id: "example.notes".into(),
        version: "1.2.3".into(),
        publisher_id: "example".into(),
        title: "Notes".into(),
        summary: "Notes".into(),
        source_url: "https://example.test/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-notes-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: digest(b"crate-package"),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec!["aarch64-apple-darwin".into()],
        availability: Availability::Listed,
        documentation: vec![],
    }
}

fn linked_details() -> ReleaseDetailsSnapshot {
    let linked = linked_release();
    let mut snapshot = details();
    let release = &mut snapshot.releases[0];
    release.base_release_identity = linked.immutable_identity().unwrap();
    release.distributions.remove(0);
    release.distributions.push(Distribution {
        id: "bun".into(),
        kind: DistributionKind::NpmPackage,
        package: "@example/notes-plugin".into(),
        version: linked.version.clone(),
        integrity: Some(digest(b"npm-package")),
        registry_url: Some("https://registry.npmjs.org".into()),
        artifact: None,
        targets: vec!["linux-arm64".into()],
    });
    snapshot
}

#[test]
fn signed_details_metadata_joins_linked_cargo_and_npm_without_portable_base() {
    let signing = SigningKey::from_bytes(&[16; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let linked = LinkedCargoSnapshot::new("catalog".into(), 1, 100, 200, vec![linked_release()]);
    let linked = linked_cargo::verify(
        &linked_cargo::sign(&linked, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let details = verify_release_details(
        &sign_release_details(&linked_details(), "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();

    let selected = linked
        .select_details(&details, "example.notes", "1.2.3", 150)
        .unwrap();
    assert_eq!(selected.distributions.len(), 2);
    assert_eq!(
        selected.distributions[0].kind,
        DistributionKind::CargoPackage
    );
    assert_eq!(selected.distributions[1].kind, DistributionKind::NpmPackage);
    assert!(
        linked
            .select_details(&details, "example.notes", "1.2.3", 201)
            .is_err()
    );

    let portable = verify(&sign(&base(), "key", &signing).unwrap(), &trust, None, 150).unwrap();
    assert!(
        portable
            .select_details(&details, "example.notes", "1.2.3", 150)
            .is_err()
    );
}

#[test]
fn linked_details_reject_unbound_or_missing_cargo_coordinates() {
    let linked = linked_release();
    let matching = linked_details().releases.remove(0);
    assert!(matching.validate_against_linked(&linked).is_ok());

    for field in 0..5 {
        let mut changed = matching.clone();
        let cargo = &mut changed.distributions[0];
        match field {
            0 => cargo.package = "another-plugin".into(),
            1 => cargo.version = "1.2.4".into(),
            2 => cargo.registry_url = Some("https://other.example.test".into()),
            3 => cargo.integrity = Some(digest(b"different-crate")),
            4 => cargo.targets = vec!["x86_64-unknown-linux-gnu".into()],
            _ => unreachable!(),
        }
        assert!(
            changed.validate_against_linked(&linked).is_err(),
            "field {field}"
        );
    }

    let mut changed = matching.clone();
    changed.base_release_identity = digest(b"different-linked-base");
    assert!(changed.validate_against_linked(&linked).is_err());

    let mut changed = matching.clone();
    changed.distributions.remove(0);
    assert!(changed.validate_against_linked(&linked).is_err());

    let mut changed = matching;
    changed
        .distributions
        .push(details().releases[0].distributions[0].clone());
    assert!(changed.validate_against_linked(&linked).is_err());

    let mut changed = linked_details();
    changed.releases[0].distributions[1].version = "9.9.9".into();
    assert!(sign_release_details(&changed, "key", &SigningKey::from_bytes(&[17; 32])).is_err());
}

#[test]
fn verifies_multi_distribution_details_without_changing_base_catalog_types() {
    let signing = SigningKey::from_bytes(&[7; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let envelope = sign_release_details(&details(), "key", &signing).unwrap();
    let base = verify(&sign(&base(), "key", &signing).unwrap(), &trust, None, 150).unwrap();

    let verified = verify_release_details(&envelope, &trust, None, 150).unwrap();
    let selected = base
        .select_details(&verified, "example.notes", "1.2.3", 150)
        .unwrap();

    assert_eq!(selected.distributions.len(), 2);
    assert_eq!(selected.documentation[0].digest, digest(b"docs"));
}

#[test]
fn immutable_details_and_distribution_shapes_fail_closed() {
    let signing = SigningKey::from_bytes(&[9; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let first = sign_release_details(&details(), "key", &signing).unwrap();
    let accepted = verify_release_details(&first, &trust, None, 150).unwrap();
    let mut errata = details();
    errata.revision = 2;
    errata.releases[0].documentation[0].revision = "2".into();
    errata.releases[0].documentation[0].url = "https://example.test/changed.md".into();
    let errata = sign_release_details(&errata, "key", &signing).unwrap();
    assert!(verify_release_details(&errata, &trust, Some(accepted.checkpoint()), 150).is_ok());

    let mut mutated = details();
    mutated.revision = 2;
    mutated.releases[0].documentation[0].url = "https://example.test/changed.md".into();
    let mutated = sign_release_details(&mutated, "key", &signing).unwrap();
    assert!(verify_release_details(&mutated, &trust, Some(accepted.checkpoint()), 150).is_err());

    let mut invalid = details();
    invalid.releases[0].distributions[1].artifact =
        invalid.releases[0].distributions[0].artifact.clone();
    assert!(sign_release_details(&invalid, "key", &signing).is_err());

    let mut wrong_version = details();
    wrong_version.releases[0].distributions[1].version = "1.2.4".into();
    assert!(sign_release_details(&wrong_version, "key", &signing).is_err());
}

#[test]
fn details_must_join_the_exact_immutable_base_release() {
    let signing = SigningKey::from_bytes(&[11; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let base = verify(&sign(&base(), "key", &signing).unwrap(), &trust, None, 150).unwrap();
    let mut mismatched = details();
    mismatched.releases[0].base_release_identity = digest(b"different-base-release");
    let details = verify_release_details(
        &sign_release_details(&mismatched, "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();

    assert!(
        base.select_details(&details, "example.notes", "1.2.3", 150)
            .is_err()
    );
}

#[test]
fn details_follow_a_legitimate_base_artifact_url_rotation() {
    let signing = SigningKey::from_bytes(&[13; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let mut rotated = base();
    rotated.revision = 2;
    rotated.releases[0].artifact.url = "https://cdn.example.test/notes.lenso-plugin".into();
    let base = verify(&sign(&rotated, "key", &signing).unwrap(), &trust, None, 150).unwrap();
    let details = verify_release_details(
        &sign_release_details(&details(), "key", &signing).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();

    assert!(
        base.select_details(&details, "example.notes", "1.2.3", 150)
            .is_ok()
    );
}

#[test]
fn rejects_invalid_package_manager_coordinates() {
    let signing = SigningKey::from_bytes(&[12; 32]);
    for invalid in ["---", "1crate"] {
        let mut snapshot = details();
        snapshot.releases[0].distributions[1].package = invalid.into();
        assert!(sign_release_details(&snapshot, "key", &signing).is_err());
    }

    for invalid in ["@/", "a/b/c", "foo@bar", "Uppercase"] {
        let mut snapshot = details();
        let distribution = &mut snapshot.releases[0].distributions[1];
        distribution.kind = DistributionKind::NpmPackage;
        distribution.package = invalid.into();
        assert!(sign_release_details(&snapshot, "key", &signing).is_err());
    }

    let mut scoped = details();
    let distribution = &mut scoped.releases[0].distributions[1];
    distribution.kind = DistributionKind::NpmPackage;
    distribution.package = "@lenso/example-notes".into();
    assert!(sign_release_details(&scoped, "key", &signing).is_ok());
}

#[test]
fn rejects_oversized_versions_and_checkpoint_history() {
    let signing = SigningKey::from_bytes(&[14; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let mut oversized = details();
    oversized.releases[0].version = format!("1.2.3+{}", "a".repeat(128));
    oversized.releases[0].distributions[0].version = oversized.releases[0].version.clone();
    oversized.releases[0].distributions[1].version = oversized.releases[0].version.clone();
    assert!(sign_release_details(&oversized, "key", &signing).is_err());

    let envelope = sign_release_details(&details(), "key", &signing).unwrap();
    let verified = verify_release_details(&envelope, &trust, None, 150).unwrap();
    let mut checkpoint = verified.checkpoint().clone();
    checkpoint.document_identities = (0..65_537)
        .map(|index| (format!("doc-{index}"), digest(b"doc")))
        .collect();
    assert!(verify_release_details(&envelope, &trust, Some(&checkpoint), 150).is_err());
}

#[test]
fn maximum_length_document_identity_round_trips() {
    let signing = SigningKey::from_bytes(&[15; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), signing.verifying_key())]),
    };
    let plugin_id = [
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61),
    ]
    .join(".");
    let version = format!("1.2.3+{}", "a".repeat(122));
    assert_eq!(plugin_id.len(), 253);
    assert_eq!(version.len(), 128);
    let mut snapshot = details();
    let release = &mut snapshot.releases[0];
    release.plugin_id = plugin_id;
    release.version = version.clone();
    release.distributions[0].version = version.clone();
    release.distributions[1].version = version;
    release.documentation[0].id = "i".repeat(128);
    release.documentation[0].revision = "r".repeat(128);

    let envelope = sign_release_details(&snapshot, "key", &signing).unwrap();
    assert!(verify_release_details(&envelope, &trust, None, 150).is_ok());
}
