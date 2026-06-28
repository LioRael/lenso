# Lenso Service Product Layer V6 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Lenso services a first-class product surface across contract, SDKs, CLI, Console, catalog, and support-suite examples.

**Architecture:** Keep Host as the control plane and keep the existing remote protocol as the transport base. Add one shared service contract consumed by Rust and TypeScript SDKs, CLI service workflows, Runtime Console service views, catalog provider mapping, and examples. Service remains an independently running provider process; module remains the business capability provided inside a service or linked into the host.

**Tech Stack:** Rust 2024, Axum, clap, serde/serde_json, reqwest, TypeScript, React, TanStack Router/Query, Vitest, pnpm, existing `@lenso/service-kit` and `@lenso/remote-module-kit`.

**Estimated Work:** 14-18 hours. Use focused tests per task and one final support-suite host path check.

---

## Verification Policy

Use cheap checks while developing:

```sh
cargo test -p lenso-platform-module-remote remote_source
cargo test -p lenso-platform-admin-data
cargo test -p lenso-service
cd /Users/leosouthey/Projects/framework/lenso-cli && cargo test
cd /Users/leosouthey/Projects/framework/lenso-runtime-console && pnpm --filter @lenso/service-kit build && pnpm test:local
```

Run one end-to-end support-suite check near the end:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm --filter @lenso/example-support-ticket smoke
pnpm host-api-smoke:support-ticket
```

Do not add broad smoke loops unless the changed code crosses Host, service, and
Console in one path.

## File Map

### `/Users/leosouthey/Projects/framework/lenso`

- Modify `Cargo.toml`: add the new `crates/lenso-service` workspace member and workspace dependency.
- Create `crates/lenso-service/Cargo.toml`: public Rust SDK crate for service authors.
- Create `crates/lenso-service/src/lib.rs`: export manifest builders, health routes, and handler registration helpers.
- Create `crates/lenso-service/tests/contract.rs`: verify generated service manifest shape.
- Modify `crates/platform-module-remote/src/protocol.rs`: extend service manifest DTOs without breaking old service envelopes.
- Modify `crates/platform-module-remote/src/source.rs`: preserve service manifest parsing and expose provider metadata.
- Modify `crates/platform-module-remote/tests/remote_source.rs`: cover V6 service contract fields.
- Modify `crates/platform-admin-data/src/dto.rs`: add service provider/catalog fields and service-center DTOs.
- Modify `crates/platform-admin-data/src/handlers.rs`: include provider mapping and service status in admin data.
- Modify `crates/platform-admin-data/catalogs/lenso-official-module-catalog.json`: add support-suite provider mapping.
- Modify `crates/lenso-api/tests/admin_data_console.rs`: cover service provider mapping in the admin-data response.

### `/Users/leosouthey/Projects/framework/lenso-runtime-console`

- Modify `packages/service-kit/src/index.ts`: add V6 service contract helpers while keeping remote-module-kit exports.
- Create `packages/service-kit/src/index.test.ts`: verify service builder output and safe defaults.
- Modify `packages/service-kit/README.md`: document the TS service authoring path.
- Modify `scripts/package-readiness.mjs`: keep `@lenso/service-kit` in package readiness.
- Create `src/pages/services-model.ts`: service-center state mapping and links.
- Create `src/pages/services-model.test.ts`: model tests for states, provider-module graph, and call links.
- Create `src/pages/services-page.tsx`: Service Center UI.
- Modify `src/app/router.tsx`: add `/services`.
- Modify `src/components/runtime/runtime-console-shell.tsx`: add Services navigation entry.
- Modify `src/data/available-modules.ts`: surface provider metadata already returned by Host.
- Modify `src/pages/available-modules-model.ts`: map catalog provider fields onto module rows.
- Modify `src/pages/available-modules-model.test.ts`: cover provider-backed module rows.

### `/Users/leosouthey/Projects/framework/lenso-cli`

- Modify `src/main.rs`: add `service create`, `service dev`, and stronger `service check` arguments.
- Create `src/service.rs`: service-oriented command implementation.
- Modify `src/module.rs`: keep legacy install/uninstall internals reusable from `src/service.rs`.
- Create `templates/service-ts/package.json.tmpl`.
- Create `templates/service-ts/src/server.ts`.
- Create `templates/service-ts/src/service.ts`.
- Create `templates/service-ts/lenso.service.json.tmpl`.
- Create `templates/service-rust/Cargo.toml.tmpl`.
- Create `templates/service-rust/src/main.rs`.
- Create `templates/service-rust/lenso.service.json.tmpl`.
- Modify `README.md`: document `service create`, `service dev`, and `service check`.

### `/Users/leosouthey/Projects/framework/lenso-examples`

- Modify `examples/support-ticket/src/module.ts`: make the service provide `support-ticket`, `support-notification`, and `support-knowledge-base`.
- Modify `examples/support-ticket/src/smoke.ts`: assert all three modules exist in the service manifest.
- Modify `examples/support-ticket/catalog-entry.json`: use provider-backed module catalog shape.
- Modify `examples/support-ticket/README.md`: document the support-suite provider workflow.
- Modify `docs/support-ticket-service-module-run.md`: update the V6 runbook.
- Modify `scripts/support-ticket-host-api-smoke.ts`: assert provider-backed install state and one service operation link.

## Task 1: Service Contract V1 In Host Protocol

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-module-remote/src/protocol.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-module-remote/src/source.rs`
- Test: `/Users/leosouthey/Projects/framework/lenso/crates/platform-module-remote/tests/remote_source.rs`

- [ ] **Step 1: Write the failing V6 envelope test**

Add a test that parses old and new service envelopes:

```rust
#[test]
fn service_manifest_accepts_v6_provider_fields() {
    let value = serde_json::json!({
        "name": "support-suite-provider",
        "version": "0.2.0",
        "provider": {
            "name": "support-suite-provider",
            "vendor": "Lenso",
            "summary": "Support workflow provider"
        },
        "compatibility": {
            "remoteProtocolVersion": "1",
            "requiredHostFeatures": ["service.status"]
        },
        "health": {
            "readyUrl": "http://127.0.0.1:4110/lenso/service/v1/ready",
            "statusUrl": "http://127.0.0.1:4110/lenso/service/v1/status"
        },
        "localProcess": {
            "command": "pnpm --dir examples/support-ticket start",
            "autoStart": true,
            "readyTimeoutMs": 30000
        },
        "modules": [
            {
                "name": "support-ticket",
                "version": "0.1.0",
                "capabilities": ["support_ticket.tickets.read"]
            }
        ]
    });

    let envelope: RemoteManifestEnvelope = serde_json::from_value(value).unwrap();
    let RemoteManifestEnvelope::Service(service) = envelope else {
        panic!("expected service envelope");
    };

    assert_eq!(service.name, "support-suite-provider");
    assert_eq!(service.version.as_deref(), Some("0.2.0"));
    assert_eq!(service.provider.as_ref().unwrap().vendor.as_deref(), Some("Lenso"));
    assert_eq!(service.health.as_ref().unwrap().ready_url.as_deref(), Some("http://127.0.0.1:4110/lenso/service/v1/ready"));
    assert_eq!(service.modules[0].name, "support-ticket");
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-platform-module-remote service_manifest_accepts_v6_provider_fields
```

Expected: fail because `RemoteServiceManifestResponse` does not expose V6 fields.

- [ ] **Step 3: Add the V6 DTOs**

In `protocol.rs`, replace the service manifest struct with optional fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceManifestResponse {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub provider: Option<RemoteServiceProviderMetadata>,
    #[serde(default)]
    pub compatibility: Option<RemoteServiceCompatibility>,
    #[serde(default)]
    pub config: Vec<RemoteServiceConfigField>,
    #[serde(default)]
    pub env: Vec<RemoteServiceEnvField>,
    #[serde(default)]
    pub health: Option<RemoteServiceHealth>,
    #[serde(default)]
    pub local_process: Option<RemoteServiceLocalProcess>,
    pub modules: Vec<ModuleManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceProviderMetadata {
    pub name: String,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceCompatibility {
    #[serde(default)]
    pub remote_protocol_version: Option<String>,
    #[serde(default)]
    pub required_host_features: Vec<String>,
    #[serde(default)]
    pub sdk_language: Option<String>,
    #[serde(default)]
    pub sdk_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceConfigField {
    pub key: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceEnvField {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceHealth {
    #[serde(default)]
    pub manifest_url: Option<String>,
    #[serde(default)]
    pub ready_url: Option<String>,
    #[serde(default)]
    pub liveness_url: Option<String>,
    #[serde(default)]
    pub status_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServiceLocalProcess {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_service_auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_service_ready_timeout_ms")]
    pub ready_timeout_ms: u64,
}

fn default_service_auto_start() -> bool {
    true
}

fn default_service_ready_timeout_ms() -> u64 {
    30_000
}
```

- [ ] **Step 4: Preserve old service envelopes**

Add a test with only `name` and `modules`:

```rust
#[test]
fn service_manifest_accepts_v5_shape() {
    let value = serde_json::json!({
        "name": "support-service",
        "modules": [{ "name": "support-ticket", "version": "0.1.0" }]
    });

    let envelope: RemoteManifestEnvelope = serde_json::from_value(value).unwrap();
    let RemoteManifestEnvelope::Service(service) = envelope else {
        panic!("expected service envelope");
    };

    assert_eq!(service.name, "support-service");
    assert!(service.provider.is_none());
    assert_eq!(service.modules.len(), 1);
}
```

- [ ] **Step 5: Run the focused tests**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-platform-module-remote service_manifest_accepts
```

Expected: both service manifest tests pass.

- [ ] **Step 6: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso
git add crates/platform-module-remote/src/protocol.rs crates/platform-module-remote/src/source.rs crates/platform-module-remote/tests/remote_source.rs
git commit -m "feat: add service contract v1 fields"
```

## Task 2: TypeScript Service Kit V6 Helpers

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/src/index.ts`
- Create: `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/src/index.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/README.md`

- [ ] **Step 1: Write the failing builder test**

Create `packages/service-kit/src/index.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { defineServiceContract, serviceEnv, serviceHealth } from "./index";

describe("defineServiceContract", () => {
  it("builds a provider service manifest with modules", () => {
    const manifest = defineServiceContract({
      name: "support-suite-provider",
      version: "0.2.0",
      provider: {
        name: "support-suite-provider",
        vendor: "Lenso",
        summary: "Support workflow provider",
      },
      compatibility: {
        remoteProtocolVersion: "1",
        requiredHostFeatures: ["service.status"],
        sdkLanguage: "ts",
        sdkVersion: "0.1.0",
      },
      env: [serviceEnv("PORT", { example: "4110", required: true })],
      health: serviceHealth({
        readyUrl: "http://127.0.0.1:4110/lenso/service/v1/ready",
        statusUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
      }),
      modules: [
        {
          name: "support-ticket",
          version: "0.1.0",
          capabilities: ["support_ticket.tickets.read"],
        },
      ],
    });

    expect(manifest).toMatchObject({
      name: "support-suite-provider",
      provider: { vendor: "Lenso" },
      env: [{ name: "PORT", required: true, example: "4110" }],
      health: {
        readyUrl: "http://127.0.0.1:4110/lenso/service/v1/ready",
      },
      modules: [{ name: "support-ticket" }],
    });
  });
});
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run packages/service-kit/src/index.test.ts
```

Expected: fail because the named helpers do not exist.

- [ ] **Step 3: Add the thin helper API**

In `packages/service-kit/src/index.ts`, keep the existing export and add:

```ts
export * from "@lenso/remote-module-kit";

export type ServiceContract = {
  name: string;
  version?: string;
  provider?: ServiceProviderMetadata;
  compatibility?: ServiceCompatibility;
  config?: ServiceConfigField[];
  env?: ServiceEnvField[];
  health?: ServiceHealth;
  localProcess?: ServiceLocalProcess;
  modules: ServiceModuleContract[];
};

export type ServiceProviderMetadata = {
  name: string;
  vendor?: string;
  summary?: string;
  homepage?: string;
};

export type ServiceCompatibility = {
  remoteProtocolVersion?: string;
  requiredHostFeatures?: string[];
  sdkLanguage?: "ts" | "rust" | string;
  sdkVersion?: string;
};

export type ServiceConfigField = {
  key: string;
  required?: boolean;
  defaultValue?: unknown;
  secret?: boolean;
};

export type ServiceEnvField = {
  name: string;
  required?: boolean;
  example?: string;
};

export type ServiceHealth = {
  manifestUrl?: string;
  readyUrl?: string;
  livenessUrl?: string;
  statusUrl?: string;
};

export type ServiceLocalProcess = {
  command: string;
  cwd?: string;
  env?: Record<string, string>;
  autoStart?: boolean;
  readyTimeoutMs?: number;
};

export type ServiceModuleContract = {
  name: string;
  version?: string;
  capabilities?: string[];
  dependencies?: string[];
};

export function defineServiceContract(contract: ServiceContract): ServiceContract {
  return {
    ...contract,
    config: contract.config ?? [],
    env: contract.env ?? [],
    modules: contract.modules,
  };
}

export function serviceEnv(
  name: string,
  options: Omit<ServiceEnvField, "name"> = {}
): ServiceEnvField {
  return { name, ...options };
}

export function serviceHealth(health: ServiceHealth): ServiceHealth {
  return health;
}
```

- [ ] **Step 4: Document the minimal TS author path**

Add this example to `packages/service-kit/README.md`:

```ts
import {
  defineModule,
  defineService,
  defineServiceContract,
  serviceEnv,
  serveService,
} from "@lenso/service-kit";

const supportTicket = defineModule({
  name: "support-ticket",
  version: "0.1.0",
  capabilities: ["support_ticket.tickets.read"],
});

export const contract = defineServiceContract({
  name: "support-suite-provider",
  version: "0.2.0",
  env: [serviceEnv("PORT", { example: "4110", required: true })],
  modules: [{ name: supportTicket.name, version: supportTicket.version }],
});

export const manifest = defineService({
  name: contract.name,
  version: contract.version,
  modules: [supportTicket],
});

serveService(manifest, { modules: {} });
```

- [ ] **Step 5: Run focused TS checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run packages/service-kit/src/index.test.ts
pnpm --filter @lenso/service-kit build
```

Expected: tests and build pass.

- [ ] **Step 6: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
git add packages/service-kit/src/index.ts packages/service-kit/src/index.test.ts packages/service-kit/README.md
git commit -m "feat: add service kit contract helpers"
```

## Task 3: Rust Service SDK Crate

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/Cargo.toml`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-service/Cargo.toml`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-service/src/lib.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-service/tests/contract.rs`

- [ ] **Step 1: Add the crate to the workspace**

In `/Users/leosouthey/Projects/framework/lenso/Cargo.toml`, add:

```toml
members = [
    "fixtures/remote-module",
    "crates/lenso-contracts",
    "crates/lenso",
    "crates/lenso-api",
    "crates/lenso-migrate",
    "crates/lenso-worker",
    "crates/lenso-bootstrap",
    "crates/lenso-service",
    "crates/platform-admin",
    "crates/platform-admin-data",
    "crates/platform-core",
    "crates/platform-module",
    "crates/platform-module-remote",
    "crates/platform-http",
    "crates/platform-runtime",
    "crates/platform-testing",
    "modules/story",
    "tools/generate-contracts",
    "tools/arch-check",
    "tools/otel-smoke",
]

[workspace.dependencies]
lenso-service = { path = "crates/lenso-service", version = "0.1.0" }
```

Keep the existing entries in the same relative order where possible.

- [ ] **Step 2: Create the crate manifest**

Create `crates/lenso-service/Cargo.toml`:

```toml
[package]
name = "lenso-service"
version = "0.1.0"
edition.workspace = true
license = "MIT"
description = "Rust helpers for building Lenso service providers."
repository = "https://github.com/LioRael/lenso"
homepage = "https://github.com/LioRael/lenso"
categories = ["web-programming", "development-tools"]
keywords = ["backend", "framework", "services"]
rust-version.workspace = true

[dependencies]
axum.workspace = true
platform-module.workspace = true
platform-module-remote.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true

[lints]
workspace = true
```

- [ ] **Step 3: Write the failing Rust SDK test**

Create `crates/lenso-service/tests/contract.rs`:

```rust
use lenso_service::{ServiceContract, ServiceHealth, ServiceProvider};
use platform_module::ModuleManifest;

#[test]
fn service_contract_serializes_provider_and_modules() {
    let contract = ServiceContract::new(
        "support-suite-provider",
        vec![
            ModuleManifest::builder("support-ticket")
                .capabilities(vec!["support_ticket.tickets.read".to_owned()])
                .build(),
        ],
    )
    .version("0.2.0")
    .provider(ServiceProvider {
        name: "support-suite-provider".to_owned(),
        vendor: Some("Lenso".to_owned()),
        summary: Some("Support workflow provider".to_owned()),
        homepage: None,
    })
    .health(ServiceHealth {
        ready_url: Some("http://127.0.0.1:4110/lenso/service/v1/ready".to_owned()),
        status_url: Some("http://127.0.0.1:4110/lenso/service/v1/status".to_owned()),
        ..ServiceHealth::default()
    });

    let value = serde_json::to_value(contract).unwrap();

    assert_eq!(value["name"], "support-suite-provider");
    assert_eq!(value["provider"]["vendor"], "Lenso");
    assert_eq!(value["modules"][0]["name"], "support-ticket");
}
```

- [ ] **Step 4: Implement the minimal SDK**

Create `crates/lenso-service/src/lib.rs`:

```rust
use axum::{Json, Router, routing::get};
use platform_module::ModuleManifest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProvider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceContract {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ServiceProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<ServiceHealth>,
    pub modules: Vec<ModuleManifest>,
}

impl ServiceContract {
    #[must_use]
    pub fn new(name: impl Into<String>, modules: Vec<ModuleManifest>) -> Self {
        Self {
            name: name.into(),
            version: None,
            provider: None,
            health: None,
            modules,
        }
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn provider(mut self, provider: ServiceProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    #[must_use]
    pub fn health(mut self, health: ServiceHealth) -> Self {
        self.health = Some(health);
        self
    }
}

#[must_use]
pub fn health_router() -> Router {
    Router::new()
        .route("/lenso/service/v1/ready", get(|| async { Json(serde_json::json!({"ready": true})) }))
        .route("/lenso/service/v1/status", get(|| async { Json(serde_json::json!({"state": "ready"})) }))
}
```

- [ ] **Step 5: Run the crate tests**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-service
```

Expected: Rust SDK contract test passes.

- [ ] **Step 6: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso
git add Cargo.toml crates/lenso-service
git commit -m "feat: add rust service sdk crate"
```

## Task 4: CLI Service Create, Dev, And Check

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/src/service.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-ts/package.json.tmpl`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-ts/src/server.ts`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-ts/src/service.ts`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-ts/lenso.service.json.tmpl`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-rust/Cargo.toml.tmpl`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-rust/src/main.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso-cli/templates/service-rust/lenso.service.json.tmpl`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/README.md`

- [ ] **Step 1: Add CLI argument tests**

Add unit tests near the CLI parse tests in `src/main.rs`:

```rust
#[test]
fn parses_service_create_ts() {
    let cli = Cli::parse_from([
        "lenso",
        "service",
        "create",
        "support-suite-provider",
        "--lang",
        "ts",
    ]);

    let Command::Service { command: ServiceCommand::Create(args) } = cli.command else {
        panic!("expected service create");
    };

    assert_eq!(args.name, "support-suite-provider");
    assert_eq!(args.lang, ServiceLanguage::Ts);
}

#[test]
fn parses_service_dev() {
    let cli = Cli::parse_from(["lenso", "service", "dev", "--skip-db"]);
    let Command::Service { command: ServiceCommand::Dev(args) } = cli.command else {
        panic!("expected service dev");
    };

    assert!(args.skip_db);
}
```

- [ ] **Step 2: Add command shapes**

In `src/main.rs`:

```rust
mod service;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ServiceLanguage {
    Rust,
    Ts,
}

#[derive(Debug, Args, Clone)]
struct ServiceCreateArgs {
    name: String,
    #[arg(long, value_enum)]
    lang: ServiceLanguage,
    #[arg(long)]
    output_dir: Option<std::path::PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args, Clone)]
struct ServiceDevArgs {
    #[arg(long)]
    repo_root: Option<std::path::PathBuf>,
    #[arg(long)]
    module_services_file: Option<std::path::PathBuf>,
    #[arg(long)]
    skip_db: bool,
    #[arg(long)]
    skip_migrate: bool,
    #[arg(long)]
    separate_worker: bool,
}
```

Add variants:

```rust
Create(ServiceCreateArgs),
Dev(ServiceDevArgs),
```

Map them:

```rust
ServiceCommand::Create(args) => {
    service::create_service((&args).into())?;
}
ServiceCommand::Dev(args) => {
    service::dev_service((&args).into()).await?;
}
```

- [ ] **Step 3: Implement service command options**

Create `src/service.rs`:

```rust
use std::path::PathBuf;

use anyhow::Result;

use crate::{ServiceCreateArgs, ServiceDevArgs, ServiceLanguage, host, module};

#[derive(Debug, Clone)]
pub struct ServiceCreateOptions {
    pub dry_run: bool,
    pub lang: ServiceLanguage,
    pub name: String,
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ServiceDevOptions {
    pub module_services_file: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
    pub separate_worker: bool,
    pub skip_db: bool,
    pub skip_migrate: bool,
}

impl From<&ServiceCreateArgs> for ServiceCreateOptions {
    fn from(args: &ServiceCreateArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            lang: args.lang,
            name: args.name.clone(),
            output_dir: args.output_dir.clone(),
        }
    }
}

impl From<&ServiceDevArgs> for ServiceDevOptions {
    fn from(args: &ServiceDevArgs) -> Self {
        Self {
            module_services_file: args.module_services_file.clone(),
            repo_root: args.repo_root.clone(),
            separate_worker: args.separate_worker,
            skip_db: args.skip_db,
            skip_migrate: args.skip_migrate,
        }
    }
}

pub fn create_service(options: ServiceCreateOptions) -> Result<()> {
    match options.lang {
        ServiceLanguage::Rust => create_rust_service(options),
        ServiceLanguage::Ts => create_ts_service(options),
    }
}

pub async fn dev_service(options: ServiceDevOptions) -> Result<()> {
    module::start_declared_module_services(
        options.repo_root.as_deref(),
        options.module_services_file.as_deref(),
    )
    .await?;
    host::serve(
        options.repo_root.as_deref(),
        options.skip_db,
        options.skip_migrate,
        options.separate_worker,
    )
    .await
}
```

- [ ] **Step 4: Expose a reusable service starter**

In `src/module.rs`, add:

```rust
pub async fn start_declared_module_services(
    repo_root: Option<&Path>,
    module_services_file: Option<&Path>,
) -> Result<()> {
    let repo_root = repo_root.unwrap_or_else(|| Path::new("."));
    let module_services_path =
        resolve_module_services_file_path(repo_root, module_services_file);
    let states = read_remote_module_service_states(&module_services_path)?;
    for state in states {
        for service in state.services {
            if service.auto_start {
                start_module_service(ModuleServiceStartOptions {
                    module_name: state.module_name.clone(),
                    service_name: service.name.clone(),
                    module_services_file: Some(module_services_path.clone()),
                    repo_root: Some(repo_root.to_path_buf()),
                })
                .await?;
            }
        }
    }
    Ok(())
}
```

If this repeats existing `start_module_service` code too much, extract the shared
private helper first and call it from both places.

- [ ] **Step 5: Add templates**

TS `templates/service-ts/package.json.tmpl`:

```json
{
  "name": "{{package_name}}",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "check": "node src/server.ts --check",
    "dev": "node src/server.ts",
    "start": "node src/server.ts"
  },
  "dependencies": {
    "@lenso/service-kit": "0.1.0"
  }
}
```

Rust `templates/service-rust/Cargo.toml.tmpl`:

```toml
[package]
name = "{{crate_name}}"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
lenso-service = "0.1.0"
platform-module = "0.1.4"
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Service JSON template:

```json
{
  "name": "{{service_name}}",
  "version": "0.1.0",
  "provider": {
    "name": "{{service_name}}",
    "summary": "{{service_label}} provider"
  },
  "compatibility": {
    "remoteProtocolVersion": "1",
    "requiredHostFeatures": ["service.status"]
  },
  "modules": [
    {
      "name": "{{module_name}}",
      "version": "0.1.0",
      "capabilities": ["{{module_name}}.read"]
    }
  ]
}
```

- [ ] **Step 6: Run CLI tests**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test
```

Expected: parse tests and existing tests pass.

- [ ] **Step 7: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
git add src/main.rs src/service.rs src/module.rs templates/service-ts templates/service-rust README.md
git commit -m "feat: add service create and dev workflow"
```

## Task 5: Runtime Console Service Center

**Files:**
- Create: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`
- Create: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`
- Create: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/app/router.tsx`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/components/runtime/runtime-console-shell.tsx`

- [ ] **Step 1: Write service-center model tests**

Create `src/pages/services-model.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  serviceCenterRows,
  serviceRemoteCallsPath,
  serviceStateLabel,
} from "./services-model";

describe("service center model", () => {
  it("groups provider services with provided modules", () => {
    const rows = serviceCenterRows({
      modules: [
        {
          moduleName: "support-ticket",
          providerName: "support-suite-provider",
          status: "ready",
          services: [{ name: "support-service", ready: true }],
        },
        {
          moduleName: "support-notification",
          providerName: "support-suite-provider",
          status: "ready",
          services: [{ name: "support-service", ready: true }],
        },
      ],
    });

    expect(rows).toEqual([
      {
        providerName: "support-suite-provider",
        state: "ready",
        modules: ["support-notification", "support-ticket"],
        managedServices: ["support-service"],
      },
    ]);
  });

  it("labels unhealthy services", () => {
    expect(serviceStateLabel("unhealthy")).toBe("unhealthy");
  });

  it("links to remote calls for a provider module", () => {
    expect(serviceRemoteCallsPath("support-ticket")).toContain("module=support-ticket");
  });
});
```

- [ ] **Step 2: Implement the model**

Create `src/pages/services-model.ts`:

```ts
import { remoteProxyCallsPath } from "./remote-proxy-calls-model";

export type ServiceCenterModule = {
  moduleName: string;
  providerName?: string | null;
  status: string;
  services?: Array<{ name: string; ready?: boolean }>;
};

export type ServiceCenterResponse = {
  modules: ServiceCenterModule[];
};

export type ServiceCenterRow = {
  providerName: string;
  state: string;
  modules: string[];
  managedServices: string[];
};

export function serviceCenterRows(response: ServiceCenterResponse): ServiceCenterRow[] {
  const groups = new Map<string, ServiceCenterModule[]>();
  for (const module of response.modules) {
    const provider = module.providerName ?? module.moduleName;
    groups.set(provider, [...(groups.get(provider) ?? []), module]);
  }

  return Array.from(groups.entries())
    .map(([providerName, modules]) => ({
      providerName,
      state: providerState(modules),
      modules: modules.map((module) => module.moduleName).sort(),
      managedServices: Array.from(
        new Set(modules.flatMap((module) => module.services?.map((service) => service.name) ?? []))
      ).sort(),
    }))
    .sort((a, b) => a.providerName.localeCompare(b.providerName));
}

export function serviceStateLabel(state: string) {
  return state;
}

export function serviceRemoteCallsPath(moduleName: string) {
  return remoteProxyCallsPath({ moduleName });
}

function providerState(modules: ServiceCenterModule[]) {
  if (modules.some((module) => module.status === "unhealthy")) {
    return "unhealthy";
  }
  if (modules.some((module) => module.status === "restart_pending")) {
    return "restart pending";
  }
  if (modules.every((module) => module.status === "ready")) {
    return "ready";
  }
  return "configured";
}
```

- [ ] **Step 3: Add the Service Center page**

Create `src/pages/services-page.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Network } from "lucide-react";

import { httpClient } from "../lib/http-client";
import {
  type ServiceCenterResponse,
  serviceCenterRows,
  serviceRemoteCallsPath,
  serviceStateLabel,
} from "./services-model";

export function ServicesPage() {
  const query = useQuery({
    queryKey: ["services", "center"],
    queryFn: () =>
      httpClient.get("admin/data/service-modules").json<ServiceCenterResponse>(),
  });

  const rows = serviceCenterRows(query.data ?? { modules: [] });

  return (
    <section className="page-section">
      <header className="page-header">
        <Network aria-hidden="true" />
        <div>
          <h1>Services</h1>
          <p>Provider processes and the modules they expose.</p>
        </div>
      </header>
      <div className="table-surface">
        {rows.map((row) => (
          <article className="table-row" key={row.providerName}>
            <div>
              <strong>{row.providerName}</strong>
              <span>{serviceStateLabel(row.state)}</span>
            </div>
            <div>{row.modules.join(", ")}</div>
            <div>{row.managedServices.join(", ") || "external"}</div>
            <Link
              className="inline-flex min-h-7 items-center justify-center rounded-[var(--radius-control)] border border-(--line) bg-(--bg-control) px-2.5 text-xs font-medium text-(--fg-primary)"
              to={serviceRemoteCallsPath(row.modules[0] ?? row.providerName)}
            >
              Remote calls
            </Link>
          </article>
        ))}
      </div>
    </section>
  );
}
```

- [ ] **Step 4: Wire the route and nav**

In `src/app/router.tsx`:

```tsx
import { ServicesPage } from "../pages/services-page";

const servicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services",
  component: ServicesPage,
});
```

Add `servicesRoute` to `routeTree`.

In `runtime-console-shell.tsx`, add one nav item:

```tsx
{
  icon: Network,
  label: "Services",
  path: "/services",
}
```

- [ ] **Step 5: Run Console checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run src/pages/services-model.test.ts src/app/router.test.ts
pnpm typecheck:local
```

Expected: model tests and typecheck pass.

- [ ] **Step 6: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
git add src/pages/services-model.ts src/pages/services-model.test.ts src/pages/services-page.tsx src/app/router.tsx src/components/runtime/runtime-console-shell.tsx
git commit -m "feat: add service center page"
```

## Task 6: Catalog Provider Mapping

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/catalogs/lenso-official-module-catalog.json`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.test.ts`

- [ ] **Step 1: Add catalog DTO fields**

In `dto.rs`, add optional fields to the catalog module DTO:

```rust
#[serde(default, rename = "providedBy")]
pub provided_by: Option<String>,
#[serde(default, rename = "serviceManifest")]
pub service_manifest: Option<String>,
```

Keep existing module catalog fields unchanged.

- [ ] **Step 2: Add official support-suite catalog entry**

In `lenso-official-module-catalog.json`, shape the support ticket entry as:

```json
{
  "name": "support-ticket",
  "version": "0.1.0",
  "source": "service",
  "providedBy": "support-suite-provider",
  "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest",
  "summary": "Ticket intake, triage, and operations",
  "capabilities": [
    "support_ticket.tickets.read",
    "support_ticket.tickets.write",
    "support_ticket.tickets.escalate"
  ]
}
```

- [ ] **Step 3: Make module install resolve provider services**

In `lenso-cli/src/module.rs`, when `install_module` receives a business module
name and the catalog entry has `serviceManifest`, call the existing service
manifest install path with that manifest reference. Preserve `module install`
as the business entrypoint and `service install` as the direct provider
entrypoint.

Add a unit test:

```rust
#[test]
fn catalog_service_entry_resolves_to_service_manifest() {
    let entry = serde_json::json!({
        "name": "support-ticket",
        "source": "service",
        "providedBy": "support-suite-provider",
        "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest"
    });

    assert_eq!(
        catalog_service_manifest_reference(&entry),
        Some("http://127.0.0.1:4110/lenso/service/v1/manifest")
    );
}
```

- [ ] **Step 4: Surface provider fields to Console**

In `available-modules-model.ts`, extend the row type:

```ts
providerName?: string | null;
serviceManifest?: string | null;
```

Map response fields:

```ts
providerName: item.providedBy ?? item.provided_by ?? null,
serviceManifest: item.serviceManifest ?? item.service_manifest ?? null,
```

Add a test row:

```ts
expect(row.providerName).toBe("support-suite-provider");
expect(row.serviceManifest).toBe("http://127.0.0.1:4110/lenso/service/v1/manifest");
```

- [ ] **Step 5: Run focused catalog checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-api available_modules_reads_official_catalog_when_no_local_catalog_exists
cargo test -p lenso-api service_modules_merges_service_provider_source_into_provided_module
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test catalog_service_entry_resolves_to_service_manifest
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm exec vitest run src/pages/available-modules-model.test.ts
```

Expected: catalog provider mapping is present in Host, CLI, and Console model.

- [ ] **Step 6: Commit per repo**

```sh
cd /Users/leosouthey/Projects/framework/lenso
git add crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs crates/platform-admin-data/catalogs/lenso-official-module-catalog.json crates/lenso-api/tests/admin_data_console.rs
git commit -m "feat: add service provider catalog mapping"

cd /Users/leosouthey/Projects/framework/lenso-cli
git add src/module.rs
git commit -m "feat: resolve module installs through provider services"

cd /Users/leosouthey/Projects/framework/lenso-runtime-console
git add src/pages/available-modules-model.ts src/pages/available-modules-model.test.ts
git commit -m "feat: show provider-backed module installs"
```

## Task 7: Support Suite Proof

**Files:**
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/module.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/smoke.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/catalog-entry.json`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/README.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/docs/support-ticket-service-module-run.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/scripts/support-ticket-host-api-smoke.ts`

- [ ] **Step 1: Add service-level smoke assertions**

In `examples/support-ticket/src/smoke.ts`, assert:

```ts
expectModuleNames(manifest, [
  "support-knowledge-base",
  "support-notification",
  "support-ticket",
]);
```

Implement the helper in the same file:

```ts
function expectModuleNames(manifest, expected) {
  const names = manifest.modules.map((module) => module.name).sort();
  if (JSON.stringify(names) !== JSON.stringify(expected)) {
    throw new Error(`Expected modules ${expected.join(", ")}, got ${names.join(", ")}`);
  }
}
```

- [ ] **Step 2: Add the sibling modules**

In `module.ts`, define two small modules:

```ts
export const supportNotificationModule = defineModule({
  capabilities: ["support_notification.notifications.send"],
  name: "support-notification",
  runtimeFunctions: [
    runtimeFunction("support-notification.send-ticket-update.v1", {
      queue: "support-ticket",
    }),
  ],
  version: "0.1.0",
});

export const supportKnowledgeBaseModule = defineModule({
  capabilities: ["support_knowledge_base.articles.read"],
  httpRoutes: [
    getRoute("/articles/{id}", {
      capability: "support_knowledge_base.articles.read",
      displayName: "Get article",
      storyTitle: "Support article viewed",
    }),
  ],
  name: "support-knowledge-base",
  version: "0.1.0",
});
```

Add them to the service:

```ts
modules: [
  supportTicketModule,
  supportNotificationModule,
  supportKnowledgeBaseModule,
],
```

Add handlers:

```ts
"support-notification": {
  runtime: {
    "support-notification.send-ticket-update.v1": ({ input }) => ({
      delivered: true,
      ticket_id: input.ticket_id,
    }),
  },
},
"support-knowledge-base": {
  http: {
    "GET /articles/{id}": ({ params }) => ({
      article: {
        id: params.id,
        title: "Invite teammates",
      },
    }),
  },
},
```

- [ ] **Step 3: Update the catalog entry**

Set:

```json
{
  "name": "support-ticket",
  "source": "service",
  "providedBy": "support-suite-provider",
  "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest"
}
```

Keep the current capabilities and install service command.

- [ ] **Step 4: Update docs**

In the README and runbook, use this flow:

```sh
pnpm --filter @lenso/example-support-ticket start
lenso module install support-ticket
lenso service check support-suite-provider
```

State that `support-ticket` is the business module and
`support-suite-provider` is the service that also provides
`support-notification` and `support-knowledge-base`.

- [ ] **Step 5: Run the support-suite checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm --filter @lenso/example-support-ticket smoke
```

If `@lenso/service-kit` is not published in the package registry, run after
workspace linking or local dependency override is available. Record that as a
release-order blocker instead of rewriting the example.

- [ ] **Step 6: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
git add examples/support-ticket docs/support-ticket-service-module-run.md scripts/support-ticket-host-api-smoke.ts
git commit -m "feat: expand support ticket into service suite"
```

## Final Integration Check

- [ ] **Step 1: Check every repo diff**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso && git status --short
cd /Users/leosouthey/Projects/framework/lenso-cli && git status --short
cd /Users/leosouthey/Projects/framework/lenso-runtime-console && git status --short
cd /Users/leosouthey/Projects/framework/lenso-examples && git status --short
```

Expected: no unstaged changes after task commits.

- [ ] **Step 2: Run focused final checks**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso && cargo test -p lenso-platform-module-remote -p lenso-service
cd /Users/leosouthey/Projects/framework/lenso-cli && cargo test
cd /Users/leosouthey/Projects/framework/lenso-runtime-console && pnpm test:local
cd /Users/leosouthey/Projects/framework/lenso-examples && pnpm --filter @lenso/example-support-ticket smoke
```

Expected: focused checks pass, except package-publish blockers explicitly tied
to unavailable `@lenso/service-kit` artifacts.

- [ ] **Step 3: Leave the branch ready**

Run:

```sh
cd /Users/leosouthey/Projects/framework/lenso && git log -1 --oneline
cd /Users/leosouthey/Projects/framework/lenso-cli && git log -1 --oneline
cd /Users/leosouthey/Projects/framework/lenso-runtime-console && git log -1 --oneline
cd /Users/leosouthey/Projects/framework/lenso-examples && git log -1 --oneline
```

Expected: each touched repo has the task commit at the top.
