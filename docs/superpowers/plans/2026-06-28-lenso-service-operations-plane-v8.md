# Lenso Service Operations Plane V8 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make service-provided HTTP routes, runtime functions, event handlers, and admin actions visible as host-owned service operations with CLI checks, Console evidence links, local logs, and TypeScript/Rust proof services.

**Architecture:** Keep service operations as a computed view over existing manifests, metadata snapshots, install receipts, and service process state. Do not add a new runtime table or registry in V8; execution remains in the existing host HTTP proxy, runtime function, outbox event, and admin action paths. Use one small operation metadata object on manifest declarations and keep old manifests valid.

**Tech Stack:** Rust 2024, serde/serde_json, utoipa, clap, reqwest, TypeScript, Vitest, React, TanStack Query/Router, pnpm, Axum examples.

---

## File Structure

- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/operation.rs`
  - New shared Rust contract types for optional operation metadata.
- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/http.rs`
  - Add optional `operation` metadata to HTTP route declarations.
- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/runtime.rs`
  - Add optional `operation` metadata to runtime function declarations.
- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/events.rs`
  - Add optional `operation` metadata to event handler declarations.
- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/admin.rs`
  - Add optional `operation` metadata to admin actions.
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
  - Add operation DTOs to the service lifecycle response.
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
  - Compute operations from existing module metadata.
- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs`
  - Cover operation catalog output.
- `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`
  - Refresh generated OpenAPI after DTO changes.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.ts`
  - Add TS operation metadata types, builder wiring, and host context reader.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.test.ts`
  - Cover metadata serialization and host context parsing.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/src/index.ts`
  - Re-export the new operation helpers through the service kit.
- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
  - Add `service check --operation`, `--sample-input`, and `service logs`.
- `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
  - Normalize manifest operations, run explicit safe probes, and capture local service logs.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
  - Type service operations from the admin-data API.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`
  - Group operations under provider rows and compute operation detail links.
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`
  - Add the operation list/detail panel in `/services`.
- `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/module.ts`
  - Add V8 operation metadata and safe probes to the TypeScript proof.
- `/Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/src/main.rs`
  - Add one Rust runtime operation and operation metadata.

## Scope Check

V8 spans four repositories, but each task leaves working, testable software:

- Task 1 gives manifests a metadata shape.
- Task 2 exposes operations from the host API.
- Task 3 makes TypeScript services author that shape.
- Task 4 makes CLI checks operation-aware.
- Task 5 renders operations in Console.
- Task 6 captures local managed-service logs.
- Task 7 upgrades examples as proof.

No task adds service discovery, a gateway, a schema registry, a database-backed operation registry, or a log platform.

### Task 1: Rust Operation Metadata Contract

**Files:**
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/operation.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/lib.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/http.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/runtime.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/events.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/admin.rs`

- [ ] **Step 1: Write the failing contract test**

Append this test module to `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/operation.rs` after creating the file with the types from Step 3:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AdminAction, AdminActionDangerLevel, ModuleHttpMethod, ModuleHttpRoute,
        RuntimeFunctionDeclaration,
    };
    use serde_json::json;

    #[test]
    fn operation_metadata_serializes_on_route_runtime_and_action() {
        let route = ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/tickets".to_owned(),
            capability: Some("support_ticket.tickets.read".to_owned()),
            display_name: Some("List tickets".to_owned()),
            story_title: Some("Tickets listed".to_owned()),
            operation: Some(ServiceOperationMetadata {
                operation_id: Some("support-ticket/http/GET:/tickets".to_owned()),
                summary: Some("List tickets".to_owned()),
                safe_probe: Some(ServiceOperationSafeProbe {
                    method: Some("GET".to_owned()),
                    path: Some("/tickets".to_owned()),
                    input: None,
                    expect_status: Some(200),
                }),
                ..ServiceOperationMetadata::default()
            }),
        };
        let route_json = serde_json::to_value(route).unwrap();
        assert_eq!(
            route_json["operation"]["operationId"],
            "support-ticket/http/GET:/tickets"
        );
        assert_eq!(route_json["operation"]["safeProbe"]["expectStatus"], 200);

        let function = RuntimeFunctionDeclaration {
            name: "support-ticket.escalate-ticket.v1".to_owned(),
            version: 1,
            queue: "support-ticket".to_owned(),
            input_schema: None,
            retry_policy: None,
            operation: Some(ServiceOperationMetadata {
                idempotency: Some(ServiceOperationIdempotency::RequiresKey),
                timeout_ms: Some(2_000),
                ..ServiceOperationMetadata::default()
            }),
        };
        let function_json = serde_json::to_value(function).unwrap();
        assert_eq!(function_json["operation"]["idempotency"], "requires_key");
        assert_eq!(function_json["operation"]["timeoutMs"], 2000);

        let action = AdminAction {
            name: "assign_ticket".to_owned(),
            label: "Assign ticket".to_owned(),
            capability: "support_ticket.tickets.write".to_owned(),
            input_schema: None,
            confirmation: None,
            danger_level: AdminActionDangerLevel::Low,
            operation: Some(ServiceOperationMetadata {
                output_schema: Some(json!({ "type": "object" })),
                ..ServiceOperationMetadata::default()
            }),
        };
        let action_json = serde_json::to_value(action).unwrap();
        assert_eq!(action_json["operation"]["outputSchema"]["type"], "object");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```sh
cargo test --locked -p lenso-contracts operation_metadata_serializes_on_route_runtime_and_action
```

Expected: FAIL because `operation.rs` and the `operation` fields do not exist.

- [ ] **Step 3: Add the minimal operation metadata types**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/operation.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOperationMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_probe: Option<ServiceOperationSafeProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<ServiceOperationIdempotency>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOperationSafeProbe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_status: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceOperationIdempotency {
    None,
    Idempotent,
    RequiresKey,
}
```

Modify `/Users/leosouthey/Projects/framework/lenso/crates/lenso-contracts/src/lib.rs`:

```rust
pub mod operation;
pub use operation::{
    ServiceOperationIdempotency, ServiceOperationMetadata, ServiceOperationSafeProbe,
};
```

Add this field to `ModuleHttpRoute`, `RuntimeFunctionDeclaration`, `EventHandlerDeclaration`, and `AdminAction`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub operation: Option<crate::ServiceOperationMetadata>,
```

- [ ] **Step 4: Update constructors/tests that instantiate changed structs**

Search:

```sh
rg "ModuleHttpRoute \\{|RuntimeFunctionDeclaration \\{|EventHandlerDeclaration \\{|AdminAction \\{" crates modules
```

For each struct literal, add:

```rust
operation: None,
```

Expected: this is a mechanical compile fix only; do not change behavior.

- [ ] **Step 5: Run the contract tests**

Run:

```sh
cargo test --locked -p lenso-contracts operation_metadata_serializes_on_route_runtime_and_action
cargo test --locked -p lenso-contracts manifest_with_http_routes_round_trips_through_json
```

Expected: PASS.

- [ ] **Step 6: Commit**

```sh
git add crates/lenso-contracts/src
git commit -m "feat: add service operation metadata contract"
```

### Task 2: Host Operation Catalog In Service Lifecycle API

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml`

- [ ] **Step 1: Write the failing API test**

Append this test to `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs` near the existing `service_modules_*` tests:

```rust
#[tokio::test]
async fn service_modules_exposes_operations_for_provider_modules() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n",
    );
    let _ledger = FileFixture::write(
        ".lenso/module-installs.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "support-ticket",
                "source": "remote",
                "service": {
                    "name": "support-suite-provider",
                    "baseUrl": "http://127.0.0.1:4110/lenso/service/v1",
                    "statusPath": "/lenso/service/v1/status",
                    "statusUrl": "http://127.0.0.1:4110/lenso/service/v1/status",
                    "version": "0.1.0"
                }
            }]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "support-ticket".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/tickets".to_owned(),
            capability: Some("support_ticket.tickets.read".to_owned()),
            display_name: Some("List tickets".to_owned()),
            story_title: Some("Tickets listed".to_owned()),
            operation: Some(ServiceOperationMetadata {
                operation_id: Some("support-ticket/http/GET:/tickets".to_owned()),
                summary: Some("List tickets".to_owned()),
                safe_probe: Some(ServiceOperationSafeProbe {
                    method: Some("GET".to_owned()),
                    path: Some("/tickets".to_owned()),
                    input: None,
                    expect_status: Some(200),
                }),
                ..ServiceOperationMetadata::default()
            }),
        }],
        runtime: Some(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: "support-ticket.escalate-ticket.v1".to_owned(),
                version: 1,
                queue: "support-ticket".to_owned(),
                input_schema: None,
                retry_policy: None,
                operation: None,
            }],
            schedules: vec![],
        }),
        events: Some(EventSurface {
            handlers: vec![EventHandlerDeclaration {
                name: "ticket_created".to_owned(),
                event_name: "support.ticket_created.v1".to_owned(),
                operation: None,
            }],
        }),
        lifecycle: None,
        console: vec![],
        story_display: vec![],
        capabilities: vec!["support_ticket.tickets.read".to_owned()],
        dependencies: vec![],
        admin: Some(AdminSurface::DeclarativeCustom(AdminDeclarativeSurface {
            pages: vec![],
            actions: vec![AdminAction {
                name: "assign_ticket".to_owned(),
                label: "Assign ticket".to_owned(),
                capability: "support_ticket.tickets.write".to_owned(),
                input_schema: None,
                confirmation: None,
                danger_level: AdminActionDangerLevel::Low,
                operation: None,
            }],
            fallback_schema: None,
        })),
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http".to_owned(),
                base_url: "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket"
                    .to_owned(),
                manifest_url:
                    "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket/manifest"
                        .to_owned(),
                timeout_ms: 5000,
                auth_configured: false,
                load_duration_ms: Some(10),
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let operations = body["modules"][0]["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 4);
    assert_eq!(operations[0]["kind"], "http_route");
    assert_eq!(operations[0]["operationId"], "support-ticket/http/GET:/tickets");
    assert_eq!(operations[0]["capability"], "support_ticket.tickets.read");
    assert_eq!(operations[0]["safeProbe"], true);
    assert_eq!(operations[0]["links"]["remoteCalls"], "/operations/remote-calls?module=support-ticket");
    assert_eq!(operations[1]["kind"], "runtime_function");
    assert_eq!(operations[1]["links"]["runtime"], "/operations/functions?module=support-ticket");
    assert_eq!(operations[2]["kind"], "event_handler");
    assert_eq!(operations[3]["kind"], "admin_action");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```sh
cargo test --locked -p lenso-api service_modules_exposes_operations_for_provider_modules
```

Expected: FAIL because `operations` is missing from the service lifecycle DTO.

- [ ] **Step 3: Add operation DTOs**

Add to `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`:

```rust
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminServiceOperationKindDto {
    HttpRoute,
    RuntimeFunction,
    EventHandler,
    AdminAction,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceOperationLinksDto {
    pub remote_calls: Option<String>,
    pub runtime: Option<String>,
    pub story: String,
    pub technical_operations: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceOperationDto {
    pub operation_id: String,
    pub provider_name: Option<String>,
    pub module_name: String,
    pub kind: AdminServiceOperationKindDto,
    pub name: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub capability: Option<String>,
    pub summary: Option<String>,
    pub safe_probe: bool,
    pub links: AdminServiceOperationLinksDto,
    pub next_action: String,
}
```

Add this field to `AdminServiceModuleLifecycleModuleDto`:

```rust
pub operations: Vec<AdminServiceOperationDto>,
```

- [ ] **Step 4: Compute operations from existing metadata**

In `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`, add helper functions near `service_module_lifecycle_module`:

```rust
fn service_module_operations(
    provider_name: Option<&str>,
    module_name: &str,
    metadata: Option<&AdminModuleMetadata>,
) -> Vec<AdminServiceOperationDto> {
    let Some(metadata) = metadata else {
        return Vec::new();
    };
    let mut operations = Vec::new();
    for route in &metadata.http_routes {
        let method = format!("{:?}", route.method).to_ascii_uppercase();
        let operation_id = route
            .operation
            .as_ref()
            .and_then(|operation| operation.operation_id.clone())
            .unwrap_or_else(|| format!("{module_name}/http/{method}:{}", route.path));
        operations.push(service_operation_dto(
            provider_name,
            module_name,
            AdminServiceOperationKindDto::HttpRoute,
            operation_id,
            route.display_name.clone().unwrap_or_else(|| route.path.clone()),
            Some(method),
            Some(route.path.clone()),
            route.capability.clone(),
            route
                .operation
                .as_ref()
                .and_then(|operation| operation.summary.clone())
                .or_else(|| route.display_name.clone()),
            route
                .operation
                .as_ref()
                .and_then(|operation| operation.safe_probe.as_ref())
                .is_some(),
        ));
    }
    if let Some(runtime) = &metadata.runtime {
        for function in &runtime.functions {
            let operation_id = function
                .operation
                .as_ref()
                .and_then(|operation| operation.operation_id.clone())
                .unwrap_or_else(|| format!("{module_name}/runtime/{}", function.name));
            operations.push(service_operation_dto(
                provider_name,
                module_name,
                AdminServiceOperationKindDto::RuntimeFunction,
                operation_id,
                function.name.clone(),
                None,
                None,
                None,
                function
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.summary.clone()),
                function
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.safe_probe.as_ref())
                    .is_some(),
            ));
        }
    }
    if let Some(events) = &metadata.events {
        for handler in &events.handlers {
            let operation_id = handler
                .operation
                .as_ref()
                .and_then(|operation| operation.operation_id.clone())
                .unwrap_or_else(|| format!("{module_name}/event/{}", handler.name));
            operations.push(service_operation_dto(
                provider_name,
                module_name,
                AdminServiceOperationKindDto::EventHandler,
                operation_id,
                handler.name.clone(),
                None,
                None,
                None,
                handler
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.summary.clone())
                    .or_else(|| Some(handler.event_name.clone())),
                handler
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.safe_probe.as_ref())
                    .is_some(),
            ));
        }
    }
    if let Some(AdminSurface::DeclarativeCustom(surface)) = &metadata.admin {
        for action in &surface.actions {
            let operation_id = action
                .operation
                .as_ref()
                .and_then(|operation| operation.operation_id.clone())
                .unwrap_or_else(|| format!("{module_name}/action/{}", action.name));
            operations.push(service_operation_dto(
                provider_name,
                module_name,
                AdminServiceOperationKindDto::AdminAction,
                operation_id,
                action.name.clone(),
                None,
                None,
                Some(action.capability.clone()),
                action
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.summary.clone())
                    .or_else(|| Some(action.label.clone())),
                action
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.safe_probe.as_ref())
                    .is_some(),
            ));
        }
    }
    operations.sort_by(|left, right| left.operation_id.cmp(&right.operation_id));
    operations
}

fn service_operation_dto(
    provider_name: Option<&str>,
    module_name: &str,
    kind: AdminServiceOperationKindDto,
    operation_id: String,
    name: String,
    method: Option<String>,
    path: Option<String>,
    capability: Option<String>,
    summary: Option<String>,
    safe_probe: bool,
) -> AdminServiceOperationDto {
    AdminServiceOperationDto {
        operation_id,
        provider_name: provider_name.map(str::to_owned),
        module_name: module_name.to_owned(),
        kind,
        name,
        method,
        path,
        capability,
        summary,
        safe_probe,
        links: AdminServiceOperationLinksDto {
            remote_calls: Some(format!("/operations/remote-calls?module={module_name}")),
            runtime: Some(format!("/operations/functions?module={module_name}")),
            story: format!("/?q={}", provider_name.unwrap_or(module_name)),
            technical_operations: format!("/operations?q={}", provider_name.unwrap_or(module_name)),
        },
        next_action: if safe_probe {
            "run lenso service check for this operation".to_owned()
        } else {
            "add safeProbe metadata before active checks".to_owned()
        },
    }
}
```

In `service_module_lifecycle_module`, compute and assign:

```rust
let operations = service_module_operations(provider_name, module_name, metadata);
```

and include:

```rust
operations,
```

- [ ] **Step 5: Run backend tests**

Run:

```sh
cargo test --locked -p lenso-api service_modules_exposes_operations_for_provider_modules
cargo test --locked -p lenso-api service_modules_exposes_release_status_and_deployment_metadata
```

Expected: PASS.

- [ ] **Step 6: Refresh OpenAPI**

Run the repo's existing OpenAPI generation command:

```sh
just openapi
```

Expected: `/Users/leosouthey/Projects/framework/lenso/contracts/openapi/app-api.v1.yaml` includes `operations` on `AdminServiceModuleLifecycleModuleDto`.

- [ ] **Step 7: Commit**

```sh
git add crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs crates/lenso-api/tests/admin_data_console.rs contracts/openapi/app-api.v1.yaml
git commit -m "feat: expose service operation catalog"
```

### Task 3: TypeScript Operation Metadata And Context Helpers

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/src/index.ts`

- [ ] **Step 1: Write failing TypeScript tests**

Add these tests to `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.test.ts`:

```ts
test("defines operation metadata on service declarations", () => {
  const module = defineModule({
    admin: declarativeCustom({
      actions: [
        adminAction("assign_ticket", {
          capability: "support_ticket.tickets.write",
          label: "Assign ticket",
          operation: {
            operationId: "support-ticket/action/assign_ticket",
            safeProbe: { input: { dry_run: true }, expectStatus: 200 },
            summary: "Assign ticket",
          },
        }),
      ],
    }),
    httpRoutes: [
      getRoute("/tickets", {
        capability: "support_ticket.tickets.read",
        displayName: "List tickets",
        operation: {
          operationId: "support-ticket/http/GET:/tickets",
          safeProbe: { method: "GET", path: "/tickets", expectStatus: 200 },
        },
      }),
    ],
    eventHandlers: [
      eventHandler("ticket_created", "support.ticket_created.v1", {
        operation: { summary: "Handle created ticket" },
      }),
    ],
    name: "support-ticket",
    runtimeFunctions: [
      runtimeFunction("support-ticket.escalate-ticket.v1", {
        operation: {
          idempotency: "requires_key",
          safeProbe: { input: { dry_run: true }, expectStatus: 200 },
        },
        queue: "support-ticket",
      }),
    ],
  });

  expect(module.http_routes[0]?.operation?.operationId).toBe(
    "support-ticket/http/GET:/tickets"
  );
  expect(module.runtime?.functions[0]?.operation?.idempotency).toBe(
    "requires_key"
  );
  expect(module.events?.handlers[0]?.operation?.summary).toBe(
    "Handle created ticket"
  );
  expect(
    module.admin?.kind === "declarative_custom"
      ? module.admin.actions[0]?.operation?.safeProbe?.input
      : null
  ).toEqual({ dry_run: true });
});

test("reads host invocation context from request headers", () => {
  const request = {
    headers: {
      "x-lenso-actor-kind": "worker",
      "x-lenso-causation-id": "httpreq_1",
      "x-lenso-correlation-id": "corr_1",
      "x-lenso-module": "support-ticket",
      "x-lenso-operation": "support-ticket/runtime/escalate",
      "x-lenso-operation-kind": "runtime_function",
      "x-lenso-provider": "support-suite-provider",
      "x-request-id": "req_1",
      traceparent: "00-00000000000000000000000000000001-0000000000000001-01",
    },
  } as unknown as import("node:http").IncomingMessage;

  expect(readLensoInvocationContext(request)).toEqual({
    actorKind: "worker",
    causationId: "httpreq_1",
    correlationId: "corr_1",
    moduleName: "support-ticket",
    operationId: "support-ticket/runtime/escalate",
    operationKind: "runtime_function",
    providerName: "support-suite-provider",
    requestId: "req_1",
    traceparent:
      "00-00000000000000000000000000000001-0000000000000001-01",
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```sh
pnpm --dir /Users/leosouthey/Projects/framework/lenso-runtime-console exec vitest run packages/remote-module-kit/src/index.test.ts
```

Expected: FAIL because `operation` options and `readLensoInvocationContext` do not exist.

- [ ] **Step 3: Add operation metadata types and builder wiring**

Add to `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.ts` near route/runtime declarations:

```ts
export type ServiceOperationIdempotency =
  | "none"
  | "idempotent"
  | "requires_key";

export interface ServiceOperationSafeProbe {
  method?: RemoteHttpMethod | string;
  path?: string;
  input?: unknown;
  expectStatus?: number;
}

export interface ServiceOperationMetadata {
  operationId?: string;
  summary?: string;
  inputSchema?: unknown;
  outputSchema?: unknown;
  safeProbe?: ServiceOperationSafeProbe;
  timeoutMs?: number;
  idempotency?: ServiceOperationIdempotency;
}
```

Add `operation?: ServiceOperationMetadata` to:

```ts
export interface RemoteHttpRoute {
  operation?: ServiceOperationMetadata;
}

export interface RemoteHttpRouteOptions {
  operation?: ServiceOperationMetadata;
}

export interface RemoteRuntimeFunctionDeclaration {
  operation?: ServiceOperationMetadata;
}

export interface RemoteRuntimeFunctionOptions {
  operation?: ServiceOperationMetadata;
}

export interface RemoteEventHandlerDeclaration {
  operation?: ServiceOperationMetadata;
}

export interface AdminAction {
  operation?: ServiceOperationMetadata;
}
```

Update builders:

```ts
const route = (
  method: RemoteHttpMethod,
  path: string,
  options: RemoteHttpRouteOptions = {}
): RemoteHttpRoute => ({
  ...(options.capability ? { capability: options.capability } : {}),
  ...(options.displayName ? { display_name: options.displayName } : {}),
  method,
  ...(options.operation ? { operation: options.operation } : {}),
  path,
  ...(options.storyTitle ? { story_title: options.storyTitle } : {}),
});

export const runtimeFunction = (
  name: string,
  options: RemoteRuntimeFunctionOptions = {}
): RemoteRuntimeFunctionDeclaration => ({
  name,
  ...(options.inputSchema ? { input_schema: options.inputSchema } : {}),
  ...(options.operation ? { operation: options.operation } : {}),
  queue: options.queue ?? runtimeFunctionQueue(name),
  ...(options.retryPolicy ? { retry_policy: options.retryPolicy } : {}),
  version: options.version ?? 1,
});

export const eventHandler = (
  name: string,
  eventName: string,
  options: { operation?: ServiceOperationMetadata } = {}
): RemoteEventHandlerDeclaration => ({
  event_name: eventName,
  name,
  ...(options.operation ? { operation: options.operation } : {}),
});
```

Update `adminAction` options to carry `operation` into the returned action object.

- [ ] **Step 4: Add host invocation context reader**

Add to `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/remote-module-kit/src/index.ts`:

```ts
export interface LensoInvocationContext {
  requestId?: string;
  correlationId?: string;
  causationId?: string;
  providerName?: string;
  moduleName?: string;
  operationId?: string;
  operationKind?: string;
  actorKind?: string;
  traceparent?: string;
}

const headerValue = (
  request: IncomingMessage,
  name: string
): string | undefined => {
  const value = request.headers[name.toLowerCase()];
  return Array.isArray(value) ? value[0] : value;
};

export function readLensoInvocationContext(
  request: IncomingMessage
): LensoInvocationContext {
  return {
    actorKind: headerValue(request, "x-lenso-actor-kind"),
    causationId: headerValue(request, "x-lenso-causation-id"),
    correlationId: headerValue(request, "x-lenso-correlation-id"),
    moduleName: headerValue(request, "x-lenso-module"),
    operationId: headerValue(request, "x-lenso-operation"),
    operationKind: headerValue(request, "x-lenso-operation-kind"),
    providerName: headerValue(request, "x-lenso-provider"),
    requestId: headerValue(request, "x-request-id"),
    traceparent: headerValue(request, "traceparent"),
  };
}
```

- [ ] **Step 5: Run package tests and build**

Run:

```sh
pnpm --dir /Users/leosouthey/Projects/framework/lenso-runtime-console exec vitest run packages/remote-module-kit/src/index.test.ts packages/service-kit/src/index.test.ts
pnpm --dir /Users/leosouthey/Projects/framework/lenso-runtime-console --filter @lenso/service-kit build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```sh
git add packages/remote-module-kit/src/index.ts packages/remote-module-kit/src/index.test.ts packages/service-kit/src/index.ts
git commit -m "feat: add service operation metadata helpers"
```

### Task 4: Operation-Aware `lenso service check`

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`

- [ ] **Step 1: Write failing CLI parser test**

Add this test to `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`:

```rust
#[test]
fn parses_service_check_operation_filter_and_sample_input() {
    let cli = Cli::parse_from([
        "lenso",
        "service",
        "check",
        "http://127.0.0.1:4110/lenso/service/v1/manifest",
        "--operation",
        "support-ticket/http/GET:/tickets",
        "--sample-input",
        "probe.json",
    ]);

    let Command::Service {
        command: ServiceCommand::Check(args),
    } = cli.command
    else {
        panic!("expected service check");
    };

    assert_eq!(
        args.operation.as_deref(),
        Some("support-ticket/http/GET:/tickets")
    );
    assert_eq!(
        args.sample_input.as_deref(),
        Some(std::path::Path::new("probe.json"))
    );
}
```

- [ ] **Step 2: Run parser test to verify it fails**

Run:

```sh
cargo test --locked --manifest-path /Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml parses_service_check_operation_filter_and_sample_input
```

Expected: FAIL because `ServiceCheckArgs` has no `operation` or `sample_input`.

- [ ] **Step 3: Add CLI args and options**

In `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`, add to `ServiceCheckArgs`:

```rust
/// Only check one operation id.
#[arg(long)]
operation: Option<String>,

/// JSON sample input used for explicit safe probes.
#[arg(long)]
sample_input: Option<std::path::PathBuf>,
```

In the `ServiceCommand::Check` dispatch, pass these into `ServiceManifestCheckOptions`:

```rust
operation: args.operation.clone(),
sample_input: args.sample_input.clone(),
```

In `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`, extend `ServiceManifestCheckOptions`:

```rust
pub operation: Option<String>,
pub sample_input: Option<PathBuf>,
```

- [ ] **Step 4: Add operation normalization tests**

Add these tests to `mod tests` in `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`:

```rust
#[test]
fn service_manifest_operations_include_kinds_and_safe_probe_state() {
    let manifest = json!({
        "modules": [{
            "name": "support-ticket",
            "http_routes": [{
                "method": "GET",
                "path": "/tickets",
                "capability": "support_ticket.tickets.read",
                "operation": {
                    "operationId": "support-ticket/http/GET:/tickets",
                    "safeProbe": { "method": "GET", "path": "/tickets", "expectStatus": 200 }
                }
            }],
            "runtime": {
                "functions": [{
                    "name": "support-ticket.escalate-ticket.v1",
                    "version": 1,
                    "queue": "support-ticket"
                }]
            },
            "events": {
                "handlers": [{
                    "name": "ticket_created",
                    "event_name": "support.ticket_created.v1"
                }]
            },
            "admin": {
                "kind": "declarative_custom",
                "actions": [{
                    "name": "assign_ticket",
                    "label": "Assign ticket",
                    "capability": "support_ticket.tickets.write"
                }]
            }
        }],
        "name": "support-suite-provider",
        "version": "0.1.0"
    });

    let operations = service_manifest_operations(&manifest, None);

    assert_eq!(operations.len(), 4);
    assert_eq!(operations[0]["operationId"], "support-ticket/action/assign_ticket");
    assert_eq!(operations[1]["operationId"], "support-ticket/event/ticket_created");
    assert_eq!(operations[2]["operationId"], "support-ticket/http/GET:/tickets");
    assert_eq!(operations[2]["safeProbe"], true);
    assert_eq!(operations[3]["operationId"], "support-ticket/runtime/support-ticket.escalate-ticket.v1");
}

#[test]
fn service_manifest_operations_filter_by_operation_id() {
    let manifest = json!({
        "modules": [{
            "name": "support-ticket",
            "http_routes": [
                { "method": "GET", "path": "/tickets" },
                { "method": "GET", "path": "/tickets/{id}" }
            ]
        }],
        "name": "support-suite-provider",
        "version": "0.1.0"
    });

    let operations = service_manifest_operations(
        &manifest,
        Some("support-ticket/http/GET:/tickets/{id}"),
    );

    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0]["path"], "/tickets/{id}");
}
```

- [ ] **Step 5: Implement operation normalization**

Add to `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs` near `service_check_declaration_summary`:

```rust
fn service_manifest_operations(manifest: &Value, filter: Option<&str>) -> Vec<Value> {
    let mut operations = Vec::new();
    for module in manifest
        .get("modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(module_name) = module.get("name").and_then(Value::as_str) else {
            continue;
        };
        for route in module
            .get("http_routes")
            .or_else(|| module.get("httpRoutes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let method = route
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_uppercase();
            let path = route.get("path").and_then(Value::as_str).unwrap_or("");
            let operation = route.get("operation").unwrap_or(&Value::Null);
            let operation_id = operation
                .get("operationId")
                .or_else(|| operation.get("operation_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{module_name}/http/{method}:{path}"));
            push_manifest_operation(
                &mut operations,
                filter,
                json!({
                    "capability": route.get("capability").and_then(Value::as_str),
                    "kind": "http_route",
                    "method": method,
                    "module": module_name,
                    "operationId": operation_id,
                    "path": path,
                    "safeProbe": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).is_some(),
                    "safeProbeSpec": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).cloned(),
                }),
            );
        }
        for function in module
            .get("runtime")
            .and_then(|runtime| runtime.get("functions"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = function.get("name").and_then(Value::as_str).unwrap_or("");
            let operation = function.get("operation").unwrap_or(&Value::Null);
            let operation_id = operation
                .get("operationId")
                .or_else(|| operation.get("operation_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{module_name}/runtime/{name}"));
            push_manifest_operation(
                &mut operations,
                filter,
                json!({
                    "kind": "runtime_function",
                    "module": module_name,
                    "name": name,
                    "operationId": operation_id,
                    "safeProbe": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).is_some(),
                    "safeProbeSpec": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).cloned(),
                }),
            );
        }
        for handler in module
            .get("events")
            .and_then(|events| events.get("handlers"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = handler.get("name").and_then(Value::as_str).unwrap_or("");
            let operation = handler.get("operation").unwrap_or(&Value::Null);
            let operation_id = operation
                .get("operationId")
                .or_else(|| operation.get("operation_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{module_name}/event/{name}"));
            push_manifest_operation(
                &mut operations,
                filter,
                json!({
                    "eventName": handler.get("event_name").and_then(Value::as_str),
                    "kind": "event_handler",
                    "module": module_name,
                    "name": name,
                    "operationId": operation_id,
                    "safeProbe": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).is_some(),
                    "safeProbeSpec": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).cloned(),
                }),
            );
        }
        for action in module
            .get("admin")
            .and_then(|admin| admin.get("actions"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = action.get("name").and_then(Value::as_str).unwrap_or("");
            let operation = action.get("operation").unwrap_or(&Value::Null);
            let operation_id = operation
                .get("operationId")
                .or_else(|| operation.get("operation_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{module_name}/action/{name}"));
            push_manifest_operation(
                &mut operations,
                filter,
                json!({
                    "capability": action.get("capability").and_then(Value::as_str),
                    "kind": "admin_action",
                    "module": module_name,
                    "name": name,
                    "operationId": operation_id,
                    "safeProbe": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).is_some(),
                    "safeProbeSpec": operation.get("safeProbe").or_else(|| operation.get("safe_probe")).cloned(),
                }),
            );
        }
    }
    operations.sort_by(|left, right| {
        left.get("operationId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(right.get("operationId").and_then(Value::as_str).unwrap_or(""))
    });
    operations
}

fn push_manifest_operation(operations: &mut Vec<Value>, filter: Option<&str>, operation: Value) {
    let operation_id = operation.get("operationId").and_then(Value::as_str);
    if filter.is_none_or(|filter| operation_id == Some(filter)) {
        operations.push(operation);
    }
}
```

- [ ] **Step 6: Replace old probe summary with operation-aware output**

In `check_service_manifest_reference`, compute:

```rust
let operations = service_manifest_operations(&manifest, options.operation.as_deref());
let probes = if let Some(manifest_url) = manifest_url.as_deref() {
    service_check_operation_probe_summary(
        &operations,
        manifest_url,
        options.sample_input.as_deref(),
    )
    .await?
} else {
    operations
        .iter()
        .map(|operation| {
            json!({
                "kind": operation.get("kind").cloned().unwrap_or(Value::Null),
                "operationId": operation.get("operationId").cloned().unwrap_or(Value::Null),
                "status": "skipped",
                "reason": "manifest URL unavailable",
            })
        })
        .collect::<Vec<_>>()
};
```

Add this helper for the first implementation:

```rust
async fn service_check_operation_probe_summary(
    operations: &[Value],
    manifest_url: &str,
    _sample_input: Option<&Path>,
) -> Result<Vec<Value>> {
    let service_base_url = manifest_url
        .strip_suffix("/manifest")
        .unwrap_or(manifest_url)
        .to_owned();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .context("build service probe HTTP client")?;
    let mut probes = Vec::new();
    for operation in operations {
        let kind = operation.get("kind").and_then(Value::as_str).unwrap_or("");
        let operation_id = operation
            .get("operationId")
            .and_then(Value::as_str)
            .unwrap_or("-");
        if kind != "http_route" || operation.get("safeProbe").and_then(Value::as_bool) != Some(true)
        {
            probes.push(json!({
                "kind": kind,
                "operationId": operation_id,
                "reason": "safeProbe not declared",
                "status": "skipped",
            }));
            continue;
        }
        let method = operation.get("method").and_then(Value::as_str).unwrap_or("");
        let path = operation.get("path").and_then(Value::as_str).unwrap_or("");
        let module_name = operation.get("module").and_then(Value::as_str).unwrap_or("");
        if method != "GET" || path.contains('{') || path.contains(':') {
            probes.push(json!({
                "kind": kind,
                "operationId": operation_id,
                "reason": "first safe probe slice only runs literal GET routes",
                "status": "skipped",
            }));
            continue;
        }
        let url = join_url_path(
            &service_base_url,
            &format!("modules/{module_name}/{}", path.trim_start_matches('/')),
        );
        let status = if remote_service_ready_url(&client, &url).await {
            "ok"
        } else {
            "failed"
        };
        probes.push(json!({
            "kind": kind,
            "method": method,
            "operationId": operation_id,
            "status": status,
            "url": url,
        }));
    }
    Ok(probes)
}
```

Text output should print:

```rust
println!("Operations:");
for operation in &operations {
    println!(
        "- {} {} {}",
        operation.get("kind").and_then(Value::as_str).unwrap_or("-"),
        operation.get("module").and_then(Value::as_str).unwrap_or("-"),
        operation.get("operationId").and_then(Value::as_str).unwrap_or("-")
    );
}
```

JSON output should include `"operations": operations`.

- [ ] **Step 7: Run CLI tests**

Run:

```sh
cargo fmt --manifest-path /Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml
cargo test --locked --manifest-path /Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml service
```

Expected: PASS.

- [ ] **Step 8: Commit**

```sh
git add src/main.rs src/module.rs
git commit -m "feat: check service operations"
```

### Task 5: Console Operation Detail

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`

- [ ] **Step 1: Write failing model test**

Add this test to `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`:

```ts
it("groups provider operations with evidence links", () => {
  const rows = serviceCenterRows({
    modules: [
      {
        configured: true,
        fixes: [],
        installed: true,
        loaded: true,
        manifestStatus: "reachable",
        moduleName: "support-ticket",
        operations: [
          {
            capability: "support_ticket.tickets.read",
            kind: "http_route",
            links: {
              remoteCalls: "/operations/remote-calls?module=support-ticket",
              runtime: "/operations/functions?module=support-ticket",
              story: "/?q=support-suite-provider",
              technicalOperations: "/operations?q=support-suite-provider",
            },
            moduleName: "support-ticket",
            name: "List tickets",
            nextAction: "run lenso service check for this operation",
            operationId: "support-ticket/http/GET:/tickets",
            path: "/tickets",
            providerName: "support-suite-provider",
            safeProbe: true,
          },
        ],
        providerName: "support-suite-provider",
        restartPending: false,
        services: [],
        status: "ready",
      },
    ],
    status: "ready",
    version: 1,
  });

  expect(rows[0]?.operations).toMatchObject([
    {
      kind: "http_route",
      operationId: "support-ticket/http/GET:/tickets",
      safeProbe: true,
    },
  ]);
});
```

- [ ] **Step 2: Run model test to verify it fails**

Run:

```sh
pnpm --dir /Users/leosouthey/Projects/framework/lenso-runtime-console exec vitest run src/pages/services-model.test.ts
```

Expected: FAIL because `operations` is not typed or grouped.

- [ ] **Step 3: Add operation response types**

Add to `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`:

```ts
export type ServiceOperationLinks = {
  remoteCalls?: string | null;
  runtime?: string | null;
  story: string;
  technicalOperations: string;
};

export type ServiceOperation = {
  operationId: string;
  providerName?: string | null;
  moduleName: string;
  kind: "http_route" | "runtime_function" | "event_handler" | "admin_action" | string;
  name: string;
  method?: string | null;
  path?: string | null;
  capability?: string | null;
  summary?: string | null;
  safeProbe: boolean;
  links: ServiceOperationLinks;
  nextAction: string;
};
```

Add to `ServiceModuleLifecycleModule`:

```ts
operations?: ServiceOperation[];
```

- [ ] **Step 4: Group operations in service model**

In `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`, import `ServiceOperation` and add to `ServiceCenterRow`:

```ts
operations: ServiceOperation[];
```

Inside `serviceCenterRows`, add:

```ts
operations: modules
  .flatMap((module) => module.operations ?? [])
  .sort((a, b) => a.operationId.localeCompare(b.operationId)),
```

- [ ] **Step 5: Add operation UI panel**

In `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`, add state:

```tsx
const [selectedOperationId, setSelectedOperationId] = useState<string | null>(null);
const selectedOperation =
  selectedRow?.operations.find(
    (operation) => operation.operationId === selectedOperationId
  ) ?? selectedRow?.operations[0];
```

Add this section in `ServiceDetail` after lifecycle:

```tsx
<DetailSection title="operations">
  <div className="grid gap-1">
    {row.operations.length === 0 ? (
      <span className="text-(--fg-tertiary)">-</span>
    ) : (
      row.operations.map((operation) => (
        <button
          className="grid grid-cols-[80px_minmax(0,1fr)_auto] gap-2 border border-(--line) px-1.5 py-1 text-left hover:bg-(--bg-control-hover)"
          key={operation.operationId}
          onClick={() => onSelectOperation(operation.operationId)}
          type="button"
        >
          <span className="text-(--fg-tertiary)">{operation.kind}</span>
          <span className="truncate text-(--fg-primary)">{operation.name}</span>
          <span className={operation.safeProbe ? "text-(--success)" : "text-(--fg-tertiary)"}>
            {operation.safeProbe ? "probe" : "skip"}
          </span>
        </button>
      ))
    )}
  </div>
</DetailSection>
```

Change the `ServiceDetail` props to include:

```tsx
onSelectOperation: (operationId: string) => void;
selectedOperation: ServiceCenterRow["operations"][number] | undefined;
```

Add an operation detail section:

```tsx
<DetailSection title="operation detail">
  {selectedOperation ? (
    <div className="grid gap-1 text-(--fg-secondary)">
      <span>{selectedOperation.operationId}</span>
      <span>{selectedOperation.capability ?? "no capability"}</span>
      <span>{selectedOperation.nextAction}</span>
      <div className="flex flex-wrap gap-1">
        {selectedOperation.links.remoteCalls ? (
          <DetailLink label="calls" to={selectedOperation.links.remoteCalls} />
        ) : null}
        {selectedOperation.links.runtime ? (
          <DetailLink label="runtime" to={selectedOperation.links.runtime} />
        ) : null}
        <DetailLink label="story" to={selectedOperation.links.story} />
        <DetailLink label="ops" to={selectedOperation.links.technicalOperations} />
      </div>
    </div>
  ) : (
    <span className="text-(--fg-tertiary)">No operation selected.</span>
  )}
</DetailSection>
```

- [ ] **Step 6: Run Console checks**

Run:

```sh
pnpm --dir /Users/leosouthey/Projects/framework/lenso-runtime-console exec vitest run src/pages/services-model.test.ts
pnpm --dir /Users/leosouthey/Projects/framework/lenso-runtime-console exec tsc -b --pretty false
```

Expected: PASS.

- [ ] **Step 7: Commit**

```sh
git add src/pages/available-modules-model.ts src/pages/services-model.ts src/pages/services-model.test.ts src/pages/services-page.tsx
git commit -m "feat: show service operations in console"
```

### Task 6: Local Managed Service Logs

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/docs/architecture/service-module-operator-runbook.md`

- [ ] **Step 1: Write failing parser test for logs command**

Add this test to `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`:

```rust
#[test]
fn parses_service_logs() {
    let cli = Cli::parse_from([
        "lenso",
        "service",
        "logs",
        "support-suite-provider",
        "support-suite-provider",
        "--tail",
        "25",
    ]);

    let Command::Service {
        command: ServiceCommand::Logs(args),
    } = cli.command
    else {
        panic!("expected service logs");
    };

    assert_eq!(args.module_name, "support-suite-provider");
    assert_eq!(args.service_name, "support-suite-provider");
    assert_eq!(args.tail, 25);
}
```

- [ ] **Step 2: Run parser test to verify it fails**

Run:

```sh
cargo test --locked --manifest-path /Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml parses_service_logs
```

Expected: FAIL because `ServiceCommand::Logs` is missing.

- [ ] **Step 3: Add logs command**

In `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`, add:

```rust
/// Print local managed service logs.
Logs(ModuleServiceLogsArgs),
```

Add args:

```rust
#[derive(Debug, Args, Clone)]
struct ModuleServiceLogsArgs {
    /// Module or provider name.
    module_name: String,

    /// Service name.
    service_name: String,

    /// Lenso host repository root.
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,

    /// Remote module services file.
    #[arg(long)]
    module_services_file: Option<std::path::PathBuf>,

    /// Number of lines to print.
    #[arg(long, default_value_t = 100)]
    tail: usize,
}
```

Add dispatch:

```rust
ServiceCommand::Logs(args) => {
    module::logs_module_service((&args).into()).await?;
}
```

Add conversion to `module::ModuleServiceLogsOptions`.

- [ ] **Step 4: Capture stdout/stderr to log file**

In `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`, add:

```rust
pub struct ModuleServiceLogsOptions {
    pub module_name: String,
    pub service_name: String,
    pub repo_root: Option<PathBuf>,
    pub module_services_file: Option<PathBuf>,
    pub tail: usize,
}
```

Add helper:

```rust
fn module_service_log_path(
    repo_root: &Path,
    module_name: &str,
    service_name: &str,
) -> PathBuf {
    repo_root
        .join(".lenso/service-logs")
        .join(slugify(module_name))
        .join(format!("{}.log", slugify(service_name)))
}
```

In `start_module_service`, before `spawn()`:

```rust
let log_file_path = module_service_log_path(&repo_root, &module_name, &service.name);
ensure_parent_dir(&log_file_path)?;
let stdout = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&log_file_path)
    .with_context(|| format!("open service log {}", log_file_path.display()))?;
let stderr = stdout.try_clone().context("clone service log handle")?;
```

Use:

```rust
let mut child = shell_command(&service.command)
    .current_dir(cwd)
    .stdout(std::process::Stdio::from(stdout))
    .stderr(std::process::Stdio::from(stderr))
    .spawn()
    .with_context(|| format!("start service {}/{}", module_name, service.name))?;
```

- [ ] **Step 5: Implement tail output**

Add:

```rust
pub async fn logs_module_service(options: ModuleServiceLogsOptions) -> Result<()> {
    let repo_root = resolve_repo_root(options.repo_root.as_deref())?;
    let module_services_path =
        resolve_module_services_file_path(&repo_root, options.module_services_file.as_deref());
    let states = read_remote_module_service_states(&module_services_path)?;
    let (module_name, service) =
        find_module_service(&states, &options.module_name, &options.service_name)?;
    let log_file_path = module_service_log_path(&repo_root, &module_name, &service.name);
    let source = read_text(&log_file_path)
        .with_context(|| format!("read service log {}", log_file_path.display()))?;
    for line in source
        .lines()
        .rev()
        .take(options.tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        println!("{line}");
    }
    Ok(())
}
```

- [ ] **Step 6: Add a small unit test for tailing**

Add helper function:

```rust
fn tail_lines(source: &str, count: usize) -> Vec<&str> {
    source
        .lines()
        .rev()
        .take(count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
```

Add test:

```rust
#[test]
fn tail_lines_keeps_last_n_lines_in_order() {
    assert_eq!(tail_lines("a\nb\nc\n", 2), vec!["b", "c"]);
}
```

- [ ] **Step 7: Update operator runbook**

Add to `/Users/leosouthey/Projects/framework/lenso/docs/architecture/service-module-operator-runbook.md` Fast Path:

```md
lenso service logs <provider> <service> --tail 100
```

Add one sentence:

```md
Logs are available only for local services started by `lenso service start` or
host auto-start; externally managed services keep their logs in their own
deployment platform.
```

- [ ] **Step 8: Run checks**

Run:

```sh
cargo fmt --manifest-path /Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml
cargo test --locked --manifest-path /Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml service
```

Expected: PASS.

- [ ] **Step 9: Commit**

```sh
git add src/main.rs src/module.rs
git commit -m "feat: capture local service logs"
cd /Users/leosouthey/Projects/framework/lenso
git add docs/architecture/service-module-operator-runbook.md
git commit -m "docs: document service logs"
```

### Task 7: V8 Proof Examples

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/module.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/smoke.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/README.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/README.md`

- [ ] **Step 1: Write failing support-ticket smoke assertions**

In `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/smoke.ts`, after fetching the manifest, add:

```ts
const supportTicket = manifest.modules.find(
  (module) => module.name === "support-ticket"
);
if (!supportTicket?.http_routes?.[0]?.operation?.operationId) {
  throw new Error("support-ticket route is missing V8 operation metadata");
}
if (!supportTicket?.runtime?.functions?.[0]?.operation?.safeProbe) {
  throw new Error("support-ticket runtime function is missing safe probe metadata");
}
const assignTicket = supportTicket?.admin?.actions?.find(
  (action) => action.name === "assign_ticket"
);
if (!assignTicket?.operation?.operationId) {
  throw new Error("assign_ticket action is missing operation metadata");
}
```

- [ ] **Step 2: Run smoke to verify it fails**

Run:

```sh
pnpm --dir /Users/leosouthey/Projects/framework/lenso-examples --filter @lenso/example-support-ticket smoke
```

Expected: FAIL until operation metadata is added.

- [ ] **Step 3: Add TypeScript operation metadata**

In `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/module.ts`, update declarations:

```ts
adminAction("assign_ticket", {
  capability: writeCapability,
  inputFields: [
    actionTextField("ticket_id", {
      label: "Ticket ID",
      required: true,
    }),
    actionTextField("assignee", { label: "Assignee", required: true }),
    actionTimestampField("updated_at", { label: "Updated At" }),
  ],
  label: "Assign ticket",
  operation: {
    operationId: "support-ticket/action/assign_ticket",
    safeProbe: {
      input: { assignee: "support-lead", dry_run: true, ticket_id: "ticket_1" },
      expectStatus: 200,
    },
    summary: "Assign support ticket",
  },
})
```

Update route declarations:

```ts
getRoute("/tickets/{id}", {
  capability: readCapability,
  displayName: "Get ticket",
  operation: {
    operationId: "support-ticket/http/GET:/tickets/{id}",
    summary: "Get ticket",
  },
  storyTitle: "Support ticket viewed",
})
postRoute("/tickets", {
  capability: writeCapability,
  displayName: "Create ticket",
  operation: {
    operationId: "support-ticket/http/POST:/tickets",
    safeProbe: {
      input: { dry_run: true, title: "Probe support ticket" },
      expectStatus: 200,
    },
    summary: "Create ticket",
  },
  storyTitle: "Support ticket created",
})
```

Update runtime functions:

```ts
runtimeFunction("support-ticket.escalate-ticket.v1", {
  operation: {
    idempotency: "requires_key",
    operationId: "support-ticket/runtime/support-ticket.escalate-ticket.v1",
    safeProbe: {
      input: { dry_run: true, ticket_id: "ticket_1" },
      expectStatus: 200,
    },
    summary: "Escalate ticket",
  },
  queue: "support-ticket",
})
```

- [ ] **Step 4: Add Rust runtime proof**

In `/Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/src/main.rs`, import runtime types:

```rust
use lenso::{RuntimeFunctionDeclaration, RuntimeSurface, ServiceOperationMetadata, ServiceOperationSafeProbe};
```

Update `audit_log_module()`:

```rust
.runtime(RuntimeSurface {
    functions: vec![RuntimeFunctionDeclaration {
        name: "rust-audit-log.summarize-events.v1".to_owned(),
        version: 1,
        queue: "rust-audit-log".to_owned(),
        input_schema: None,
        retry_policy: None,
        operation: Some(ServiceOperationMetadata {
            operation_id: Some("rust-audit-log/runtime/rust-audit-log.summarize-events.v1".to_owned()),
            summary: Some("Summarize audit events".to_owned()),
            safe_probe: Some(ServiceOperationSafeProbe {
                method: None,
                path: None,
                input: Some(json!({ "dry_run": true })),
                expect_status: Some(200),
            }),
            ..ServiceOperationMetadata::default()
        }),
    }],
})
```

Add route handler for service-kit compatible runtime invocation:

```rust
.route(
    "/lenso/service/v1/modules/rust-audit-log/runtime/functions/rust-audit-log.summarize-events.v1/invoke",
    axum::routing::post(summarize_events),
)
```

Add handler:

```rust
async fn summarize_events() -> Json<Value> {
    Json(json!({
        "output": {
            "count": 2,
            "summary": "2 audit events available"
        }
    }))
}
```

Update Rust tests to assert runtime operation metadata exists:

```rust
assert_eq!(
    manifest["modules"][0]["runtime"]["functions"][0]["operation"]["operationId"],
    "rust-audit-log/runtime/rust-audit-log.summarize-events.v1"
);
```

- [ ] **Step 5: Update docs**

Add to `/Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/README.md`:

```md
The Rust example also exposes `rust-audit-log.summarize-events.v1` as a runtime
operation so V8 can prove non-HTTP operation metadata from Rust.
```

Add to `/Users/leosouthey/Projects/framework/lenso-examples/README.md`:

```md
The V8 proof path uses `lenso service check --serve-command` to report checked,
skipped, and failed service operations for both TypeScript and Rust services.
```

- [ ] **Step 6: Run example checks**

Run:

```sh
cargo fmt --manifest-path /Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/Cargo.toml
cargo test --manifest-path /Users/leosouthey/Projects/framework/lenso-examples/examples/rust-service/Cargo.toml
pnpm --dir /Users/leosouthey/Projects/framework/lenso-examples --filter @lenso/example-support-ticket smoke
```

Expected: PASS. If `pnpm` cannot install unpublished local packages, run the support-ticket smoke after `pnpm install` in the examples repo with the existing workspace overrides.

- [ ] **Step 7: Commit**

```sh
git add examples/support-ticket/src/module.ts examples/support-ticket/src/smoke.ts examples/rust-service/src/main.rs examples/rust-service/README.md README.md
git commit -m "feat: add v8 service operation proofs"
```

## Final Verification

- [ ] **Step 1: Run backend contract/API checks**

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test --locked -p lenso-contracts operation_metadata_serializes_on_route_runtime_and_action
cargo test --locked -p lenso-api service_modules_exposes_operations_for_provider_modules
```

Expected: PASS.

- [ ] **Step 2: Run CLI checks**

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test --locked --manifest-path Cargo.toml service
```

Expected: PASS.

- [ ] **Step 3: Run Console checks**

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run src/pages/services-model.test.ts packages/remote-module-kit/src/index.test.ts packages/service-kit/src/index.test.ts
pnpm exec tsc -b --pretty false
```

Expected: PASS.

- [ ] **Step 4: Run examples checks**

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
cargo test --manifest-path examples/rust-service/Cargo.toml
```

Expected: PASS.

- [ ] **Step 5: Check repository status**

```sh
git -C /Users/leosouthey/Projects/framework/lenso status --short
git -C /Users/leosouthey/Projects/framework/lenso-cli status --short
git -C /Users/leosouthey/Projects/framework/lenso-runtime-console status --short
git -C /Users/leosouthey/Projects/framework/lenso-examples status --short
```

Expected: each repo prints nothing.

## Self-Review Notes

- Spec coverage: Task 1 covers operation metadata; Task 2 covers host operation catalog; Task 3 covers TypeScript SDK and context reader; Task 4 covers operation-aware check; Task 5 covers Console detail; Task 6 covers local logs; Task 7 covers TypeScript/Rust proof.
- Deliberate simplification: operation metadata is an `operation` object on each declaration. This avoids colliding with existing `input_schema`, admin `input_schema`, and route display fields.
- Deferred by design: no database-backed operation registry, no log search, no schema registry, no gateway, no service discovery.
