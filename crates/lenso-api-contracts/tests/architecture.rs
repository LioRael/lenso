#[path = "support/architecture.rs"]
mod arch_check;

// Keep this suite focused on repository structure and compatibility rules.
// Generated artifact freshness belongs to `generated_artifacts`, so a stale
// contract has one owner and one failure path.
#[test]
fn architecture_rules_pass_for_current_workspace() {
    arch_check::run().expect("architecture rules should pass");
}

#[test]
fn public_application_lifecycle_must_be_complete_and_ordered() {
    let root = TestRepo::new();
    root.write(
        "README.md",
        r#"
## Public lifecycle

1. **Compose.** Materialize `lenso.app.json`.
3. **Connect.** Connect Console.
2. **Run locally.** Use `lenso system dev`.
4. **Status.** Inspect the System. Console does not release or deploy.
        "#,
    );
    root.write(
        "docs/getting-started.md",
        r#"
## Public lifecycle
### Compose
Materialize `lenso.app.json`.
### Run locally
Use `lenso system dev`.
### Connect
Connect Console.
### Status
Console does not release or deploy.
        "#,
    );

    let error = arch_check::check_public_application_lifecycle(root.path())
        .expect_err("an out-of-order lifecycle should fail");

    assert!(error.to_string().contains("out of order"), "{error}");
}

#[test]
fn typescript_service_kit_must_reject_autonomous_parity_claims() {
    let root = TestRepo::new();
    root.write(
        "README.md",
        "[Service Capability Tiers](docs/architecture/service-capability-tiers.md)",
    );
    root.write(
        "docs/architecture/service-capability-tiers.md",
        r#"
# Service Capability Tiers
## Provider
`lenso.service.v1` uses Rust and TypeScript.
## Autonomous Service
`lenso.service.v2` is Rust only with direct HTTP, direct gRPC, Event Contracts,
Durable Workflows, Workload Identity, Delegated Actor Context, and Service-owned storage.
        "#,
    );
    root.write(
        "sdk/typescript/packages/service-kit/README.md",
        "Provider tier only: `lenso.service.v1`. `lenso.service.v2` support is coming soon.",
    );

    let error = arch_check::check_service_capability_tiers(root.path())
        .expect_err("TypeScript must state its Autonomous Service non-parity");

    assert!(
        error
            .to_string()
            .contains("does not provide Autonomous Service parity"),
        "{error}",
    );
}

#[test]
fn curated_product_docs_reject_retired_workflow_vocabulary() {
    let root = TestRepo::new();
    root.write(
        "README.md",
        "Create an App Proof from the Launchpad change plan.",
    );

    let error = arch_check::check_retired_public_product_vocabulary(root.path())
        .expect_err("retired product vocabulary should fail");

    assert!(
        error.to_string().contains("retired public term `proof`"),
        "{error}",
    );
    assert!(
        error
            .to_string()
            .contains("retired public term `launchpad`"),
        "{error}",
    );
    assert!(
        error
            .to_string()
            .contains("retired public term `change plan`"),
        "{error}",
    );
}

#[test]
fn root_tooling_boundary_rejects_generic_tools_and_scripts() {
    let root = TestRepo::new();
    root.write("tools/check.sh", "#!/bin/sh\n");
    root.write("scripts/check.sh", "#!/bin/sh\n");

    let error = arch_check::check_root_tooling_boundary(root.path())
        .expect_err("generic root tooling should fail");

    assert!(error.to_string().contains("root tools/ must not exist"));
    assert!(error.to_string().contains("root scripts/ must not exist"));
}

#[test]
fn root_justfile_boundary_rejects_repository_task_runners() {
    let root = TestRepo::new();
    root.write("justfile", "check:\n    cargo test\n");

    let error = arch_check::check_root_justfile_boundary(root.path())
        .expect_err("repository task runners should fail");

    assert!(error.to_string().contains("root justfile must not exist"));
}

#[test]
fn runtime_function_without_contract_fails() {
    let root = TestRepo::new();
    root.write(
        "modules/identity/src/runtime/mod.rs",
        r#"
        use platform_runtime::FunctionDefinition;

        pub fn descriptor() {
            let _function = FunctionDefinition {
                name: "identity.cleanup_expired_sessions.v1".to_owned(),
                version: 1,
                queue: "identity".to_owned(),
                retry_policy: RetryPolicy::default(),
                handler: Arc::new(CleanupExpiredSessions),
            };
        }
        "#,
    );

    let error = arch_check::check_runtime_function_contracts(root.path())
        .expect_err("missing runtime function contract should fail");

    assert!(
        error
            .to_string()
            .contains("identity.cleanup_expired_sessions.v1 is missing"),
        "{error}",
    );
}

#[test]
fn runtime_function_constant_without_contract_fails() {
    let root = TestRepo::new();
    root.write(
        "modules/notifications/src/runtime/mod.rs",
        r#"
        pub const SEND_WELCOME_EMAIL: &str = "notifications.send_welcome_email.v1";

        pub fn descriptor() {
            let _function = FunctionDefinition {
                name: SEND_WELCOME_EMAIL.to_owned(),
                version: 1,
                queue: "notifications".to_owned(),
                retry_policy: RetryPolicy::default(),
                handler: Arc::new(SendWelcomeEmail),
            };
        }
        "#,
    );

    let error = arch_check::check_runtime_function_contracts(root.path())
        .expect_err("missing runtime function contract should fail");

    assert!(
        error
            .to_string()
            .contains("notifications.send_welcome_email.v1 is missing"),
        "{error}",
    );
}

#[test]
fn event_schema_ref_without_contract_fails() {
    let root = TestRepo::new();
    root.write(
        "modules/identity/src/commands/create_user.rs",
        r#"
        fn event() {
            let schema_ref = "contracts/events/identity/identity.user_registered.v1.schema.json";
        }
        "#,
    );

    let error = arch_check::check_event_schema_refs_exist(root.path())
        .expect_err("missing event schema reference should fail");

    assert!(
        error
            .to_string()
            .contains("contracts/events/identity/identity.user_registered.v1.schema.json"),
        "{error}",
    );
}

#[test]
fn event_contract_name_must_match_path() {
    let root = TestRepo::new();
    root.write(
        "contracts/events/identity/identity.user_registered.v1.schema.json",
        r#"{
          "$schema": "https://json-schema.org/draft/2020-12/schema",
          "$id": "identity.created.v1",
          "title": "identity.created.v1",
          "type": "object"
        }"#,
    );

    let error = arch_check::check_event_contract_names_match_paths(root.path())
        .expect_err("event contract title and id mismatch should fail");

    assert!(
        error.to_string().contains("identity.user_registered.v1"),
        "{error}",
    );
}

#[test]
fn cross_module_public_import_is_allowed() {
    let root = TestRepo::new();
    root.write("modules/auth/src/lib.rs", "pub mod public;");
    root.write(
        "modules/auth-password/src/lib.rs",
        "use auth::public::{self, AuthUserId};",
    );

    arch_check::check_forbidden_cross_module_imports(root.path())
        .expect("public module imports should be allowed");
}

#[test]
fn cross_module_internal_import_fails() {
    let root = TestRepo::new();
    root.write("modules/auth/src/lib.rs", "pub mod public;");
    root.write(
        "modules/auth-password/src/lib.rs",
        "use auth::repositories::PostgresAuthUserRepository;",
    );

    let error = arch_check::check_forbidden_cross_module_imports(root.path())
        .expect_err("internal module imports should fail");

    assert!(
        error
            .to_string()
            .contains("modules must call other modules through public interfaces"),
        "{error}",
    );
}

struct TestRepo {
    root: std::path::PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "lenso-architecture-test-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("test repo root should be created");
        Self { root }
    }

    fn path(&self) -> &std::path::Path {
        &self.root
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("test parent directory should be created");
        }
        std::fs::write(path, contents).expect("test file should be written");
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
