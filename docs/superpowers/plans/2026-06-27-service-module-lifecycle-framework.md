# Service Module Lifecycle Framework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Lenso service modules from a renamed remote-module path into an operator-visible lifecycle framework: create, configure, inspect, run, diagnose, view in Console, and extract from linked modules.

**Architecture:** Keep Host as the control plane and keep `Remote` as the underlying source type. Add a shared lifecycle vocabulary over the existing files and runtime metadata: `.env` / `REMOTE_MODULES`, `.lenso/module-installs.json`, `.lenso/module-services.json`, source diagnostics, module registry, Remote Calls, Runtime Story, and console package state. Do not add service discovery, API gateway, service mesh, distributed transactions, schema registry, deployment orchestration, or automatic linked-to-service migration.

**Tech Stack:** Rust 2024, clap, serde/serde_json, reqwest, Axum, utoipa, TypeScript, React, Vitest, Node scripts, existing `@lenso/remote-module-kit`.

**Estimated Work:** About 10 hours of implementation. Verification is deliberately concentrated: focused unit/model tests while developing, one final example smoke at the end.

---

## Scope Shift From The Smaller Plan

This replaces the smaller "operator loop" plan. The larger v3 includes:

- CLI JSON doctor output.
- CLI service lifecycle commands for host-local service declarations.
- Host API service-module doctor endpoint.
- Console service-module cockpit on the existing Modules surface.
- Better service-module scaffold output with a proper ready endpoint and lifecycle install hints.
- A linked-to-service extraction checklist visible from docs and skills.
- One final support-ticket integration smoke, not repeated smoke checks after every task.

## Verification Policy

Use cheap checks while building:

- Rust unit/API tests for changed backend/CLI logic.
- Vitest model tests for Console state mapping.
- `git diff --check` per repo before final.

Only run host-level smoke once near the end:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm host-api-smoke:support-ticket
```

Do not add extra smoke scripts unless a feature cannot be verified any other way.

## File Map

### `lenso-cli`

- Modify `src/main.rs`: add `module service list|status|start|stop`, and add `module doctor --json`.
- Modify `src/module.rs`: extract service-module doctor report, add local service process inspection helpers, add service lifecycle commands that operate on `.lenso/module-services.json`.
- Modify `README.md`: document service lifecycle commands and JSON doctor output.
- Modify `templates/starter-host/README.md`: show the new operator flow.

### `lenso`

- Modify `crates/platform-admin-data/src/dto.rs`: add service-module lifecycle DTOs.
- Modify `crates/platform-admin-data/src/handlers.rs`: expose read-only service-module lifecycle state under `/admin/data/service-modules`.
- Modify `crates/lenso-api/tests/admin_data_console.rs`: cover loaded/configured/restart-pending/stale-state service-module API states.
- Modify `docs/architecture/service-module-boundary.md`: describe lifecycle framework boundary.
- Create `docs/architecture/service-module-operator-runbook.md`: operator state table and fixes.
- Modify `docs/architecture/linked-to-service-module.md`: add extraction checklist.
- Modify `skills/lenso-remote-module-authoring/SKILL.md`: teach agents to use the lifecycle flow.
- Modify `skills/lenso-start/SKILL.md`: route service-module work through lifecycle commands.

### `lenso-runtime-console`

- Modify `src/data/available-modules.ts`: add client types/fetcher for service-module lifecycle API.
- Modify `src/pages/available-modules-model.ts`: merge lifecycle status into existing doctor checks and handoff state.
- Modify `src/pages/modules-page.tsx`: add a compact "Service Module" cockpit on the Modules detail view.
- Modify `src/pages/available-modules-model.test.ts`: cover status mapping.
- Modify `src/pages/data-render-model.test.ts`: cover readiness summary wording if changed.

### `lenso-examples`

- Modify `examples/support-ticket/README.md`: document lifecycle commands.
- Modify `docs/support-ticket-service-module-run.md`: use CLI/API/Console lifecycle state.
- Modify `scripts/support-ticket-host-api-smoke.ts`: add exactly one host API lifecycle assertion.

## Task 1: CLI Doctor JSON And Shared Report Model

**Budget:** 1.25 hours.

**Files:**
- Modify: `lenso-cli/src/main.rs`
- Modify: `lenso-cli/src/module.rs`
- Test: `lenso-cli/src/module.rs`

- [ ] **Step 1: Add `--json` to doctor args**

In `ModuleDoctorArgs`:

```rust
/// Print machine-readable JSON.
#[arg(long)]
json: bool,
```

In the `From<&ModuleDoctorArgs> for module::ModuleDoctorOptions` mapping:

```rust
json: args.json,
```

In `module::ModuleDoctorOptions`:

```rust
pub json: bool,
```

- [ ] **Step 2: Add report structs**

Place near `RemoteModuleServiceDoctorStatus`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleDoctorReport {
    issue_count: usize,
    sources_checked: usize,
    services_checked: usize,
    sources: Vec<ModuleDoctorSourceReport>,
    services: Vec<ModuleDoctorServiceReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleDoctorSourceReport {
    module_name: String,
    installed: bool,
    configured: bool,
    enabled: bool,
    base_url: Option<String>,
    manifest_url: Option<String>,
    manifest_status: ModuleDoctorManifestStatus,
    fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ModuleDoctorManifestStatus {
    Reachable,
    Unreachable,
    Skipped,
    NotConfigured,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleDoctorServiceReport {
    module_name: String,
    service_name: String,
    status: String,
    ready_url: String,
    process: String,
    command: Option<String>,
    lock_file: Option<String>,
    pid_file: Option<String>,
    fix: Option<String>,
}
```

- [ ] **Step 3: Refactor `doctor_module` into build + render**

Create:

```rust
async fn build_module_doctor_report(
    repo_root: &Path,
    env_file_path: &Path,
    module_services_path: &Path,
    requested_module: Option<&str>,
) -> Result<ModuleDoctorReport>
```

Move existing source/manifest/readyUrl/stale-file logic into this function.

Create:

```rust
fn print_module_doctor_report(repo_root: &Path, report: &ModuleDoctorReport)
```

It should render the current human output.

- [ ] **Step 4: Print JSON when requested**

In `doctor_module`:

```rust
if options.json {
    println!("{}", serde_json::to_string_pretty(&report)?);
} else {
    print_module_doctor_report(&repo_root, &report);
}
```

Keep the same non-zero behavior:

```rust
if report.issue_count > 0 {
    bail!("Module doctor found {} service module issue(s)", report.issue_count);
}
```

- [ ] **Step 5: Add tests**

Add:

```rust
#[test]
fn doctor_manifest_status_serializes_snake_case() {
    assert_eq!(
        serde_json::to_value(ModuleDoctorManifestStatus::Unreachable).unwrap(),
        json!("unreachable")
    );
}

#[test]
fn doctor_service_manual_not_ready_is_an_issue() {
    assert!(
        remote_module_service_doctor_status(true, true, false, false, false, false).is_issue()
    );
}
```

- [ ] **Step 6: Verify cheap**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked --bin lenso doctor
```

Expected: doctor-related tests pass.

## Task 2: CLI Service Lifecycle Commands

**Budget:** 2 hours.

**Files:**
- Modify: `lenso-cli/src/main.rs`
- Modify: `lenso-cli/src/module.rs`
- Modify: `lenso-cli/README.md`
- Test: `lenso-cli/src/module.rs`

**Intent:** Operators should not need to manually inspect `.lenso/module-services.json`, `.lock`, and `.pid` files. The CLI should expose the service declarations and local state directly.

- [ ] **Step 1: Add subcommands**

In `ModuleCommand`:

```rust
/// Inspect and manage declared service-module processes.
Service {
    #[command(subcommand)]
    command: ModuleServiceCommand,
},
```

Add:

```rust
#[derive(Debug, Subcommand)]
enum ModuleServiceCommand {
    /// List declared service-module services.
    List(ModuleServiceListArgs),
    /// Show one service-module service with local state.
    Status(ModuleServiceStatusArgs),
    /// Start a declared service-module service in the background.
    Start(ModuleServiceStartArgs),
    /// Stop a declared service-module service started by the CLI or host.
    Stop(ModuleServiceStopArgs),
}
```

Each args struct should include `repo_root`, `module_services_file`, and optional module/service names as needed.

- [ ] **Step 2: Reuse existing service file parser**

Use `read_remote_module_service_states()` and `remote_module_service_state_path()`. Do not create another parser.

- [ ] **Step 3: Implement `module service list`**

Output columns:

```text
MODULE          SERVICE       PROCESS       READY URL
support-ticket  api           host-started  http://127.0.0.1:4110/lenso/module/v1/manifest
```

Support:

```sh
lenso module service list --json
```

JSON shape:

```json
{
  "services": [
    {
      "moduleName": "support-ticket",
      "serviceName": "api",
      "autoStart": true,
      "command": "pnpm start:support-ticket",
      "readyUrl": "http://127.0.0.1:4110/lenso/module/v1/manifest"
    }
  ]
}
```

- [ ] **Step 4: Implement `module service status`**

Status checks should reuse the same readyUrl and lock/pid logic as doctor. Output:

```text
support-ticket/api: ready
readyUrl: http://127.0.0.1:4110/lenso/module/v1/manifest
state: lock=.lenso/remote-support-ticket-api.lock pid=.lenso/remote-support-ticket-api.pid
```

JSON shape:

```json
{
  "moduleName": "support-ticket",
  "serviceName": "api",
  "status": "ready",
  "ready": true,
  "lockFile": null,
  "pidFile": null
}
```

- [ ] **Step 5: Implement `module service start` minimally**

Start only one declared service:

```sh
lenso module service start support-ticket api
```

Rules:

- If `readyUrl` already returns success, print `already ready` and exit 0.
- Spawn the command with the declared `cwd` or repo root.
- Write `.lock` and `.pid` using existing sanitized paths.
- Do not daemonize through a new supervisor. Use `std::process::Command` with inherited stdio; document that this is local development process management.

Add a `// ponytail:` comment:

```rust
// ponytail: local dev process control; a real supervisor belongs in deployment tooling.
```

- [ ] **Step 6: Implement `module service stop` minimally**

Read pid file, send SIGTERM on Unix by invoking:

```rust
Command::new("kill").arg(pid).status()
```

Then remove the pid/lock files if the command succeeds. If no pid file exists, print `not running`.

Do not add cross-platform process abstractions in this pass.

- [ ] **Step 7: Add unit tests**

Test:

- service selection by module/service name;
- state path sanitization already exists, keep it;
- JSON list output serializes camelCase.

- [ ] **Step 8: Verify cheap**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked --bin lenso module_service
```

Expected: service command tests pass.

## Task 3: Host Service Module Lifecycle API

**Budget:** 2 hours.

**Files:**
- Modify: `lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `lenso/crates/lenso-api/tests/admin_data_console.rs`

**Intent:** Runtime Console should not guess lifecycle state from scattered fields. The host should expose a read-only service-module lifecycle snapshot.

- [ ] **Step 1: Add DTOs**

In `dto.rs`:

```rust
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleLifecycleResponse {
    pub version: u8,
    pub status: AdminServiceModuleLifecycleStatus,
    pub modules: Vec<AdminServiceModuleLifecycleModuleDto>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleLifecycleStatus {
    Ready,
    NeedsAttention,
    Empty,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleLifecycleModuleDto {
    pub module_name: String,
    pub status: AdminServiceModuleLifecycleModuleStatus,
    pub installed: bool,
    pub configured: bool,
    pub loaded: bool,
    pub restart_pending: bool,
    pub base_url: Option<String>,
    pub manifest_url: Option<String>,
    pub manifest_status: AdminServiceModuleManifestStatus,
    pub services: Vec<AdminServiceModuleLifecycleServiceDto>,
    pub fixes: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleLifecycleModuleStatus {
    Ready,
    RestartPending,
    ConfiguredNotLoaded,
    ManifestUnreachable,
    ServiceNotReady,
    StaleState,
    NotConfigured,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceModuleManifestStatus {
    Reachable,
    Unreachable,
    Skipped,
    NotConfigured,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceModuleLifecycleServiceDto {
    pub name: String,
    pub ready_url: String,
    pub ready: bool,
    pub auto_start: bool,
    pub lock_file: Option<String>,
    pub pid_file: Option<String>,
}
```

- [ ] **Step 2: Parse service files once**

In `handlers.rs`, add local deserialization structs for `.lenso/module-services.json`. Keep them private to this file.

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleServicesFile {
    #[serde(default)]
    modules: Vec<LocalModuleServiceState>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleServiceState {
    module_name: String,
    #[serde(default)]
    services: Vec<LocalModuleServiceSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalModuleServiceSpec {
    name: String,
    command: String,
    ready_url: String,
    #[serde(default)]
    auto_start: bool,
}
```

- [ ] **Step 3: Build lifecycle modules**

Create:

```rust
async fn service_module_lifecycle_response(
    metadata: Vec<AdminModuleMetadata>,
    install_state: AvailableModuleInstallStateContext,
) -> AdminServiceModuleLifecycleResponse
```

Status priority:

1. stale lock/pid -> `StaleState`;
2. restart pending -> `RestartPending`;
3. configured but not loaded -> `ConfiguredNotLoaded`;
4. HTTP manifest unreachable -> `ManifestUnreachable`;
5. readyUrl failed -> `ServiceNotReady`;
6. loaded/configured without failures -> `Ready`;
7. no config -> `NotConfigured`.

For gRPC/non-HTTP base URLs, set `manifest_status = Skipped`.

- [ ] **Step 4: Add endpoint**

Expose:

```text
GET /admin/data/service-modules
```

Do not add start/stop HTTP actions. This endpoint is read-only.

- [ ] **Step 5: Add tests**

Add tests in `admin_data_console.rs`:

- `service_modules_returns_empty_when_none_configured`;
- `service_modules_marks_restart_pending_from_env_source`;
- `service_modules_marks_stale_state_from_lock_file`;
- `service_modules_marks_loaded_remote_ready`.

- [ ] **Step 6: Verify focused**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
HTTP_HOST=127.0.0.1 cargo test --locked -p lenso-api --test admin_data_console service_modules
```

Expected: selected tests pass.

## Task 4: Runtime Console Service Module Cockpit

**Budget:** 2 hours.

**Files:**
- Modify: `lenso-runtime-console/src/data/available-modules.ts`
- Modify: `lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `lenso-runtime-console/src/pages/modules-page.tsx`
- Test: `lenso-runtime-console/src/pages/available-modules-model.test.ts`

**Intent:** A user opening Console should immediately see service-module state without reading CLI output.

- [ ] **Step 1: Add data types**

Add:

```ts
export type ServiceModuleLifecycleModuleStatus =
  | "ready"
  | "restart_pending"
  | "configured_not_loaded"
  | "manifest_unreachable"
  | "service_not_ready"
  | "stale_state"
  | "not_configured";

export type ServiceModuleLifecycleModule = {
  moduleName: string;
  status: ServiceModuleLifecycleModuleStatus;
  installed: boolean;
  configured: boolean;
  loaded: boolean;
  restartPending: boolean;
  baseUrl: string | null;
  manifestUrl: string | null;
  manifestStatus: "reachable" | "unreachable" | "skipped" | "not_configured";
  fixes: string[];
};

export type ServiceModuleLifecycleResponse = {
  version: number;
  status: "ready" | "needs_attention" | "empty";
  modules: ServiceModuleLifecycleModule[];
};
```

- [ ] **Step 2: Add fetcher**

Add:

```ts
export async function fetchServiceModuleLifecycle(httpClient: HttpClient) {
  return httpClient
    .get("admin/data/service-modules")
    .json<ServiceModuleLifecycleResponse>();
}
```

- [ ] **Step 3: Map lifecycle status to doctor chips**

Add helper in `available-modules-model.ts`:

```ts
export function serviceModuleLifecycleDoctorCheck(
  moduleName: string,
  lifecycle: ServiceModuleLifecycleResponse | null
): AvailableModuleDoctorCheck | null {
  const module = lifecycle?.modules.find((item) => item.moduleName === moduleName);
  if (!module) return null;
  if (module.status === "ready") {
    return doctorCheck("doctor", "service", "ok", "service module is ready");
  }
  return doctorCheck(
    "doctor",
    "service",
    "fix",
    module.fixes[0] ?? `service module status: ${module.status}`,
    "lenso module doctor"
  );
}
```

- [ ] **Step 4: Render cockpit in `modules-page.tsx`**

Use the existing module detail Operations panel. Add rows:

```tsx
{serviceLifecycleModule ? (
  <MetadataRows
    rows={[
      { label: "service status", value: serviceLifecycleModule.status },
      { label: "configured", value: String(serviceLifecycleModule.configured) },
      { label: "loaded", value: String(serviceLifecycleModule.loaded) },
      { label: "manifest", value: serviceLifecycleModule.manifestStatus },
      { label: "fix", value: serviceLifecycleModule.fixes[0] ?? "-" },
    ]}
  />
) : null}
```

Do not add a new page.

- [ ] **Step 5: Add model tests**

Cover:

- ready -> ok chip;
- restart_pending -> fix chip;
- manifest_unreachable -> fix chip;
- stale_state -> fix chip.

- [ ] **Step 6: Verify focused**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run src/pages/available-modules-model.test.ts
```

Expected: selected tests pass.

## Task 5: Service Module Scaffold Upgrade

**Budget:** 1.5 hours.

**Files:**
- Modify: `lenso-cli/src/module.rs`
- Modify: `lenso-cli/README.md`
- Modify: `lenso-examples/examples/support-ticket/README.md`

**Intent:** `lenso module create --remote` should generate a service module that looks like the product story: manifest, ready endpoint, install service declaration, smoke command, and clear run/install commands.

- [ ] **Step 1: Add a proper ready endpoint to generated backend**

Update generated `server.mjs` so it responds to:

```text
GET /readyz
GET /lenso/module/v1/manifest
```

`/readyz` returns:

```json
{ "status": "ready" }
```

- [ ] **Step 2: Add install service declaration to generated manifest**

In generated `lenso.module.json`, include:

```json
"install": {
  "services": [
    {
      "name": "api",
      "command": "pnpm --dir backend dev",
      "readyUrl": "http://127.0.0.1:4100/readyz",
      "readyTimeoutMs": 10000,
      "autoStart": true
    }
  ]
}
```

Keep this local-dev oriented. Do not introduce deployment config.

- [ ] **Step 3: Update generated README**

Generated README should show:

```sh
pnpm --dir backend dev
lenso module install http://127.0.0.1:4100/lenso/module/v1/manifest
lenso module service list
lenso module doctor <module>
```

- [ ] **Step 4: Add tests for generated scaffold strings**

Existing scaffold tests should assert:

- generated README contains `lenso module service list`;
- manifest contains `install.services[0].readyUrl`;
- backend source contains `/readyz`.

- [ ] **Step 5: Verify focused**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked --bin lenso remote
```

Expected: selected remote scaffold tests pass.

## Task 6: Linked-To-Service Extraction Surface

**Budget:** 1 hour.

**Files:**
- Modify: `lenso/docs/architecture/linked-to-service-module.md`
- Modify: `lenso/docs/architecture/service-module-boundary.md`
- Modify: `lenso/skills/lenso-remote-module-authoring/SKILL.md`
- Modify: `lenso/skills/lenso-start/SKILL.md`

**Intent:** Make extraction a real framework story, not a paragraph.

- [ ] **Step 1: Add an extraction checklist**

In `linked-to-service-module.md`, add:

```md
## Extraction Checklist

- [ ] Freeze manifest `name`.
- [ ] Freeze capability names.
- [ ] Freeze HTTP route paths and methods.
- [ ] Freeze runtime function names and schemas.
- [ ] Freeze event handler names and schemas.
- [ ] Freeze admin action/query names and schemas.
- [ ] Move implementation into the service process.
- [ ] Install the service manifest.
- [ ] Remove linked registration.
- [ ] Restart API and worker.
- [ ] Verify `lenso module doctor <name>`.
- [ ] Verify Console service status and Remote Calls.
```

- [ ] **Step 2: Add a migration warning**

Add:

```md
Do not change contract names during extraction. A rename is a second migration
and should be reviewed separately from moving the implementation out of process.
```

- [ ] **Step 3: Update skills**

In `lenso-remote-module-authoring`, instruct agents to:

- preserve manifest identity during extraction;
- use lifecycle commands;
- keep Host-owned runtime boundaries.

In `lenso-start`, route "extract this module" prompts to the service-module authoring skill.

- [ ] **Step 4: Verify docs**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
git diff --check
```

Expected: no whitespace errors.

## Task 7: Operator Runbook And Docs

**Budget:** 1 hour.

**Files:**
- Create: `lenso/docs/architecture/service-module-operator-runbook.md`
- Modify: `lenso/docs/architecture/service-module-boundary.md`
- Modify: `lenso-cli/README.md`
- Modify: `lenso-examples/docs/support-ticket-service-module-run.md`

- [ ] **Step 1: Add runbook**

Create:

```md
# Service Module Operator Runbook

| State | Meaning | CLI | Console | Fix |
| --- | --- | --- | --- | --- |
| ready | Source is configured, manifest/load checks are good, service checks pass. | `lenso module doctor <name>` | Service status `ready`. | None. |
| restart_pending | `.env` or install state changed after API/worker startup. | `lenso module doctor <name>` | Service status `restart_pending`. | Restart API and worker. |
| configured_not_loaded | Source exists, but module metadata is absent. | `lenso module doctor <name>` | Service status `configured_not_loaded`. | Restart, then inspect manifest errors. |
| manifest_unreachable | HTTP manifest URL does not respond. | `manifestStatus: unreachable` | Manifest `unreachable`. | Start service or fix `REMOTE_MODULES`. |
| service_not_ready | Declared `readyUrl` does not respond. | `service_not_ready` | Service status `service_not_ready`. | Start service or inspect process logs. |
| stale_state | Lock/pid state exists for a service that is not ready. | `stale_lock_or_pid` | Service status `stale_state`. | Restart API/worker; remove stale files if still stuck. |
```

- [ ] **Step 2: Link runbook**

Link it from `service-module-boundary.md` and support-ticket guide.

- [ ] **Step 3: Update CLI README**

Add:

```sh
lenso module service list
lenso module service status support-ticket api
lenso module service start support-ticket api
lenso module service stop support-ticket api
lenso module doctor support-ticket --json
```

- [ ] **Step 4: Keep docs short**

Do not add marketplace, deployment, or Kubernetes docs in this task.

## Task 8: Final Verification

**Budget:** 1 hour.

Run only these final checks:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
git diff --check
cargo test --locked --bin lenso
```

```sh
cd /Users/leosouthey/Projects/framework/lenso
git diff --check
just arch-check
HTTP_HOST=127.0.0.1 cargo test --locked -p lenso-api --test admin_data_console service_modules
```

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
git diff --check
pnpm exec vitest run src/pages/available-modules-model.test.ts src/data/available-modules.test.ts src/pages/data-render-model.test.ts
```

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
git diff --check
pnpm host-api-smoke:support-ticket
```

Report skipped scope:

```text
Skipped service discovery, gateway, service mesh, deployment orchestration,
protocol version negotiation, distributed transactions, schema registry, and
automatic linked-to-service migration.
```

## Self-Review

- Spec coverage: this is now a 10-hour slice across CLI lifecycle, Host API lifecycle, Console cockpit, scaffold output, extraction docs, and one final support-ticket proof.
- Smoke policy: only one final support-ticket host smoke is planned; intermediate checks are targeted unit/model/API tests.
- Type consistency: Rust API uses `AdminServiceModuleLifecycle*`; TypeScript uses `ServiceModuleLifecycle*`; CLI uses `ModuleDoctor*`.
- YAGNI check: no service discovery, gateway, mesh, orchestrator, or automatic migration.
