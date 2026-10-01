use lenso_engine::discovery::DiscoveryLimits;
use lenso_engine::discovery::{AcquisitionStats, DiscoverySession};

#[test]
fn caller_budgets_apply_across_scoped_roots() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("a"), "123").unwrap();
    fs::write(root.path().join("nested/b"), "456").unwrap();
    let mut session = DiscoverySession::with_limits(
        root.path(),
        DiscoveryLimits {
            files: 1,
            bytes: 3,
            file_bytes: 3,
            entries: 2,
        },
    )
    .unwrap();
    session.read("a", 3).unwrap();
    assert!(
        session
            .scope(&root.path().join("nested"))
            .unwrap()
            .read("b", 3)
            .is_err()
    );
    assert!(session.read("a", 4).is_err());
    session.directory("").unwrap();
    assert!(session.directory("nested").is_err());
}
use std::fs;

#[test]
fn scoped_roots_share_physical_identity_and_invalidate_each_other() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("nested/input.json"), "1").unwrap();
    let mut parent = DiscoverySession::new(root.path()).unwrap();
    let mut child = parent.scope(&root.path().join("nested")).unwrap();
    assert_eq!(
        parent.directory("nested").unwrap()[0].path,
        "nested/input.json"
    );
    assert_eq!(child.directory("").unwrap()[0].path, "input.json");
    parent.json("nested/input.json", 100).unwrap();
    child.json("input.json", 100).unwrap();
    assert_eq!(
        parent.stats(),
        AcquisitionStats {
            directory_reads: 1,
            file_reads: 1,
            json_parses: 1
        }
    );
    fs::write(root.path().join("nested/input.json"), "2").unwrap();
    child.invalidate("input.json").unwrap();
    assert_eq!(*parent.json("nested/input.json", 100).unwrap(), 2);
    child.begin_epoch();
    assert_eq!(parent.stats(), AcquisitionStats::default());
    assert_eq!(*child.json("input.json", 100).unwrap(), 2);
    assert_eq!(parent.stats().file_reads, 1);
}

#[test]
fn overlapping_readers_share_listing_bytes_and_json_but_keep_limits() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("inputs")).unwrap();
    fs::write(root.path().join("inputs/a.json"), br#"{"value":1}"#).unwrap();
    let mut session = DiscoverySession::new(root.path()).unwrap();
    for _ in 0..2 {
        assert_eq!(session.directory("inputs").unwrap().len(), 1);
        session.read("inputs/a.json", 1024).unwrap();
        assert_eq!(session.json("inputs/a.json", 1024).unwrap()["value"], 1);
    }
    assert_eq!(
        session.stats(),
        AcquisitionStats {
            directory_reads: 1,
            file_reads: 1,
            json_parses: 1
        }
    );
    assert!(session.read("inputs/a.json", 1).is_err());
}

#[test]
fn events_invalidate_changes_additions_deletions_and_preserve_other_inputs() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("inputs")).unwrap();
    fs::write(root.path().join("inputs/a.json"), "1").unwrap();
    fs::write(root.path().join("inputs/b.json"), "2").unwrap();
    let mut session = DiscoverySession::new(root.path()).unwrap();
    session.directory("inputs").unwrap();
    session.json("inputs/a.json", 100).unwrap();
    session.json("inputs/b.json", 100).unwrap();
    fs::write(root.path().join("inputs/a.json"), "3").unwrap();
    assert_eq!(*session.json("inputs/a.json", 100).unwrap(), 1);
    session.invalidate("inputs/a.json").unwrap();
    assert_eq!(*session.json("inputs/a.json", 100).unwrap(), 3);
    session.json("inputs/b.json", 100).unwrap();
    assert_eq!(session.stats().file_reads, 3);
    fs::write(root.path().join("inputs/c.json"), "4").unwrap();
    session.invalidate("inputs/c.json").unwrap();
    assert_eq!(session.directory("inputs").unwrap().len(), 3);
    fs::remove_file(root.path().join("inputs/a.json")).unwrap();
    session.invalidate("inputs/a.json").unwrap();
    assert_eq!(session.directory("inputs").unwrap().len(), 2);
    assert!(session.read("inputs/a.json", 100).is_err());
}

#[test]
fn fresh_epoch_observes_configuration_and_does_not_replay_build_inputs() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("options.json"), "1").unwrap();
    let mut session = DiscoverySession::new(root.path()).unwrap();
    session.json("options.json", 100).unwrap();
    fs::write(root.path().join("options.json"), "2").unwrap();
    session.begin_epoch();
    assert_eq!(*session.json("options.json", 100).unwrap(), 2);
    assert_eq!(session.stats().file_reads, 1);
    assert!(session.read("../options.json", 100).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_replacement_cannot_return_cached_bytes() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(root.path().join("input"), "safe").unwrap();
    fs::write(outside.path().join("input"), "outside").unwrap();
    let mut session = DiscoverySession::new(root.path()).unwrap();
    session.read("input", 100).unwrap();
    fs::remove_file(root.path().join("input")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("input"), root.path().join("input")).unwrap();
    assert!(session.read("input", 100).is_err());
}
