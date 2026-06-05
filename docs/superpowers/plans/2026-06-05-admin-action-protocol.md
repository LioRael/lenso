# Admin Action Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a host-owned admin action invocation protocol for manifest-declared module admin actions.

**Architecture:** Keep serializable declarations in `ModuleManifest` and put action behavior behind a new narrow `AdminActionSource` trait in `platform-module`. `platform-admin-data` exposes one OpenAPI-covered endpoint that validates the loaded module and declared action before invoking the injected source; the Runtime Console renders declarative action buttons and posts through the host endpoint.

**Tech Stack:** Rust, Axum, utoipa-axum, async-trait, serde_json, Vite/React, TanStack Query, generated OpenAPI + TypeScript SDK.

---

## File Structure

- Modify `crates/platform-module/src/admin_data.rs`: add `AdminActionSource` beside the read-only data seam.
- Modify `crates/platform-module/src/module.rs`: carry optional action behavior on `Module`.
- Modify `crates/platform-module/src/lib.rs`: export the new seam.
- Modify `crates/platform-admin-data/src/dto.rs`: add request/response DTOs for action invocation.
- Modify `crates/platform-admin-data/src/lib.rs`: add `AdminModule.action_source` and register the route.
- Modify `crates/platform-admin-data/src/handlers.rs`: add `POST /admin/data/{module}/actions/{action}` handler and tests.
- Modify `crates/app-bootstrap/src/lib.rs`: pass linked/remote action sources into `AdminModule`; remote entries get `None` for now.
- Modify `apps/api/tests/admin_data_console.rs`: add end-to-end API coverage for successful and unknown action invocation.
- Modify `apps/runtime-console/src/pages/data-page.tsx`: render declarative action buttons and call the endpoint.
- Modify `apps/runtime-console/src/pages/data-render-model.ts`: add small helpers for displayable actions if needed.
- Modify `apps/runtime-console/src/pages/data-render-model.test.ts`: cover action display helpers if added.
- Regenerate `contracts/openapi/app-api.v1.yaml` and `packages/ts-sdk/src/generated/*` with `just generate`.

## Task 1: Backend Red Test

**Files:**
- Modify: `apps/api/tests/admin_data_console.rs`

- [ ] **Step 1: Write failing integration tests**

Add a stub action source and two tests:

```rust
#[derive(Debug)]
struct StubActions;

#[async_trait::async_trait]
impl AdminActionSource for StubActions {
    async fn invoke(
        &self,
        action: &str,
        input: serde_json::Value,
    ) -> platform_core::AppResult<serde_json::Value> {
        Ok(serde_json::json!({
            "action": action,
            "dry_run": input.get("dry_run").and_then(serde_json::Value::as_bool).unwrap_or(false),
        }))
    }
}
```

Install an `AdminModule` whose declarative admin surface declares `sync_contacts`, then assert:

```rust
let response = app
    .oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/admin/data/remote-crm/actions/sync_contacts")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"input":{"dry_run":true}}"#))
            .unwrap(),
    )
    .await
    .unwrap();

assert_eq!(response.status(), StatusCode::OK);
```

Also post to `/admin/data/remote-crm/actions/missing_action` and assert `StatusCode::NOT_FOUND`.

- [ ] **Step 2: Run the red test**

Run:

```sh
cargo test --locked -p app-api --test admin_data_console admin_action -- --nocapture
```

Expected: FAIL because `AdminActionSource`, `AdminModule.action_source`, and the action route do not exist yet.

## Task 2: Platform Action Seam

**Files:**
- Modify: `crates/platform-module/src/admin_data.rs`
- Modify: `crates/platform-module/src/module.rs`
- Modify: `crates/platform-module/src/lib.rs`

- [ ] **Step 1: Implement the action trait**

Add:

```rust
#[async_trait::async_trait]
pub trait AdminActionSource: std::fmt::Debug + Send + Sync {
    async fn invoke(
        &self,
        action: &str,
        input: serde_json::Value,
    ) -> platform_core::AppResult<serde_json::Value>;
}
```

- [ ] **Step 2: Add action behavior to `Module`**

Add `pub admin_actions: Option<Arc<dyn AdminActionSource>>` and initialize it to `None` in `Module::new`. Add:

```rust
pub fn with_admin_actions(mut self, actions: Arc<dyn AdminActionSource>) -> Self {
    self.admin_actions = Some(actions);
    self
}
```

- [ ] **Step 3: Export the trait**

Update `lib.rs` to re-export `AdminActionSource`.

- [ ] **Step 4: Check the package**

Run:

```sh
cargo check --locked -p platform-module --all-targets
```

Expected: PASS.

## Task 3: Admin Data Endpoint

**Files:**
- Modify: `crates/platform-admin-data/src/dto.rs`
- Modify: `crates/platform-admin-data/src/lib.rs`
- Modify: `crates/platform-admin-data/src/handlers.rs`
- Modify: `crates/app-bootstrap/src/lib.rs`
- Modify: `apps/api/tests/admin_data_console.rs`

- [ ] **Step 1: Add DTOs**

Add:

```rust
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AdminActionInvokeRequest {
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminActionInvokeResponse {
    pub data: serde_json::Value,
}
```

- [ ] **Step 2: Add action source to registry entries**

Add `pub action_source: Option<Arc<dyn AdminActionSource>>` to `AdminModule` and update all constructors with `None` or a concrete source.

- [ ] **Step 3: Implement route handler**

Add a `#[utoipa::path]`-annotated handler:

```rust
#[utoipa::path(
    post,
    path = "/admin/data/{module}/actions/{action}",
    params(("module" = String, Path), ("action" = String, Path)),
    request_body = AdminActionInvokeRequest,
    responses((status = 200, body = AdminActionInvokeResponse), (status = 404, body = ApiErrorResponse)),
    tag = "Admin Data"
)]
```

The handler should load the module snapshot, verify a declared declarative action with the requested name exists, verify an action source exists, invoke it with `request.input`, and return `Json(AdminActionInvokeResponse { data })`.

- [ ] **Step 4: Wire route and app-bootstrap**

Register the route with `routes!(invoke_action)` in `platform-admin-data::router()`. In `app-bootstrap`, set `action_source: module.admin_actions.clone()` for linked module entries and `None` for failed/remote metadata-only entries.

- [ ] **Step 5: Run backend tests**

Run:

```sh
cargo test --locked -p platform-admin-data --all-targets
cargo test --locked -p app-api --test admin_data_console admin_action -- --nocapture
```

Expected: PASS.

## Task 4: Runtime Console Action UI

**Files:**
- Modify: `apps/runtime-console/src/pages/data-page.tsx`
- Modify: `apps/runtime-console/src/pages/data-render-model.ts`
- Modify: `apps/runtime-console/src/pages/data-render-model.test.ts`

- [ ] **Step 1: Add an action API helper**

Create a small fetch helper in `data-page.tsx`:

```ts
async function invokeAdminAction(moduleName: string, actionName: string) {
  return fetchJson<unknown>(`/admin/data/${moduleName}/actions/${actionName}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ input: {} }),
  });
}
```

- [ ] **Step 2: Render declarative actions**

In `DeclarativeSurface`, render `surface.actions` as compact buttons above the page sections. Disable buttons while a mutation is pending and show a concise success/error status line.

- [ ] **Step 3: Add focused UI/model tests if helper logic is extracted**

If action rendering uses a helper, test the helper with `pnpm --dir apps/runtime-console run test -- data-render-model.test.ts`.

- [ ] **Step 4: Run console checks**

Run:

```sh
pnpm --dir apps/runtime-console run typecheck
pnpm --dir apps/runtime-console run test
```

Expected: PASS.

## Task 5: Generate And Verify

**Files:**
- Modify generated: `contracts/openapi/app-api.v1.yaml`
- Modify generated: `packages/ts-sdk/src/generated/*`

- [ ] **Step 1: Regenerate contracts and SDK**

Run:

```sh
just generate
```

- [ ] **Step 2: Review generated diffs**

Run:

```sh
git diff -- contracts packages/ts-sdk/src/generated
```

Expected: new `POST /admin/data/{module}/actions/{action}` path and generated request/response types.

- [ ] **Step 3: Run freshness and quality gates**

Run:

```sh
just generated-check
just arch-check
just sdk-check
just console-check
just ci
```

Expected: PASS.

## Self-Review

- Spec coverage: plan covers framework seam, backend endpoint, OpenAPI/SDK generation, Runtime Console invocation, and validation.
- Placeholder scan: no `TBD`, `TODO`, or unspecified implementation steps remain.
- Type consistency: action declarations remain `AdminAction` manifest data; behavior is carried by `AdminActionSource`; `AdminModule` is the injected backend registry entry.
