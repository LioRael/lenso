# Remote Runtime Functions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Register remote module runtime function declarations into the host `FunctionRegistry` and prove the existing worker status machine executes them through the remote invocation protocol.

**Architecture:** The host remains the runtime owner. `ModuleManifest::runtime.functions` stays pure data, `RemoteModuleSource` validates and converts those declarations into a source-specific `RemoteBinding`, and the worker continues to execute ordinary `runtime.function_runs` with a proxy-backed `RuntimeFunction` handler.

**Tech Stack:** Rust 2024, async-trait, reqwest, axum test servers, sqlx/Postgres test database, `platform-runtime`, `platform-module`, `platform-module-remote`, `app-bootstrap`.

---

## File Structure

- `crates/platform-runtime/src/functions.rs`: make `FunctionDefinition.name` and `queue` owned strings so remote declarations loaded at runtime can register without leaking memory.
- `crates/platform-runtime/src/registry.rs`: keep linked descriptors working while accepting owned function definitions.
- `domains/identity/src/runtime/mod.rs`, `domains/notifications/src/runtime/mod.rs`, `crates/platform-module/src/linked.rs`, `crates/platform-runtime/tests/function_runtime.rs`: update linked call sites to construct owned function definitions.
- `crates/platform-module-remote/src/binding.rs`: store remote-generated `FunctionDefinition` values and register them.
- `crates/platform-module-remote/src/source.rs`: validate remote runtime declarations during manifest load and build `RemoteBinding` from manifest runtime data plus `RemoteModuleConfig`.
- `crates/platform-module-remote/src/runtime.rs`: expose a small helper for validating runtime function path segments and keep `RemoteRuntimeFunction` as the invocation client.
- `crates/platform-module-remote/tests/remote_source.rs`: assert loaded remote modules register manifest-declared runtime functions.
- `examples/remote-module/src/lib.rs`, `examples/remote-module/tests/protocol.rs`: serve the first runtime function invoke endpoint in the remote fixture.
- `crates/platform-module-remote/Cargo.toml`, `crates/platform-module-remote/tests/remote_worker.rs`: add DB-backed worker tests for remote success, retryable failure, timeout, and host-side missing registration.
- `docs/architecture/module-remote-runtime.md`: mark implementation steps as they land and record any non-obvious choices.

---

### Task 1: Runtime Registry Accepts Owned Function Definitions

**Files:**
- Modify: `crates/platform-runtime/src/functions.rs`
- Modify: `crates/platform-runtime/src/registry.rs`
- Modify: `domains/identity/src/runtime/mod.rs`
- Modify: `domains/notifications/src/runtime/mod.rs`
- Modify: `crates/platform-module/src/linked.rs`
- Modify: `crates/platform-runtime/tests/function_runtime.rs`

- [ ] **Step 1: Write the focused regression test**

Add this test near `can_register_function` in `crates/platform-runtime/tests/function_runtime.rs`:

```rust
#[test]
fn can_register_runtime_loaded_function_names() {
    let mut registry = FunctionRegistry::default();
    let function_name = format!("{}.{}", "remote_crm", "sync_contact.v1");

    registry.register(FunctionDefinition {
        name: function_name.clone(),
        version: 1,
        queue: "remote-crm".to_owned(),
        retry_policy: RetryPolicy::default(),
        handler: Arc::new(Succeeds),
    });

    let definition = registry
        .get("remote_crm.sync_contact.v1")
        .expect("function should register");
    assert_eq!(definition.name, function_name);
    assert_eq!(definition.queue, "remote-crm");
}
```

- [ ] **Step 2: Run the failing test**

Run:

```bash
cargo test --locked -p platform-runtime can_register_runtime_loaded_function_names
```

Expected: compilation fails because `FunctionDefinition.name` and `queue` are `&'static str`.

- [ ] **Step 3: Make `FunctionDefinition` owned**

Change `crates/platform-runtime/src/functions.rs`:

```rust
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub version: u16,
    pub queue: String,
    pub retry_policy: RetryPolicy,
    pub handler: Arc<dyn FunctionHandler>,
}
```

Keep `FunctionRegistry::register` as:

```rust
pub fn register(&mut self, function: FunctionDefinition) {
    self.functions.insert(function.name.clone(), function);
}
```

Change `RuntimeWorker::run_claimed` context construction to clone the owned queue:

```rust
queue: definition.queue.clone(),
```

- [ ] **Step 4: Update linked call sites**

Use `.to_owned()` for every linked function definition:

```rust
FunctionDefinition {
    name: "identity.cleanup_expired_sessions.v1".to_owned(),
    version: 1,
    queue: "identity".to_owned(),
    retry_policy: RetryPolicy::default(),
    handler: Arc::new(CleanupExpiredSessions),
}
```

For notifications:

```rust
FunctionDefinition {
    name: SEND_WELCOME_EMAIL.to_owned(),
    version: 1,
    queue: "notifications".to_owned(),
    retry_policy: RetryPolicy::default(),
    handler: Arc::new(SendWelcomeEmail),
}
```

In test helpers:

```rust
fn test_function(name: &str, handler: Arc<dyn RuntimeFunction>) -> FunctionDefinition {
    test_function_with_retry_policy(name, RetryPolicy::fixed(3, Duration::ZERO), handler)
}

fn test_function_with_retry_policy(
    name: &str,
    retry_policy: RetryPolicy,
    handler: Arc<dyn RuntimeFunction>,
) -> FunctionDefinition {
    FunctionDefinition {
        name: name.to_owned(),
        version: 1,
        queue: "default".to_owned(),
        retry_policy,
        handler,
    }
}
```

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo test --locked -p platform-runtime
cargo test --locked -p platform-module
```

Expected: all tests pass.

Commit:

```bash
git add crates/platform-runtime/src/functions.rs crates/platform-runtime/src/registry.rs domains/identity/src/runtime/mod.rs domains/notifications/src/runtime/mod.rs crates/platform-module/src/linked.rs crates/platform-runtime/tests/function_runtime.rs
git commit -m "refactor(runtime): own function definitions"
```

---

### Task 2: Remote Binding Registers Manifest Functions

**Files:**
- Modify: `crates/platform-module-remote/src/binding.rs`
- Modify: `crates/platform-module-remote/src/source.rs`
- Modify: `crates/platform-module-remote/src/runtime.rs`
- Modify: `crates/platform-module-remote/src/proxy.rs`
- Modify: `crates/platform-module-remote/tests/remote_source.rs`

- [ ] **Step 1: Add binding tests**

Add to `crates/platform-module-remote/src/binding.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{
        RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
    };

    #[test]
    fn remote_binding_registers_declared_functions() {
        let binding = RemoteBinding::from_runtime_surface(
            RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
            Some(&RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 3,
                        initial_delay_ms: 1000,
                    }),
                }],
            }),
        )
        .expect("remote binding should build");

        let mut registry = FunctionRegistry::default();
        binding.register_functions(&mut registry);

        let definition = registry
            .get("remote_crm.sync_contact.v1")
            .expect("remote function should register");
        assert_eq!(definition.version, 1);
        assert_eq!(definition.queue, "remote-crm");
        assert_eq!(definition.retry_policy.max_attempts, 3);
        assert_eq!(
            definition.retry_policy.initial_delay,
            std::time::Duration::from_millis(1000)
        );
    }
}
```

- [ ] **Step 2: Run the failing test**

Run:

```bash
cargo test --locked -p platform-module-remote remote_binding_registers_declared_functions
```

Expected: fails because `RemoteBinding::from_runtime_surface` does not exist.

- [ ] **Step 3: Implement `RemoteBinding` conversion**

Replace `RemoteBinding` in `crates/platform-module-remote/src/binding.rs` with:

```rust
use crate::config::RemoteModuleConfig;
use crate::runtime::RemoteRuntimeFunction;
use platform_core::{AppResult, EventHandlerRegistry};
use platform_module::{ModuleBinding, RuntimeSurface};
use platform_runtime::{FunctionDefinition, FunctionRegistry, RetryPolicy};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct RemoteBinding {
    functions: Vec<FunctionDefinition>,
}

impl RemoteBinding {
    pub fn from_runtime_surface(
        config: RemoteModuleConfig,
        runtime: Option<&RuntimeSurface>,
    ) -> AppResult<Self> {
        let functions = runtime
            .into_iter()
            .flat_map(|surface| surface.functions.iter())
            .map(|declaration| {
                Ok(FunctionDefinition {
                    name: declaration.name.clone(),
                    version: declaration.version,
                    queue: declaration.queue.clone(),
                    retry_policy: declaration
                        .retry_policy
                        .as_ref()
                        .map(|policy| {
                            RetryPolicy::fixed(
                                policy.max_attempts,
                                Duration::from_millis(policy.initial_delay_ms),
                            )
                        })
                        .unwrap_or_default(),
                    handler: Arc::new(RemoteRuntimeFunction::new(
                        config.clone(),
                        declaration.name.clone(),
                    )?),
                })
            })
            .collect::<AppResult<Vec<_>>>()?;

        Ok(Self { functions })
    }
}

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        for function in self.functions.iter().cloned() {
            registry.register(function);
        }
    }

    fn register_event_handlers(&self, _registry: &mut EventHandlerRegistry) {}
}
```

- [ ] **Step 4: Wire source load to build the binding**

In `crates/platform-module-remote/src/source.rs`, after HTTP route validation:

```rust
validate_remote_http_routes(&manifest.http_routes)?;
let binding = RemoteBinding::from_runtime_surface(
    self.config.clone(),
    manifest.runtime.as_ref(),
)?;
```

Then build the module with:

```rust
let mut module = Module::remote(manifest, Arc::new(binding));
```

Update `crates/platform-module-remote/src/proxy.rs` test helper:

```rust
std::sync::Arc::new(crate::RemoteBinding::default())
```

- [ ] **Step 5: Add source integration assertion**

In `loads_manifest_and_attaches_admin_data_source`, after loading:

```rust
let mut registry = platform_runtime::FunctionRegistry::default();
module.binding.register_functions(&mut registry);
assert!(registry.get("remote_crm.sync_contact.v1").is_some());
```

- [ ] **Step 6: Verify and commit**

Run:

```bash
cargo test --locked -p platform-module-remote
cargo test --locked -p app-bootstrap
```

Expected: all tests pass.

Commit:

```bash
git add crates/platform-module-remote/src/binding.rs crates/platform-module-remote/src/source.rs crates/platform-module-remote/src/runtime.rs crates/platform-module-remote/src/proxy.rs crates/platform-module-remote/tests/remote_source.rs
git commit -m "feat(runtime): register remote function bindings"
```

---

### Task 3: Remote Example Executes Runtime Function Invocations

**Files:**
- Modify: `examples/remote-module/src/lib.rs`
- Modify: `examples/remote-module/tests/protocol.rs`

- [ ] **Step 1: Add protocol tests for the example invocation endpoint**

Add to `examples/remote-module/tests/protocol.rs`:

```rust
#[tokio::test]
async fn runtime_function_invoke_returns_output_envelope() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/lenso/module/v1/runtime/functions/remote_crm.sync_contact.v1/invoke")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    r#"{"function_run_id":"fnrun_1","function_name":"remote_crm.sync_contact.v1","attempt":1,"correlation_id":"corr_1","causation_id":"httpreq_1","actor":{"kind":"service","service_id":"worker","scopes":[]},"trace":{"trace_id":"trace_1","span_id":"span_1","baggage":[]},"input":{"contact_id":"contact_1"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(value["output"]["synced"], true);
    assert_eq!(value["output"]["contact_id"], "contact_1");
    assert_eq!(value["output"]["function_run_id"], "fnrun_1");
}
```

- [ ] **Step 2: Run the failing test**

Run:

```bash
cargo test --locked -p remote-module-example runtime_function_invoke_returns_output_envelope
```

Expected: FAIL with 404 because the endpoint is not served yet.

- [ ] **Step 3: Implement the example endpoint**

Add request DTOs and route in `examples/remote-module/src/lib.rs`:

```rust
#[derive(Debug, Deserialize)]
struct RuntimeFunctionInvokeRequest {
    function_run_id: String,
    function_name: String,
    attempt: u32,
    correlation_id: String,
    causation_id: Option<String>,
    actor: Value,
    trace: Value,
    input: Value,
}
```

Add to `router()`:

```rust
.route(
    "/lenso/module/v1/runtime/functions/{function_name}/invoke",
    post(invoke_runtime_function),
)
```

Add handler:

```rust
async fn invoke_runtime_function(
    Path(function_name): Path<String>,
    Json(request): Json<RuntimeFunctionInvokeRequest>,
) -> Response {
    if function_name != "remote_crm.sync_contact.v1"
        || request.function_name != "remote_crm.sync_contact.v1"
    {
        return remote_error(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("runtime function {function_name} was not found"),
            false,
        );
    }

    Json(json!({
        "output": {
            "synced": true,
            "contact_id": request
                .input
                .get("contact_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "function_run_id": request.function_run_id,
            "attempt": request.attempt,
            "correlation_id": request.correlation_id,
            "causation_id": request.causation_id,
            "actor_kind": request.actor.get("kind").and_then(Value::as_str).unwrap_or(""),
            "trace_id": request.trace.get("trace_id").and_then(Value::as_str).unwrap_or(""),
        }
    }))
    .into_response()
}
```

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo test --locked -p remote-module-example
cargo test --locked -p platform-module-remote
```

Expected: all tests pass.

Commit:

```bash
git add examples/remote-module/src/lib.rs examples/remote-module/tests/protocol.rs
git commit -m "feat(remote-module): serve runtime function invoke"
```

---

### Task 4: Worker Executes Remote Functions Through Existing Status Machine

**Files:**
- Modify: `crates/platform-module-remote/Cargo.toml`
- Create: `crates/platform-module-remote/tests/remote_worker.rs`

- [ ] **Step 1: Add dev dependencies**

In `crates/platform-module-remote/Cargo.toml`:

```toml
[dev-dependencies]
platform-testing.workspace = true
sqlx.workspace = true
tokio.workspace = true
```

- [ ] **Step 2: Add success worker test**

Create `crates/platform-module-remote/tests/remote_worker.rs` with a test server, migration helper, registry helper, and this test:

```rust
#[tokio::test]
async fn worker_completes_remote_runtime_function() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let remote = spawn_remote(runtime_success_router()).await;
    let mut registry = FunctionRegistry::default();
    registry.register(remote_definition(&remote, "remote_crm.sync_contact.v1", 3, 0));

    enqueue(&db.pool, "remote_crm.sync_contact.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-remote");
    let count = worker
        .claim_and_run_batch(10)
        .await
        .expect("remote function should run");

    assert_eq!(count, 1);
    assert_eq!(run_status(&db.pool, "remote_crm.sync_contact.v1").await, "completed");
    assert!(
        execution_log_bodies(&db.pool, "remote_crm.sync_contact.v1")
            .await
            .contains(&"Function run completed".to_owned())
    );

    db.cleanup().await;
}
```

Use these helper signatures in the same file:

```rust
async fn spawn_remote(router: Router) -> String;
fn remote_definition(
    base_url: &str,
    function_name: &str,
    max_attempts: u32,
    initial_delay_ms: u64,
) -> FunctionDefinition;
async fn enqueue(pool: &platform_core::DbPool, function_name: &str, max_attempts: i32) -> String;
async fn run_status(pool: &platform_core::DbPool, function_name: &str) -> String;
async fn execution_log_bodies(pool: &platform_core::DbPool, function_name: &str) -> Vec<String>;
async fn apply_runtime_stack_migrations(db: &TestDatabase);
```

- [ ] **Step 3: Add retryable failure, timeout, and missing registration tests**

Add three tests to the same file:

```rust
#[tokio::test]
async fn worker_retries_remote_runtime_failure() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let remote = spawn_remote(runtime_retryable_failure_router()).await;
    let mut registry = FunctionRegistry::default();
    registry.register(remote_definition(&remote, "remote_crm.sync_contact.v1", 3, 0));
    enqueue(&db.pool, "remote_crm.sync_contact.v1", 3).await;

    RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-remote")
        .claim_and_run_batch(10)
        .await
        .expect("worker should handle remote failure");

    assert_eq!(
        run_status_and_attempts(&db.pool, "remote_crm.sync_contact.v1").await,
        ("failed".to_owned(), 1)
    );

    db.cleanup().await;
}

#[tokio::test]
async fn worker_marks_remote_runtime_timeout_failed() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let remote = spawn_remote(runtime_slow_router()).await;
    let mut registry = FunctionRegistry::default();
    registry.register(remote_definition_with_timeout(
        &remote,
        "remote_crm.sync_contact.v1",
        10,
    ));
    enqueue(&db.pool, "remote_crm.sync_contact.v1", 3).await;

    RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-remote")
        .claim_and_run_batch(10)
        .await
        .expect("worker should handle timeout");

    assert_eq!(
        run_status_and_attempts(&db.pool, "remote_crm.sync_contact.v1").await,
        ("failed".to_owned(), 1)
    );

    db.cleanup().await;
}

#[tokio::test]
async fn worker_does_not_create_remote_story_when_function_is_unregistered() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;
    enqueue(&db.pool, "remote_crm.missing.v1", 1).await;

    RuntimeWorker::new(
        db.pool.clone(),
        Arc::new(FunctionRegistry::default()),
        "worker-remote",
    )
    .claim_and_run_batch(10)
    .await
    .expect("worker should handle missing registration");

    assert_eq!(
        run_status_and_attempts(&db.pool, "remote_crm.missing.v1").await,
        ("dead".to_owned(), 1)
    );

    db.cleanup().await;
}
```

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo test --locked -p platform-module-remote --test remote_worker
cargo test --locked -p platform-module-remote
```

Expected: all tests pass.

Commit:

```bash
git add crates/platform-module-remote/Cargo.toml crates/platform-module-remote/tests/remote_worker.rs
git commit -m "test(runtime): cover remote worker execution"
```

---

### Task 5: Bootstrap And Docs Checkpoint

**Files:**
- Modify: `docs/architecture/module-remote-runtime.md`
- Inspect: `crates/app-bootstrap/src/lib.rs`
- Inspect: `apps/worker/src/main.rs`

- [ ] **Step 1: Confirm worker already uses async module loading**

Run:

```bash
rg "load_modules|function_registry" apps/worker/src/main.rs crates/app-bootstrap/src/lib.rs
```

Expected: `apps/worker` calls `app_bootstrap::load_modules(&ctx).await`, then `app_bootstrap::function_registry(&modules)`.

- [ ] **Step 2: Mark implementation order**

Update `docs/architecture/module-remote-runtime.md`:

```markdown
4. Register remote function handlers into `FunctionRegistry` during module
   loading. Done.
5. Add worker/runtime tests proving success, retryable failure, exhausted
   attempts, timeout, and missing remote function behavior. Done.
```

- [ ] **Step 3: Run broad checks**

Run:

```bash
cargo fmt --check
just arch-check
just rust-check
```

Expected: all checks pass. If `just rust-check` exposes unrelated failures, record the failure and still commit only related files after narrow checks are green.

- [ ] **Step 4: Commit docs**

Commit:

```bash
git add docs/architecture/module-remote-runtime.md
git commit -m "docs(runtime): checkpoint remote function execution"
```

---

## Self-Review

- Spec coverage: host-owned queues/retries/stories stay in `platform-runtime`; remote modules only execute through `RemoteRuntimeFunction`; direct outbox/event handler support remains absent.
- Placeholder scan: all implementation tasks identify exact files, commands, expected outcomes, and concrete code shapes.
- Type consistency: `FunctionDefinition` becomes owned in Task 1, and Task 2 uses owned remote declarations directly without string leaks.
