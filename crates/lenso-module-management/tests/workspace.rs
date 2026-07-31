use lenso_module_management::*;
use std::fs;

fn temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "lenso-management-snapshot-{name}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn missing_planning_context_is_an_explicit_unconfigured_snapshot() {
    let root = temp_root("unconfigured");
    let snapshot = WorkspaceModuleManagement::new(&root).snapshot().unwrap();

    assert_eq!(snapshot.protocol, MODULE_MANAGEMENT_SNAPSHOT_PROTOCOL);
    assert_eq!(
        snapshot.status,
        ModuleManagementSnapshotStatus::Unconfigured
    );
    assert!(!snapshot.planning_available);
    assert!(snapshot.issues.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_target_owned_contract_is_reported_without_guessing_state() {
    let root = temp_root("invalid");
    fs::write(root.join("lenso.modules.json"), b"{not-json").unwrap();
    let snapshot = WorkspaceModuleManagement::new(&root).snapshot().unwrap();

    assert_eq!(snapshot.status, ModuleManagementSnapshotStatus::Invalid);
    assert_eq!(snapshot.issues, vec!["desired_composition_invalid"]);
    assert!(snapshot.desired.is_none());
    fs::remove_dir_all(root).unwrap();
}
