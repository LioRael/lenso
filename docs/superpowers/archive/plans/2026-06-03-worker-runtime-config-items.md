# Worker Runtime Config Items Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the worker's poll interval and batch size live-tunable runtime config items (`worker.poll_interval_ms`, `worker.batch_size`), consumed by the running worker loop, and remove the orphan static `WorkerConfig`.

**Architecture:** Thread `batch_size` from the `OutboxRelay`/`RuntimeWorker` constructors into per-call arguments so it can change per tick; add a `WorkerRuntimeConfig` typed struct + two `Service("worker")` descriptors in `platform-core`; aggregate platform descriptors at the composition root; the worker loop reads the snapshot each tick to drive both the sleep interval and the batch size. Defaults match the current hardcoded values (500ms / 25) for zero behavior change on first deploy.

**Tech Stack:** Rust 2024 (cargo), sqlx/Postgres, the existing `runtime_config` system (RuntimeConfigDescriptor/Registry/Snapshot/Provider).

**Reference spec:** `docs/superpowers/specs/2026-06-03-worker-runtime-config-items.md`

**Verified facts about the current code:**
- `OutboxRelay` (`crates/platform-core/src/outbox.rs:249-263`): struct `{ pool, worker_id, batch_size: i64 }`; `new(pool, worker_id, batch_size)`; `claim_batch(&self)` binds `self.batch_size` (line 308); `relay_once(&self, dispatcher)` calls `self.claim_batch()` (line 344).
- `RuntimeWorker` (`crates/platform-runtime/src/functions.rs:220-330`): struct `{ pool, registry, worker_id, batch_size: i64 }`; `new(pool, registry, worker_id, batch_size)`; `claim_batch(&self)` binds `self.batch_size`; `claim_and_run_batch(&self)` calls `self.claim_batch()`.
- Callers/tests: `apps/worker/src/main.rs:53,54,66,74`; `crates/platform-core/tests/outbox_relay.rs` (lines 16,19,39,41,67,69,93,95 — `new(.., 10)`, `claim_batch()`, `relay_once(..)`); `crates/platform-runtime/tests/function_runtime.rs` (lines 92,94,128,130,160,162 — `new(.., 10)`, `claim_and_run_batch()`).
- `platform-core` already exports the runtime_config system; `RuntimeConfigDescriptor`, `RuntimeConfigScope`, `RuntimeConfigType`, `RuntimeConfigRegistry`, `RuntimeConfigSnapshot` are at the crate root.
- `app_bootstrap::runtime_config_descriptors(ctx) -> Vec<RuntimeConfigDescriptor>` flat-maps `domain.runtime_config`.
- Static `WorkerConfig` lives at `crates/platform-core/src/config.rs:147-158`; `AppConfig.worker` field (line 10); `from_env` init (line 23); re-export in `lib.rs`.

---

## File Structure

**New:**
- `crates/platform-core/src/worker_runtime_config.rs` — `WorkerRuntimeConfig` struct + `RUNTIME_CONFIG` descriptor list.
- `crates/platform-core/tests/worker_runtime_config.rs` — integration test (snapshot read + provider refresh).

**Modified:**
- `crates/platform-core/src/outbox.rs` — `OutboxRelay`: drop `batch_size` field/param, thread it through `claim_batch`/`relay_once`.
- `crates/platform-runtime/src/functions.rs` — `RuntimeWorker`: same.
- `crates/platform-core/src/lib.rs` — export `WorkerRuntimeConfig`, `worker_runtime_config` module; remove `WorkerConfig` export.
- `crates/platform-core/src/config.rs` — remove static `WorkerConfig` + `AppConfig.worker`.
- `crates/app-bootstrap/src/lib.rs` — chain platform worker descriptors into `runtime_config_descriptors`.
- `apps/worker/src/main.rs` — read snapshot per tick; drive interval + batch.
- `crates/platform-core/tests/outbox_relay.rs`, `crates/platform-runtime/tests/function_runtime.rs` — update call sites.

---

## Task 1: Thread `batch_size` through `OutboxRelay`

Move `batch_size` out of the constructor/field into the `claim_batch`/`relay_once` call arguments.

**Files:**
- Modify: `crates/platform-core/src/outbox.rs`
- Modify: `crates/platform-core/tests/outbox_relay.rs`

- [ ] **Step 1: Update the `OutboxRelay` struct and `new`**

In `crates/platform-core/src/outbox.rs`, change the struct (line ~249) to drop the field:
```rust
#[derive(Debug, Clone)]
pub struct OutboxRelay {
    pool: DbPool,
    worker_id: String,
}

impl OutboxRelay {
    pub fn new(pool: DbPool, worker_id: impl Into<String>) -> Self {
        Self {
            pool,
            worker_id: worker_id.into(),
        }
    }
```

- [ ] **Step 2: Add `batch_size` param to `claim_batch` and `relay_once`**

Change `claim_batch`'s signature and the bind:
```rust
    pub async fn claim_batch(&self, batch_size: i64) -> AppResult<Vec<ClaimedOutboxEvent>> {
```
Inside it, change `.bind(self.batch_size)` → `.bind(batch_size)`.

Change `relay_once`'s signature and its internal call:
```rust
    pub async fn relay_once(
        &self,
        dispatcher: &dyn EventDispatcher,
        batch_size: i64,
    ) -> AppResult<usize> {
```
Inside `relay_once`, change `let events = self.claim_batch().await?;` → `let events = self.claim_batch(batch_size).await?;`.

- [ ] **Step 3: Update the outbox_relay tests**

In `crates/platform-core/tests/outbox_relay.rs`, update all call sites:
- `OutboxRelay::new(db.pool.clone(), "worker-a", 10)` → `OutboxRelay::new(db.pool.clone(), "worker-a")` (and `"worker-b"`).
- `.claim_batch()` → `.claim_batch(10)`.
- `.relay_once(&AlwaysSucceeds)` → `.relay_once(&AlwaysSucceeds, 10)`; same for `.relay_once(&AlwaysRetryableFailure)` → `.relay_once(&AlwaysRetryableFailure, 10)`.

(Use the line refs from the plan header: new at 16,19,39,67,93; claim_batch at 17,21; relay_once at 41,69,95. Read the file and update every occurrence — the exact count matters.)

- [ ] **Step 4: Compile check**

Run: `cargo check --locked -p platform-core --all-targets`
Expected: PASS. (The worker app still calls the old signature and will break — but `-p platform-core` doesn't build it. Full workspace is fixed in Task 4.)

- [ ] **Step 5: Run the outbox tests with Postgres**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres cargo test --locked -p platform-core --test outbox_relay -- --nocapture`
Expected: PASS (the relay tests now pass batch as an argument).

- [ ] **Step 6: Format and commit**

```bash
cargo fmt -p platform-core
git add crates/platform-core/src/outbox.rs crates/platform-core/tests/outbox_relay.rs
git commit -m "refactor(platform-core): pass outbox relay batch size per call"
```

---

## Task 2: Thread `batch_size` through `RuntimeWorker`

Same transformation for the function-run worker.

**Files:**
- Modify: `crates/platform-runtime/src/functions.rs`
- Modify: `crates/platform-runtime/tests/function_runtime.rs`

- [ ] **Step 1: Update the `RuntimeWorker` struct and `new`**

In `crates/platform-runtime/src/functions.rs`, change the struct (line ~220) and `new` to drop `batch_size`:
```rust
pub struct RuntimeWorker {
    pool: DbPool,
    registry: Arc<FunctionRegistry>,
    worker_id: String,
}

impl RuntimeWorker {
    pub fn new(
        pool: DbPool,
        registry: Arc<FunctionRegistry>,
        worker_id: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            registry,
            worker_id: worker_id.into(),
        }
    }
```

- [ ] **Step 2: Add `batch_size` param to `claim_batch` and `claim_and_run_batch`**

Change `claim_batch`:
```rust
    pub async fn claim_batch(&self, batch_size: i64) -> AppResult<Vec<ClaimedFunctionRun>> {
```
Inside it, change `.bind(self.batch_size)` → `.bind(batch_size)`.

Change `claim_and_run_batch`:
```rust
    pub async fn claim_and_run_batch(&self, batch_size: i64) -> AppResult<usize> {
```
Inside it, change `let runs = self.claim_batch().await?;` → `let runs = self.claim_batch(batch_size).await?;`.

- [ ] **Step 3: Update the function_runtime tests**

In `crates/platform-runtime/tests/function_runtime.rs`, at the three test sites (lines ~92, 128, 160):
- `RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a", 10)` → `RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a")`
- `.claim_and_run_batch()` → `.claim_and_run_batch(10)`

Read the file and update all three occurrences of each.

- [ ] **Step 4: Compile check**

Run: `cargo check --locked -p platform-runtime --all-targets`
Expected: PASS.

- [ ] **Step 5: Run the function_runtime tests with Postgres**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres cargo test --locked -p platform-runtime --test function_runtime -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Format and commit**

```bash
cargo fmt -p platform-runtime
git add crates/platform-runtime/src/functions.rs crates/platform-runtime/tests/function_runtime.rs
git commit -m "refactor(platform-runtime): pass runtime worker batch size per call"
```

---

## Task 3: Add `WorkerRuntimeConfig` + descriptors, remove static `WorkerConfig`

Define the typed read struct and the two `Service("worker")` descriptors; delete the orphan static config.

**Files:**
- Create: `crates/platform-core/src/worker_runtime_config.rs`
- Modify: `crates/platform-core/src/lib.rs`
- Modify: `crates/platform-core/src/config.rs`

- [ ] **Step 1: Create `worker_runtime_config.rs`**

Create `crates/platform-core/src/worker_runtime_config.rs`:
```rust
//! Worker process runtime-config knobs, live-tunable via the Console.
//!
//! These are platform-runtime concerns (poll cadence, batch size), not a
//! business domain, so they are registered as platform-owned descriptors at the
//! composition root rather than via a `DomainDescriptor`.

use crate::runtime_config::{RuntimeConfigDescriptor, RuntimeConfigScope, RuntimeConfigType};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;

/// Worker config resolved from the snapshot under the `worker.` key prefix.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerRuntimeConfig {
    pub poll_interval_ms: u64,
    pub batch_size: i64,
}

impl Default for WorkerRuntimeConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 500,
            batch_size: 25,
        }
    }
}

/// Platform-owned, worker-scoped runtime config descriptors.
pub static RUNTIME_CONFIG: LazyLock<Vec<RuntimeConfigDescriptor>> = LazyLock::new(|| {
    vec![
        RuntimeConfigDescriptor {
            key: "worker.poll_interval_ms",
            scope: RuntimeConfigScope::Service("worker"),
            value_type: RuntimeConfigType::Int {
                min: Some(50),
                max: Some(60_000),
            },
            default: json!(500),
            editable: true,
            restart_only: false,
            description: "Milliseconds the worker sleeps between poll ticks.",
        },
        RuntimeConfigDescriptor {
            key: "worker.batch_size",
            scope: RuntimeConfigScope::Service("worker"),
            value_type: RuntimeConfigType::Int {
                min: Some(1),
                max: Some(1_000),
            },
            default: json!(25),
            editable: true,
            restart_only: false,
            description: "Maximum outbox events / function runs claimed per tick.",
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_config::{RuntimeConfigRegistry, RuntimeConfigSnapshot};
    use std::collections::BTreeMap;

    #[test]
    fn reads_defaults_from_snapshot() {
        let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).unwrap();
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "worker", &BTreeMap::new());
        let cfg: WorkerRuntimeConfig = snapshot.get("worker").unwrap();
        assert_eq!(cfg.poll_interval_ms, 500);
        assert_eq!(cfg.batch_size, 25);
    }

    #[test]
    fn default_impl_matches_descriptor_defaults() {
        let defaults = WorkerRuntimeConfig::default();
        let poll = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.poll_interval_ms")
            .unwrap();
        let batch = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.batch_size")
            .unwrap();
        assert_eq!(defaults.poll_interval_ms, poll.default.as_u64().unwrap());
        assert_eq!(defaults.batch_size, batch.default.as_i64().unwrap());
    }

    #[test]
    fn batch_size_out_of_range_rejected() {
        let batch = RUNTIME_CONFIG
            .iter()
            .find(|d| d.key == "worker.batch_size")
            .unwrap();
        assert!(batch.validate(&json!(25)).is_ok());
        assert!(batch.validate(&json!(0)).is_err());
        assert!(batch.validate(&json!(1001)).is_err());
    }
}
```

Note: confirm field-name correctness against `RuntimeConfigDescriptor` (Task-1 of the original config plan defined `key, scope, value_type, default, editable, restart_only, description` and `RuntimeConfigType::Int { min, max }`, `RuntimeConfigScope::Service(&'static str)`, plus a `validate(&Value)` method). These are unchanged by the rename.

- [ ] **Step 2: Wire the module + export, drop the WorkerConfig export, in lib.rs**

In `crates/platform-core/src/lib.rs`:
- Add `pub mod worker_runtime_config;` near the other `pub mod` lines.
- Add a re-export: `pub use worker_runtime_config::WorkerRuntimeConfig;` (the `RUNTIME_CONFIG` static is referenced via its full path `worker_runtime_config::RUNTIME_CONFIG` from app-bootstrap, so it does not need a root re-export, but the module must be `pub`).
- In the config re-export, REMOVE `WorkerConfig`. The line currently reads:
```rust
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LogFormat, ModuleConfig, ServiceConfig,
    TelemetryConfig, WorkerConfig, parse_cors_allowed_origins,
};
```
becomes:
```rust
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LogFormat, ModuleConfig, ServiceConfig,
    TelemetryConfig, parse_cors_allowed_origins,
};
```

- [ ] **Step 3: Remove the static `WorkerConfig` from config.rs**

In `crates/platform-core/src/config.rs`:
- Delete the `struct WorkerConfig` + its `impl Default` (lines ~147-158).
- In `AppConfig`, remove the field `pub worker: WorkerConfig,` (line ~10).
- In `AppConfig::from_env`, remove `worker: WorkerConfig::default(),` (line ~23).

- [ ] **Step 4: Verify nothing else references the static WorkerConfig**

Run: `grep -rn "\bWorkerConfig\b" crates apps domains --include="*.rs"`
Expected: NO output. (The new struct is `WorkerRuntimeConfig`, a different token.)

Run: `grep -rn "config\.worker\b\|\.worker:" crates apps domains --include="*.rs"`
Expected: NO output (the AppConfig.worker field is gone, nothing read it).

- [ ] **Step 5: Compile + unit tests**

Run: `cargo check --locked -p platform-core --all-targets`
Expected: PASS.

Run: `cargo test --locked -p platform-core worker_runtime_config::`
Expected: PASS (3 unit tests).

- [ ] **Step 6: Format and commit**

```bash
cargo fmt -p platform-core
git add crates/platform-core/src/worker_runtime_config.rs crates/platform-core/src/lib.rs crates/platform-core/src/config.rs
git commit -m "feat(platform-core): add worker runtime config items, remove static WorkerConfig"
```

---

## Task 4: Aggregate platform descriptors + wire the worker loop

Register the worker descriptors at the composition root and make the worker consume them live. After this task the full workspace compiles.

**Files:**
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `apps/worker/src/main.rs`

- [ ] **Step 1: Chain platform worker descriptors into `runtime_config_descriptors`**

In `crates/app-bootstrap/src/lib.rs`, the function currently is:
```rust
pub fn runtime_config_descriptors(ctx: &AppContext) -> Vec<RuntimeConfigDescriptor> {
    domains(ctx)
        .iter()
        .flat_map(|domain| domain.runtime_config.iter().cloned())
        .collect()
}
```
Change it to also include platform-owned worker descriptors:
```rust
pub fn runtime_config_descriptors(ctx: &AppContext) -> Vec<RuntimeConfigDescriptor> {
    let domain_descriptors = domains(ctx)
        .iter()
        .flat_map(|domain| domain.runtime_config.iter().cloned())
        .collect::<Vec<_>>();
    platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(domain_descriptors)
        .collect()
}
```
(`platform_core` is already a dependency of app-bootstrap; `RuntimeConfigDescriptor` is already imported there. If the import isn't present, add it to the existing `use platform_core::{...}` line.)

- [ ] **Step 2: Update the worker loop to read the snapshot per tick**

In `apps/worker/src/main.rs`, first update the imports to bring in `WorkerRuntimeConfig`:
- In the `use platform_core::{ ... }` line, add `WorkerRuntimeConfig`.

Then update the loop body. The current relevant code (around lines 39-71 in `run_worker_loop`) constructs the relay/worker with batch 25 and sleeps a fixed 500ms:
```rust
    let relay = OutboxRelay::new(ctx.db.clone(), "worker-local", 25);
    let runtime_worker = RuntimeWorker::new(ctx.db.clone(), registry, "worker-local", 25);
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => { ... }
            () = Shutdown::wait_for_signal() => { shutdown.signal(); }
            () = tokio::time::sleep(Duration::from_millis(500)) => {
                match relay.relay_once(&dispatcher).await { ... }
                match runtime_worker.claim_and_run_batch().await { ... }
            }
        }
    }
```
Change it to:
```rust
    let relay = OutboxRelay::new(ctx.db.clone(), "worker-local");
    let runtime_worker = RuntimeWorker::new(ctx.db.clone(), registry, "worker-local");
    loop {
        let cfg: WorkerRuntimeConfig = ctx.runtime_config.snapshot().get("worker").unwrap_or_default();
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            () = Shutdown::wait_for_signal() => {
                shutdown.signal();
            }
            () = tokio::time::sleep(Duration::from_millis(cfg.poll_interval_ms)) => {
                match relay.relay_once(&dispatcher, cfg.batch_size).await {
                    Ok(count) => {
                        tracing::debug!(claimed_outbox_events = count, "outbox relay tick");
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "outbox relay tick failed");
                    }
                }
                match runtime_worker.claim_and_run_batch(cfg.batch_size).await {
                    Ok(count) => {
                        tracing::debug!(claimed_function_runs = count, "runtime worker tick");
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "runtime worker tick failed");
                    }
                }
            }
        }
    }
```
Keep the existing shutdown arms exactly as they were (only the construction, the sleep duration, and the two `.await` calls change). Read the actual file first and preserve its exact match-arm bodies.

NOTE on `cfg.poll_interval_ms`: `Duration::from_millis` takes a `u64`; `WorkerRuntimeConfig.poll_interval_ms` is `u64` — direct. `cfg.batch_size` is `i64`, matching the new `relay_once`/`claim_and_run_batch` signatures.

- [ ] **Step 3: Full workspace compile check**

Run: `cargo check --locked --workspace --all-targets`
Expected: PASS. The worker now constructs the relay/worker without a batch arg and passes batch per call.

- [ ] **Step 4: Run the full Rust test suite with Postgres**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres cargo test --locked --workspace`
Expected: PASS — including the updated outbox_relay, function_runtime, and worker_runtime_config tests.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add crates/app-bootstrap/src/lib.rs apps/worker/src/main.rs
git commit -m "feat(worker): drive poll interval and batch size from runtime config"
```

---

## Task 5: Integration test — write → refresh → typed read

Prove the worker config round-trips through Postgres like the existing provider test.

**Files:**
- Create: `crates/platform-core/tests/worker_runtime_config.rs`

- [ ] **Step 1: Write the integration test**

Create `crates/platform-core/tests/worker_runtime_config.rs`:
```rust
use platform_core::runtime_config::store::upsert_value;
use platform_core::{
    PLATFORM_MIGRATIONS, PostgresRuntimeConfigProvider, RuntimeConfigProvider, RuntimeConfigRegistry,
    WorkerRuntimeConfig, apply_migrations,
};
use platform_core::worker_runtime_config::RUNTIME_CONFIG;
use platform_testing::TestDatabase;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn worker_config_round_trips_through_postgres() {
    let Some(test_db) = TestDatabase::create().await else {
        return; // DATABASE_URL not set; skip.
    };
    apply_migrations(&test_db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).expect("registry");
    let provider =
        PostgresRuntimeConfigProvider::connect(test_db.pool.clone(), Arc::new(registry), "worker")
            .await
            .expect("connect provider");

    // Defaults before any write.
    let cfg: WorkerRuntimeConfig = provider.snapshot().get("worker").expect("worker config");
    assert_eq!(cfg.poll_interval_ms, 500);
    assert_eq!(cfg.batch_size, 25);

    // Write a worker-scoped override and refresh.
    upsert_value(&test_db.pool, "worker", "worker.batch_size", &json!(100), Some("test"))
        .await
        .expect("upsert");
    provider.refresh().await.expect("refresh");

    let cfg: WorkerRuntimeConfig = provider.snapshot().get("worker").expect("worker config");
    assert_eq!(cfg.batch_size, 100);
    assert_eq!(cfg.poll_interval_ms, 500); // untouched key keeps default

    test_db.cleanup().await;
}
```

Note: this uses `RuntimeConfigProvider` (the trait, for `.snapshot()`), `PostgresRuntimeConfigProvider::connect`, `worker_runtime_config::RUNTIME_CONFIG`, and `runtime_config::store::upsert_value` — all confirmed to exist and be public. The service key is `"worker"` and the override is written under service `"worker"` (matching `Service("worker")` scope, which stores under the literal `"worker"`, not `*`).

- [ ] **Step 2: Run it against Postgres**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres cargo test --locked -p platform-core --test worker_runtime_config -- --nocapture`
Expected: PASS (1 test). If `DATABASE_URL` unset it cleanly skips.

- [ ] **Step 3: Commit**

```bash
git add crates/platform-core/tests/worker_runtime_config.rs
git commit -m "test(platform-core): cover worker runtime config round trip"
```

---

## Final Verification

- [ ] **Run the full quality gate**

Run: `DATABASE_URL=postgres://postgres@localhost:5432/postgres just check`
Expected: PASS — fmt, rust-check, all tests (incl. Postgres integration), generated-check, arch-check, sdk-check, console-check.

- [ ] **Confirm the outward contract did not change**

Run: `just generated-check`
Expected: PASS with NO diff. This batch adds no HTTP DTOs or routes — the two items surface dynamically via the existing `/admin/config` descriptors/values endpoints. (The OpenAPI doc enumerates handlers, not config descriptors, so it is unchanged.)

- [ ] **Confirm the new items are registered (manual sanity)**

Run: `grep -rn "worker.poll_interval_ms\|worker.batch_size" crates/platform-core/src/worker_runtime_config.rs`
Expected: both keys present. The composition root chains them in via `runtime_config_descriptors`, so a running worker/api will list them under the `worker` service at `/admin/config/descriptors`.
