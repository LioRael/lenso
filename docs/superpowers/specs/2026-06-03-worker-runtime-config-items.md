# First Real Runtime Config Items: Worker Knobs

Date: 2026-06-03
Status: Approved (design); pending implementation plan
Builds on: `2026-06-02-config-system-design.md`, `2026-06-03-rename-settings-to-runtime-config.md`

## Goal

Introduce the first batch of **genuinely consumed** runtime config items: two
worker operational knobs that operators can tune live from the Console without a
redeploy. Also remove the orphan static `WorkerConfig` whose responsibility these
dynamic knobs now take over.

This proves the runtime-config system end to end (register → store → NOTIFY →
snapshot → consume) on real running code, unlike the identity TTL worked example
which is registered but not yet consumed.

## Why not CORS

`CORS_ALLOWED_ORIGINS` was considered and **deliberately kept static** (in
`AppConfig.http`):

- It is a security boundary, not an operational knob. Making it console-editable
  would let any admin actor authorize new credentialed cross-origin callers — a
  privilege-escalation surface better pinned at deploy time (env/IaC, reviewed
  and version-controlled).
- It is structurally a startup-time `tower` middleware baked into the router at
  `build_router`; hot-swapping it is real complexity for a value that changes
  rarely.
- The dynamic system's sweet spot is behavioral knobs tuned while observing the
  running system (poll intervals, batch sizes, flags), not security config.

If an origin ever needs adding without redeploy, the right tool is a config
reload or a restart-only entry — not live editing.

## Config items

Scope: `Service("worker")` — these only apply to the worker process; the API
snapshot does not surface them (resolution filters by service).

| key | type | default | range | editable | restart_only |
| --- | --- | --- | --- | --- | --- |
| `worker.poll_interval_ms` | Int | 500 | 50–60000 | true | false (hot) |
| `worker.batch_size` | Int | 25 | 1–1000 | true | false (hot) |

Defaults match the **current hardcoded values** (500ms / 25), so introducing the
registry changes no behavior on first deploy. (Note: the orphan static
`WorkerConfig.worker_poll_interval_ms` defaulted to 1000 and was never read; the
dynamic default is the real running value, 500.)

Both knobs take effect live:
- `poll_interval_ms`: the worker loop reads the current value from
  `ctx.runtime_config.snapshot()` each tick to decide the sleep duration.
- `batch_size`: moved out of the `OutboxRelay`/`RuntimeWorker` constructors into
  per-call arguments, so each tick passes the current snapshot value.

## Worker consumption changes

### Loop (`apps/worker/src/main.rs`)

Before:
```rust
let relay = OutboxRelay::new(ctx.db.clone(), "worker-local", 25);
let runtime_worker = RuntimeWorker::new(ctx.db.clone(), registry, "worker-local", 25);
loop { tokio::select! {
    () = tokio::time::sleep(Duration::from_millis(500)) => {
        relay.relay_once(&dispatcher).await ...
        runtime_worker.claim_and_run_batch().await ...
    }
}}
```

After:
```rust
let relay = OutboxRelay::new(ctx.db.clone(), "worker-local");
let runtime_worker = RuntimeWorker::new(ctx.db.clone(), registry, "worker-local");
loop {
    let cfg: WorkerRuntimeConfig =
        ctx.runtime_config.snapshot().get("worker").unwrap_or_default();
    tokio::select! {
        () = tokio::time::sleep(Duration::from_millis(cfg.poll_interval_ms)) => {
            relay.relay_once(&dispatcher, cfg.batch_size).await ...
            runtime_worker.claim_and_run_batch(cfg.batch_size).await ...
        }
        // shutdown arms unchanged
    }
}
```

Snapshot reads are lock-free (`ArcSwap`); one read per tick is negligible. A
failed `get("worker")` parse falls back via `unwrap_or_default()` to 500/25 — the
worker never stalls on bad config.

### Constructor/method signatures

- `platform-core::outbox::OutboxRelay`: drop `batch_size` from `new`; change
  `relay_once(&dispatcher)` → `relay_once(&dispatcher, batch_size: i64)`; internal
  uses of `self.batch_size` become the parameter. Remove the now-unused
  `batch_size` field.
- `platform-runtime::RuntimeWorker`: drop `batch_size` from `new`; change
  `claim_and_run_batch()` → `claim_and_run_batch(batch_size: i64)`; same internal
  swap.

## WorkerRuntimeConfig type + descriptors

`WorkerRuntimeConfig` is a plain deserializable struct whose fields map to the
`worker.` key prefix:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerRuntimeConfig {
    pub poll_interval_ms: u64,
    pub batch_size: i64,
}
impl Default for WorkerRuntimeConfig {
    fn default() -> Self { Self { poll_interval_ms: 500, batch_size: 25 } }
}
```
Plus a `RUNTIME_CONFIG: LazyLock<Vec<RuntimeConfigDescriptor>>` with the two
`Service("worker")` descriptors (Int types, the ranges above).

Both live in `platform-core` (a new small module, e.g.
`platform-core::worker_runtime_config`, alongside `config.rs`), since worker knobs
are a platform-runtime concern, not a business domain.

`snapshot.get::<T>("worker")` builds the struct from keys under the `worker.`
prefix: `worker.poll_interval_ms` → field `poll_interval_ms`, `worker.batch_size`
→ field `batch_size`.

## Registration: platform descriptors at the composition root

`app_bootstrap::runtime_config_descriptors()` currently aggregates only each
domain's descriptors. Extend it to also chain in platform-owned worker
descriptors (`platform_core::worker_runtime_config::RUNTIME_CONFIG`). This gives
platform knobs a clear home without forcing them into a business domain.

## Remove the orphan static WorkerConfig

The dynamic `worker.poll_interval_ms` now owns this responsibility. Remove:
- `struct WorkerConfig` + `impl Default for WorkerConfig` in `config.rs`
- `AppConfig.worker: WorkerConfig` field
- `worker: WorkerConfig::default()` in `AppConfig::from_env`
- the `WorkerConfig` re-export in `lib.rs`

No env var ever drove it; removal is zero-risk. Confirm via grep that nothing
reads `AppConfig.worker` (already verified: no readers).

## Testing

- **Unit**: `WorkerRuntimeConfig` parses from a snapshot via `get("worker")` and
  falls back to the 500/25 default when no rows exist; descriptor validation
  (batch_size out of range rejected, e.g. 0 and 1001).
- **Integration** (reuse the Postgres harness): write `worker.batch_size` via the
  store, refresh a `PostgresRuntimeConfigProvider` for `"worker"`, and assert
  `get::<WorkerRuntimeConfig>("worker")` reflects the new value.
- **Signature changes**: update any existing tests/callers of
  `OutboxRelay::relay_once` / `RuntimeWorker::claim_and_run_batch` / their `new`
  to the new signatures.
- **Regression**: full `just check` (with Postgres) green; `just generated-check`
  shows no diff (this batch does not touch the outward HTTP contract — it is
  worker-internal consumption plus registration).

## SDK / Console

No new screen. The two items appear automatically on the existing `/admin/config`
screen (the descriptors/values endpoints enumerate dynamically), grouped under the
`worker` service. No SDK regeneration is needed (no new DTOs).

## Components & Boundaries

| Unit | Responsibility | Depends on |
| --- | --- | --- |
| `WorkerRuntimeConfig` + descriptors | Declare worker knobs, typed read shape | `platform-core::runtime_config` |
| `OutboxRelay` (modified) | Relay outbox in caller-supplied batch | `platform-core` db |
| `RuntimeWorker` (modified) | Run function batch in caller-supplied batch | `platform-runtime` |
| worker loop (modified) | Read snapshot per tick; drive interval+batch | `AppContext.runtime_config` |
| `app_bootstrap::runtime_config_descriptors` (extended) | Aggregate domain + platform descriptors | `platform-core` worker descriptors |

## Out of scope

- CORS dynamicization (kept static, see above).
- Consuming the identity `password_reset_ttl_minutes` (no password-reset feature
  exists yet; a separate effort).
- Any new console UI.
