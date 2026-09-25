use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use lenso_plugin_catalog::{
    Availability, Distribution, DistributionKind, Trust, digest, linked_cargo,
    package::{PackageRelease, PackageSnapshot, sign, verify},
};

fn release() -> PackageRelease {
    PackageRelease {
        plugin_id: "example.notes".into(),
        version: "1.2.3".into(),
        publisher_id: "example".into(),
        title: "Notes".into(),
        summary: "Notes Plugin".into(),
        source_url: "https://example.test/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        distributions: vec![Distribution {
            id: "npm".into(),
            kind: DistributionKind::NpmPackage,
            package: "@example/notes".into(),
            version: "4.5.6".into(),
            integrity: Some(digest(b"exact npm tarball")),
            registry_url: Some("https://registry.npmjs.org".into()),
            artifact: None,
            targets: vec![],
        }],
        availability: Availability::Listed,
        documentation: vec![],
    }
}

fn snapshot(revision: u64, release: PackageRelease) -> PackageSnapshot {
    PackageSnapshot::new("catalog".into(), revision, 100, 200, vec![release])
}

fn signing() -> (SigningKey, Trust) {
    let key = SigningKey::from_bytes(&[23; 32]);
    let trust = Trust {
        catalog_id: "catalog".into(),
        keys: BTreeMap::from([("key".into(), key.verifying_key())]),
    };
    (key, trust)
}

#[test]
fn npm_only_release_selects_exact_signed_package_without_portable_base() {
    let (key, trust) = signing();
    let signed = sign(&snapshot(1, release()), "key", &key).unwrap();
    let verified = verify(&signed, &trust, None, 150).unwrap();
    let selected = verified
        .select_npm("example.notes", "1.2.3", "npm", 150)
        .unwrap();
    assert_eq!(
        (
            selected.package.as_str(),
            selected.version.as_str(),
            selected.integrity.as_deref(),
        ),
        (
            "@example/notes",
            "4.5.6",
            Some(digest(b"exact npm tarball").as_str()),
        )
    );
}

#[test]
fn package_channel_signature_cannot_be_replayed_as_linked_cargo() {
    let (key, trust) = signing();
    let signed = sign(&snapshot(1, release()), "key", &key).unwrap();
    assert!(linked_cargo::verify(&signed, &trust, None, 150).is_err());
}

#[test]
fn package_release_rejects_portable_artifact() {
    let (key, _) = signing();
    let mut candidate = release();
    candidate.distributions[0].kind = DistributionKind::PortableBundle;
    assert!(sign(&snapshot(1, candidate), "key", &key).is_err());
}

#[test]
fn package_release_rejects_missing_or_invalid_npm_integrity() {
    let (key, _) = signing();
    let mut candidate = release();
    candidate.distributions[0].integrity = None;
    assert!(sign(&snapshot(1, candidate), "key", &key).is_err());
}

#[test]
fn package_release_rejects_invalid_npm_package_name() {
    let (key, _) = signing();
    let mut candidate = release();
    candidate.distributions[0].package = "@example/../notes".into();
    assert!(sign(&snapshot(1, candidate), "key", &key).is_err());
}

#[test]
fn package_release_rejects_non_https_registry() {
    let (key, _) = signing();
    let mut candidate = release();
    candidate.distributions[0].registry_url = Some("http://registry.npmjs.org".into());
    assert!(sign(&snapshot(1, candidate), "key", &key).is_err());
}

#[test]
fn package_checkpoint_rejects_rollback() {
    let (key, trust) = signing();
    let current = verify(
        &sign(&snapshot(2, release()), "key", &key).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let old = sign(&snapshot(1, release()), "key", &key).unwrap();
    assert!(verify(&old, &trust, Some(current.checkpoint()), 150).is_err());
}

#[test]
fn package_checkpoint_rejects_same_revision_equivocation() {
    let (key, trust) = signing();
    let current = verify(
        &sign(&snapshot(1, release()), "key", &key).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let mut changed = release();
    changed.availability = Availability::Yanked;
    let equivocation = sign(&snapshot(1, changed), "key", &key).unwrap();
    assert!(verify(&equivocation, &trust, Some(current.checkpoint()), 150).is_err());
}

#[test]
fn package_checkpoint_rejects_changed_tarball_at_new_revision() {
    let (key, trust) = signing();
    let current = verify(
        &sign(&snapshot(1, release()), "key", &key).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let mut changed = release();
    changed.distributions[0].integrity = Some(digest(b"different tarball"));
    let next = sign(&snapshot(2, changed), "key", &key).unwrap();
    assert!(verify(&next, &trust, Some(current.checkpoint()), 150).is_err());
}

#[test]
fn package_revocation_preserves_history_but_prevents_selection() {
    let (key, trust) = signing();
    let current = verify(
        &sign(&snapshot(1, release()), "key", &key).unwrap(),
        &trust,
        None,
        150,
    )
    .unwrap();
    let mut revoked = release();
    revoked.availability = Availability::Revoked;
    let next = sign(&snapshot(2, revoked), "key", &key).unwrap();
    let verified = verify(&next, &trust, Some(current.checkpoint()), 150).unwrap();
    assert!(
        verified
            .select_npm("example.notes", "1.2.3", "npm", 150)
            .is_err()
    );
}
