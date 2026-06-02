#[test]
fn architecture_rules_pass_for_current_workspace() {
    arch_check::run().expect("architecture rules should pass");
}

#[test]
fn runtime_function_without_contract_fails() {
    let root = TestRepo::new();
    root.write(
        "domains/identity/src/runtime/mod.rs",
        r#"
        use platform_runtime::FunctionDefinition;

        pub fn descriptor() {
            let _function = FunctionDefinition {
                name: "identity.cleanup_expired_sessions.v1",
                version: 1,
                queue: "identity",
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
        "domains/notifications/src/runtime/mod.rs",
        r#"
        pub const SEND_WELCOME_EMAIL: &str = "notifications.send_welcome_email.v1";

        pub fn descriptor() {
            let _function = FunctionDefinition {
                name: SEND_WELCOME_EMAIL,
                version: 1,
                queue: "notifications",
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
        "domains/identity/src/commands/create_user.rs",
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

struct TestRepo {
    root: std::path::PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "lenso-arch-check-test-{}-{}",
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
