use lenso_bootstrap::provider_runtime_plan_from_workspace;
use std::fs;

fn workspace(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "lenso-provider-workspace-{}-{suffix}",
        std::process::id()
    ))
}

#[test]
fn workspace_without_management_artifacts_is_linked_only() {
    let root = workspace("linked-only");
    fs::create_dir_all(&root).unwrap();

    let plan = provider_runtime_plan_from_workspace(&root).unwrap();

    assert!(plan.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn partial_provider_management_state_fails_closed() {
    let root = workspace("partial");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("lenso.modules.lock.json"), b"{}").unwrap();

    let error = provider_runtime_plan_from_workspace(&root).unwrap_err();

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
    assert!(error.public_message.contains("workspace is invalid"));
    fs::remove_dir_all(root).unwrap();
}
