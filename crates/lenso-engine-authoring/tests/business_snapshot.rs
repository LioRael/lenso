use std::{fs, thread, time::Duration};

use lenso_engine_authoring::{
    BusinessSnapshotAcceptance, BusinessSnapshotAuthority, BusinessSnapshotAuthorization,
    BusinessSnapshotObjectId, BusinessSnapshotSourceId, FileBusinessSnapshotSource,
    HttpsBusinessSnapshotSource, VersionedBusinessSnapshot,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq)]
struct ExcerptPolicy {
    excerpt_limit: u32,
}

fn authorization() -> BusinessSnapshotAuthorization<ExcerptPolicy> {
    authorization_with_stale_limit(Duration::from_secs(60))
}

fn authorization_with_stale_limit(
    max_stale: Duration,
) -> BusinessSnapshotAuthorization<ExcerptPolicy> {
    BusinessSnapshotAuthorization::new(
        BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap(),
        BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
        json!({
            "type": "object",
            "properties": { "excerpt_limit": { "type": "integer", "minimum": 16, "maximum": 512 } },
            "required": ["excerpt_limit"],
            "additionalProperties": false
        }),
        ["excerpt_limit"],
        max_stale,
    )
    .unwrap()
}

fn candidate(revision: u64, excerpt_limit: u32) -> VersionedBusinessSnapshot {
    VersionedBusinessSnapshot::new(
        BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
        BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap(),
        revision,
        json!({ "excerpt_limit": excerpt_limit }),
    )
    .unwrap()
}

#[test]
fn request_keeps_its_authorized_business_revision_after_file_update() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let source = FileBusinessSnapshotSource::new(
        &path,
        BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
    );
    let authority = BusinessSnapshotAuthority::new(authorization());
    assert!(authority.capture_request().is_err());

    fs::write(&path, document(1, 96)).unwrap();
    authority.accept(source.read().unwrap(), None).unwrap();
    let first_request = authority.capture_request().unwrap();
    assert_eq!(first_request.revision(), 1);
    assert_eq!(first_request.value().excerpt_limit, 96);

    fs::write(&path, document(2, 48)).unwrap();
    authority.accept(source.read().unwrap(), Some(1)).unwrap();
    let next_request = authority.capture_request().unwrap();
    assert_eq!(first_request.value().excerpt_limit, 96);
    assert_eq!(first_request.revision(), 1);
    assert_eq!(next_request.value().excerpt_limit, 48);
    assert_eq!(next_request.revision(), 2);
}

fn document(revision: u64, excerpt_limit: u32) -> String {
    json!({
        "schema": "lenso.business-snapshot.v1",
        "revision": revision,
        "object": {
            "plugin_id": "company.notes",
            "instance_key": "default",
            "object_key": "excerpt-policy"
        },
        "value": { "excerpt_limit": excerpt_limit }
    })
    .to_string()
}

#[test]
fn https_source_requires_one_exact_host_admitted_origin() {
    let source = BusinessSnapshotSourceId::new("https_poll", "operator-settings").unwrap();
    assert!(
        HttpsBusinessSnapshotSource::new(
            "https://settings.example/snapshot",
            source.clone(),
            &["https://other.example/".to_owned()],
        )
        .is_err()
    );
    assert!(
        HttpsBusinessSnapshotSource::new(
            "http://settings.example/snapshot",
            source.clone(),
            &["https://settings.example/".to_owned()],
        )
        .is_err()
    );
    HttpsBusinessSnapshotSource::new(
        "https://settings.example/snapshot",
        source,
        &["https://settings.example/".to_owned()],
    )
    .unwrap();
}

#[test]
fn rejected_scope_stale_reuse_and_cas_leave_the_active_business_value_intact() {
    let authority = BusinessSnapshotAuthority::new(authorization());
    assert_eq!(
        authority.accept(candidate(2, 96), None).unwrap(),
        BusinessSnapshotAcceptance::Activated
    );
    assert_eq!(
        authority.accept(candidate(2, 96), Some(2)).unwrap(),
        BusinessSnapshotAcceptance::Unchanged
    );
    assert!(authority.accept(candidate(2, 48), Some(2)).is_err());
    assert!(authority.accept(candidate(1, 48), Some(2)).is_err());
    assert!(authority.accept(candidate(3, 48), Some(1)).is_err());
    assert!(authority.accept(candidate(3, 600), Some(2)).is_err());
    let other_source = VersionedBusinessSnapshot::new(
        BusinessSnapshotSourceId::new("file", "another-source").unwrap(),
        BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap(),
        999,
        json!({ "excerpt_limit": 48 }),
    )
    .unwrap();
    assert!(authority.accept(other_source, Some(2)).is_err());
    let extra_field = VersionedBusinessSnapshot::new(
        BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
        BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap(),
        3,
        json!({ "excerpt_limit": 48, "allowed_origins": ["https://evil.example"] }),
    )
    .unwrap();
    assert!(authority.accept(extra_field, Some(2)).is_err());
    assert_eq!(
        authority.capture_request().unwrap().value().excerpt_limit,
        96
    );
    assert_eq!(authority.capture_request().unwrap().revision(), 2);
}

#[test]
fn file_reader_rejects_links_and_public_debug_redacts_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, document(1, 96)).unwrap();
    let source_id = BusinessSnapshotSourceId::new("file", "operator-settings").unwrap();
    let source = FileBusinessSnapshotSource::new(&path, source_id.clone());
    let candidate = source.read().unwrap();
    assert!(!format!("{candidate:?}").contains("excerpt_limit"));

    #[cfg(unix)]
    {
        let linked = directory.path().join("settings-link.json");
        std::os::unix::fs::symlink(&path, &linked).unwrap();
        assert!(
            FileBusinessSnapshotSource::new(linked, source_id)
                .read()
                .is_err()
        );
    }
    assert!(
        FileBusinessSnapshotSource::new(
            "relative.json",
            BusinessSnapshotSourceId::new("file", "other").unwrap()
        )
        .read()
        .is_err()
    );
}

#[test]
fn source_outage_keeps_a_pinned_request_but_expires_new_admission() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, document(1, 96)).unwrap();
    let source = FileBusinessSnapshotSource::new(
        &path,
        BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
    );
    let authority =
        BusinessSnapshotAuthority::new(authorization_with_stale_limit(Duration::from_millis(20)));
    authority.accept(source.read().unwrap(), None).unwrap();
    let admitted = authority.capture_request().unwrap();
    fs::remove_file(path).unwrap();
    assert!(source.read().is_err());
    thread::sleep(Duration::from_millis(50));
    assert!(authority.capture_request().is_err());
    assert_eq!(admitted.value().excerpt_limit, 96);
    assert_eq!(admitted.revision(), 1);
}
