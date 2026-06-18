---
name: lenso-remote-modules
description: Use when changing Lenso remote module support, crates/platform-module-remote, RemoteModuleSource, remote manifests, remote schema-admin reads, remote HTTP proxy routes under /modules/{module}/http/{*path}, remote runtime function execution, remote/custom admin surfaces, module manifest lints, or Runtime Console Remote Calls.
---

# Lenso Remote Modules

## Purpose

Use this skill for remote-module work. Remote modules are out-of-process module sources loaded through `platform-module-remote`; the host keeps ownership of auth, queues, retries, Runtime Story semantics, OpenAPI boundaries, admin rendering policy, and operator visibility.

## First Read

Read only the docs that match the task:

- HTTP proxying: `docs/architecture/module-remote-http-proxy.md`
- Runtime function execution: `docs/architecture/module-remote-runtime.md`
- Custom admin surfaces: `docs/architecture/module-custom-admin-surfaces.md`
- Manifest lints: `docs/architecture/module-manifest-lints.md`
- Broad boundaries: `docs/architecture/overview.md` and `docs/architecture/rules.md`

Use `$lenso-module-framework` for manifest/binding/admin-surface architecture, `$lenso-contracts-sdk` for OpenAPI/SDK changes, `$lenso-runtime-console` for UI work, and `$lenso-quality-gate` for verification and commits.

## Current Implementation Map

- `crates/platform-module`: manifest data, admin surfaces, runtime declarations, HTTP route declarations, and manifest lint helpers.
- `crates/platform-module-remote`: remote source loading, protocol/client behavior, admin data source, HTTP proxy registry/router, and proxy-backed runtime functions.
- `crates/lenso-bootstrap`: loads configured remote modules, injects admin data sources, builds proxy registries, and registers remote runtime handlers.
- `crates/lenso-api`: mounts host-owned remote proxy routes and includes their static proxy envelopes in OpenAPI.
- `crates/lenso-worker`: runs host-owned runtime execution for linked and proxy-backed remote functions.
- `apps/runtime-console`: renders module metadata, custom admin surfaces, manifest lints, story nodes, Technical Operations, and `/operations/remote-calls`.

## Boundaries

- Keep `ModuleManifest` pure serializable data. Do not put clients, handlers, closures, or host credentials into manifests.
- Keep loading source separate from admin rendering. `Linked`, `Remote`, and later `Wasm` describe module source; `Schema`, `DeclarativeCustom`, and `EmbeddedCustom` describe admin UI.
- The host must not forward caller bearer tokens, cookies, or arbitrary headers to remote modules.
- The host owns auth, capability checks, request/response limits, timeouts, error normalization, tracing, persisted call history, queues, retries, and Runtime Story data.
- Dynamic remote routes must not expand the committed OpenAPI document per module. The static host proxy route shape is the contract boundary.
- Remote event handlers, marketplace trust, signatures, streaming, arbitrary host bridges, JavaScript bundle execution, and Wasm execution remain separate specs unless the current task explicitly implements them.

## HTTP Proxy Rules

Remote route declarations are module-local manifest data. Validate paths before exposing metadata or proxying:

- Must start with `/`.
- Must not be absolute URLs.
- Must not contain empty, `.`, `..`, query, or fragment segments.
- Supported patterns are literal segments and single `{param}` segments.

Host proxy routes live under:

```text
/modules/{module}/http/{*path}
```

Proxy GET, POST, PUT, PATCH, and DELETE only when declaration, method, auth, capability, body policy, and response policy all pass. Preserve remote proxy calls as host-side operational data and Runtime Story nodes instead of creating a separate business story model.

## Runtime Function Rules

Remote runtime functions are host-invoked executors, not a parallel runtime:

- Load declarations from manifest data.
- Register proxy-backed handlers in the host `FunctionRegistry`.
- Let the worker claim `runtime.function_runs`, construct `ExecutionContext`, invoke the remote protocol, apply retry policy, and write execution logs.
- Show remote invocation details through existing function-run, Story, retry, execution log, and Technical Operations surfaces.

Do not let remote modules poll `runtime.function_runs`, consume `platform.outbox`, or own retries.

## Admin Surface Rules

- `Schema`: plain generic entity list/detail reads through `AdminDataSource`.
- `DeclarativeCustom`: host-rendered trusted Runtime Console components from manifest data. No module-authored JavaScript.
- `EmbeddedCustom`: module-owned UI behind sandbox policy. The first lane is iframe with origin checks and no host bridge.
- `fallback_schema` can provide a host-rendered escape hatch, but it does not turn a custom surface into a generic schema surface.
- Manifest lint rules belong in `platform-module` and backend metadata. Runtime Console screens may filter and render lints, but must not duplicate lint rule logic in TypeScript.

## Validation

Use focused Rust checks first:

```sh
cargo test --locked -p platform-module --all-targets
cargo test --locked -p platform-module-remote --all-targets
cargo test --locked -p platform-admin-data --all-targets
cargo test --locked -p lenso-bootstrap --all-targets
```

For API/contract changes:

```sh
just generated-check
just arch-check
```

For Runtime Console changes:

```sh
just console-check
```

For cross-layer remote-module work, use `just check` or `just ci`.
