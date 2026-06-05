# Module Framework - Host-Owned Lifecycle Surface

**Date:** 2026-06-05
**Status:** Approved design, ready for implementation planning
**Scope:** Add a bounded lifecycle capability for modules without introducing arbitrary startup hooks.

---

## Context

Lenso's module framework is built around a data/behavior split:

- `ModuleManifest` is serializable declaration data.
- `ModuleBinding` is a narrow source-specific behavior seam.
- `LinkedBinding` registers in-process Rust functions and event handlers.
- `RemoteBinding` registers proxy-backed runtime functions declared by remote manifests.
- The host owns durable runtime queues, retries, Runtime Story semantics, execution logs, and operator visibility.

That boundary is important. A module may need stronger startup capability, but a generic `on_startup(AppContext)` hook would let modules run hidden side effects outside the runtime, outside retries, and outside operator workflows.

This design gives modules lifecycle power through host-owned declarations and runtime work, not through free-form startup code.

## Goals

- Let a module declare lifecycle needs in `ModuleManifest`.
- Support idempotent startup activation work such as cache warmup, remote sync bootstrap, or backfill scheduling.
- Keep host-owned orchestration for enqueueing, retries, correlation IDs, execution logs, and Runtime Console visibility.
- Preserve the Linked/Remote source split: both sources expose the same lifecycle data contract.
- Keep module lifecycle failures understandable to operators.

## Non-Goals

- No generic `ModuleBinding::init(ctx)` or `on_startup(AppContext)` hook.
- No module-owned direct access to `runtime.function_runs` or `platform.outbox`.
- No remote event handler lifecycle.
- No install/upgrade marketplace lifecycle, signatures, provenance, or compatibility policy.
- No non-idempotent "run once forever" migration system in the first slice.
- No database migrations through lifecycle. Migrations stay in the migration runner.

## Approaches Considered

| Approach | Shape | Decision |
|----------|-------|----------|
| Generic startup hook | Add `init(ctx)` to `ModuleBinding` and call it during app startup. | Rejected. It hides side effects and gives linked modules more power than remote modules. |
| Linked-only lifecycle trait | Add a new trait only linked modules can implement. | Rejected for the first slice. It solves local Rust code but does not advance the installable module direction. |
| Manifest lifecycle surface + host executor | Add lifecycle declarations to `ModuleManifest`; the host validates and executes them through runtime-owned mechanisms. | Recommended. It keeps behavior observable, durable, and source-neutral. |

## Key Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Lifecycle home | `ModuleManifest.lifecycle: Option<LifecycleSurface>` | Lifecycle is declared capability data and must be visible before execution. |
| Startup behavior | Activation work is a runtime function enqueue, not direct code execution | Runtime functions already have queues, retries, logs, and Story visibility. |
| First run policy | Only idempotent `every_startup` activation jobs | Avoids adding lifecycle state tables before there is a concrete install/upgrade need. |
| Enqueuing owner | Worker startup only | Prevents API processes from enqueueing activation work and keeps background work with the worker app. |
| Failure handling | Enqueue failures are startup failures; function failures follow runtime retry policy | The host must successfully schedule required work, but execution outcome is owned by the runtime. |
| Checks | Host-verifiable startup checks only | Avoids arbitrary module code. Remote load failures already surface through module load status. |
| Visibility | Activation jobs appear as normal function runs; check status can be exposed through module metadata later | Reuses existing Runtime Console surfaces first. |

## Manifest Shape

Add lifecycle declarations to `platform-module`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct LifecycleSurface {
    #[serde(default)]
    pub startup_checks: Vec<LifecycleStartupCheckDeclaration>,
    #[serde(default)]
    pub activation_jobs: Vec<LifecycleActivationJobDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct LifecycleStartupCheckDeclaration {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(flatten)]
    pub check: LifecycleStartupCheckKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleStartupCheckKind {
    FunctionRegistered { function_name: String },
    CapabilityDeclared { capability: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct LifecycleActivationJobDeclaration {
    pub name: String,
    pub function_name: String,
    #[serde(default = "default_every_startup")]
    pub run_policy: LifecycleActivationRunPolicy,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleActivationRunPolicy {
    EveryStartup,
}

fn default_every_startup() -> LifecycleActivationRunPolicy {
    LifecycleActivationRunPolicy::EveryStartup
}
```

The first enum values are deliberately host-verifiable. If a later slice needs richer checks, add specific variants with clear host behavior rather than introducing a free-form callback.

Example manifest fragment:

```json
{
  "name": "remote-crm",
  "runtime": {
    "functions": [
      {
        "name": "remote_crm.warm_contact_cache.v1",
        "version": 1,
        "queue": "remote-crm",
        "input_schema": "remote_crm.warm_contact_cache.v1"
      }
    ]
  },
  "lifecycle": {
    "startup_checks": [
      {
        "name": "warm cache function is registered",
        "required": true,
        "kind": "function_registered",
        "function_name": "remote_crm.warm_contact_cache.v1"
      }
    ],
    "activation_jobs": [
      {
        "name": "warm contact cache",
        "function_name": "remote_crm.warm_contact_cache.v1",
        "run_policy": "every_startup",
        "input": { "reason": "worker_startup" },
        "required": true
      }
    ]
  }
}
```

## Host Execution Flow

Worker startup should become:

1. Load linked and configured remote modules through `app-bootstrap::load_modules(ctx)`.
2. Build the `FunctionRegistry` from loaded modules.
3. Validate lifecycle declarations:
   - Activation job function names must be declared in the module runtime surface.
   - Activation job functions must be registered in the `FunctionRegistry`.
   - Required startup checks must pass.
4. Enqueue activation jobs through the existing runtime enqueue path with:
   - service actor `worker`;
   - a lifecycle correlation ID;
   - causation ID that names the module and activation job;
   - input from the manifest, wrapped with `_lenso_runtime` as usual by the runtime enqueue path.
5. Start the normal outbox relay and runtime worker loop.

API startup may load modules for metadata and route/proxy setup, but it must not enqueue activation jobs.

## Error Handling

- Invalid lifecycle declarations should produce module manifest lints.
- A required startup check failure blocks worker startup with a clear module/job message.
- A non-required startup check failure is logged and should be included in lifecycle status metadata when that endpoint is added.
- A required activation enqueue failure blocks worker startup.
- Activation function execution failures do not crash the process. They use existing runtime retry and dead-letter behavior.
- Duplicate startup activation is allowed only for idempotent jobs. Non-idempotent install or upgrade work requires a later persisted lifecycle-state slice.

## Boundaries

Lifecycle does not move business logic into the platform. The module owns the runtime function implementation. The platform owns when and how that function is scheduled, observed, retried, and surfaced.

Lifecycle also does not replace config or migrations:

- Configuration remains declared through runtime config descriptors and app config.
- Database schema changes remain deterministic migrations.
- Remote module availability remains handled by remote module loading and remote source configuration.

## Testing

The implementation plan should include:

- `platform-module` tests for lifecycle serde, builder support, and manifest lints.
- `app-bootstrap` tests that lifecycle declarations validate against module runtime declarations.
- Worker/runtime tests proving startup activation enqueues a function run with the expected correlation and causation context.
- Remote module fixture tests showing a remote manifest can declare an activation job for a remote runtime function.
- A negative test proving a missing activation function fails validation before worker loops start.

## Deferred Slices

- Persisted lifecycle state for `once_per_module_version` and install/upgrade hooks.
- Manual operator-triggered lifecycle actions in the Runtime Console.
- Rich lifecycle status endpoints under module metadata.
- Remote health protocol beyond manifest loading and runtime invocation.
- Event handler lifecycle for remote modules.
- Marketplace trust, signatures, compatibility checks, and sandbox policy.
