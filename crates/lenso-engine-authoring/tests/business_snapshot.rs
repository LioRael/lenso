use std::{
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

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

fn authorization(
    source: &FileBusinessSnapshotSource,
    max_stale: Duration,
) -> BusinessSnapshotAuthorization<ExcerptPolicy> {
    BusinessSnapshotAuthorization::new(
        BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap(),
        source.binding(),
        schema(),
        ["excerpt_limit"],
        max_stale,
    )
    .unwrap()
}

fn schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": { "excerpt_limit": { "type": "integer", "minimum": 16, "maximum": 512 } },
        "required": ["excerpt_limit"],
        "additionalProperties": false
    })
}

fn file_source(path: &Path) -> FileBusinessSnapshotSource {
    FileBusinessSnapshotSource::new(
        path,
        BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
    )
}

fn candidate(
    source: &FileBusinessSnapshotSource,
    revision: u64,
    value: serde_json::Value,
) -> VersionedBusinessSnapshot {
    fs::write(source.path(), document_value(revision, value)).unwrap();
    source.read().unwrap()
}

#[test]
fn request_keeps_its_authorized_business_revision_after_file_update() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let source = file_source(&path);
    let authority = BusinessSnapshotAuthority::new(authorization(&source, Duration::from_secs(60)));
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
    assert!(!format!("{next_request:?}").contains("excerpt_limit"));
}

fn document(revision: u64, excerpt_limit: u32) -> String {
    document_value(revision, json!({ "excerpt_limit": excerpt_limit }))
}

fn document_value(revision: u64, value: serde_json::Value) -> String {
    json!({
        "schema": "lenso.business-snapshot.v1",
        "revision": revision,
        "object": {
            "plugin_id": "company.notes",
            "instance_key": "default",
            "object_key": "excerpt-policy"
        },
        "value": value
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
    let directory = tempfile::tempdir().unwrap();
    let source = file_source(&directory.path().join("settings.json"));
    let authority = BusinessSnapshotAuthority::new(authorization(&source, Duration::from_secs(60)));
    assert_eq!(
        authority
            .accept(candidate(&source, 2, json!({ "excerpt_limit": 96 })), None)
            .unwrap(),
        BusinessSnapshotAcceptance::Activated
    );
    assert_eq!(
        authority
            .accept(
                candidate(&source, 2, json!({ "excerpt_limit": 96 })),
                Some(2)
            )
            .unwrap(),
        BusinessSnapshotAcceptance::Unchanged
    );
    assert!(
        authority
            .accept(
                candidate(&source, 2, json!({ "excerpt_limit": 48 })),
                Some(2)
            )
            .is_err()
    );
    assert!(
        authority
            .accept(
                candidate(&source, 1, json!({ "excerpt_limit": 48 })),
                Some(2)
            )
            .is_err()
    );
    assert!(
        authority
            .accept(
                candidate(&source, 3, json!({ "excerpt_limit": 48 })),
                Some(1)
            )
            .is_err()
    );
    assert!(
        authority
            .accept(
                candidate(&source, 3, json!({ "excerpt_limit": 600 })),
                Some(2)
            )
            .is_err()
    );
    let other_source = file_source(&directory.path().join("other-settings.json"));
    let other_source = candidate(&other_source, 999, json!({ "excerpt_limit": 48 }));
    assert!(authority.accept(other_source, Some(2)).is_err());
    let extra_field = candidate(
        &source,
        3,
        json!({ "excerpt_limit": 48, "allowed_origins": ["https://evil.example"] }),
    );
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
fn concurrent_business_revisions_allow_one_cas_winner() {
    let directory = tempfile::tempdir().unwrap();
    let source = file_source(&directory.path().join("settings.json"));
    let authority = Arc::new(BusinessSnapshotAuthority::new(authorization(
        &source,
        Duration::from_secs(60),
    )));
    authority
        .accept(candidate(&source, 1, json!({ "excerpt_limit": 96 })), None)
        .unwrap();
    let candidates = [(2, 48), (3, 64)]
        .map(|(revision, limit)| candidate(&source, revision, json!({ "excerpt_limit": limit })));
    let barrier = Arc::new(Barrier::new(3));
    let workers = candidates
        .into_iter()
        .map(|candidate| {
            let authority = Arc::clone(&authority);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                authority.accept(candidate, Some(1))
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let winners = workers
        .into_iter()
        .map(|worker| worker.join().unwrap().is_ok())
        .filter(|won| *won)
        .count();
    assert_eq!(winners, 1);
    assert!(matches!(authority.active_revision().unwrap(), Some(2 | 3)));
}

#[test]
fn source_outage_keeps_a_pinned_request_but_expires_new_admission() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(&path, document(1, 96)).unwrap();
    let source = file_source(&path);
    let authority =
        BusinessSnapshotAuthority::new(authorization(&source, Duration::from_millis(20)));
    authority.accept(source.read().unwrap(), None).unwrap();
    let admitted = authority.capture_request().unwrap();
    fs::remove_file(path).unwrap();
    assert!(source.read().is_err());
    thread::sleep(Duration::from_millis(50));
    assert!(authority.capture_request().is_err());
    assert_eq!(authority.active_revision().unwrap(), Some(1));
    assert_eq!(admitted.value().excerpt_limit, 96);
    assert_eq!(admitted.revision(), 1);
}

#[test]
fn authorization_rejects_invalid_deserialized_identity_and_unbounded_staleness() {
    let directory = tempfile::tempdir().unwrap();
    let source = file_source(&directory.path().join("settings.json"));
    let malformed: BusinessSnapshotObjectId = serde_json::from_value(json!({
        "plugin_id": "company.notes",
        "instance_key": "default",
        "object_key": "../other"
    }))
    .unwrap();
    assert!(
        BusinessSnapshotAuthorization::<ExcerptPolicy>::new(
            malformed,
            source.binding(),
            schema(),
            ["excerpt_limit"],
            Duration::from_secs(60)
        )
        .is_err()
    );
    assert!(
        BusinessSnapshotAuthorization::<ExcerptPolicy>::new(
            BusinessSnapshotObjectId::new("company.notes", "default", "excerpt-policy").unwrap(),
            source.binding(),
            schema(),
            ["excerpt_limit"],
            Duration::from_secs(24 * 60 * 60 + 1)
        )
        .is_err()
    );
}
