---
name: lenso-contracts-sdk
description: Use when changing Lenso API contracts, OpenAPI schemas, JSON Schema event or error contracts, generated artifacts under contracts, or the generated TypeScript SDK in packages/ts-sdk, including any change requiring just generate, just generated-check, just sdk-check, or OpenAPI source updates.
---

# Lenso Contracts SDK

## Purpose

Use this skill for contract and generated SDK work. Rust/OpenAPI/event sources are authoritative; generated files are committed but must not be edited by hand.

## Source Of Truth

- API OpenAPI source: per-endpoint `#[utoipa::path]` annotations on the handlers (`domains/*/src/routes/`, `apps/api/src/admin_runtime/handlers.rs`); document-level metadata and router assembly in `apps/api/src/openapi.rs`
- Contract generator: `tools/generate-contracts`
- TS SDK generator: `tools/generate-ts-sdk`
- Committed OpenAPI artifact: `contracts/openapi/app-api.v1.yaml`
- Error schemas: `contracts/errors/*`
- Event schemas: `contracts/events/*`
- Runtime schemas: `contracts/runtime/*`
- Generated SDK: `packages/ts-sdk/src/generated/*`
- Handwritten SDK facade: `packages/ts-sdk/src/index.ts`

## Hard Rules

- Do not hand-edit `contracts/*` artifacts when a generator owns them.
- Do not hand-edit `packages/ts-sdk/src/generated/*`.
- Change Rust OpenAPI or generator sources first, then regenerate.
- Every HTTP API needs OpenAPI coverage via a `#[utoipa::path]` annotation on its handler. Do not add detached `#[utoipa::path]` stub functions; do not re-declare paths/schemas in `openapi.rs`.
- Every event payload needs a JSON Schema contract under `contracts/events/`.
- Error responses must use the standard platform error shape.

## Workflow

1. Identify the authoring source for the changed contract.
2. Update Rust API schemas, event definitions, or generator code.
3. Run generation:

```sh
just generate
```

4. Review generated diffs and confirm they match the source change:

```sh
git diff -- contracts packages/ts-sdk/src/generated
```

5. Validate freshness:

```sh
just generated-check
just arch-check
```

6. If SDK behavior or types are affected, run:

```sh
just sdk-check
```

## Common Patterns

For a new or changed HTTP endpoint:

- Update route behavior and DTOs.
- Add or update the `#[utoipa::path]` annotation on the handler and register it with `routes!` in the router.
- Regenerate with `just generate`.
- Add or update API contract tests.

For a new or changed event:

- Update the domain event type and emitter.
- Update `tools/generate-contracts` so the event schema is generated.
- Ensure `contracts/events/{domain}/{event}.schema.json` is generated.
- Run `just arch-check`; it enforces documented current events.

For SDK ergonomic APIs:

- Keep generated code in `packages/ts-sdk/src/generated/*`.
- Put handwritten convenience wrappers in `packages/ts-sdk/src/index.ts`.
- Add SDK tests under `packages/ts-sdk/tests`.

## Validation Choice

- Contract-only change: `just generated-check && just arch-check`.
- SDK facade change: `just sdk-check`.
- Cross-cutting API plus SDK change: `just generated-check && just arch-check && just sdk-check`.
- Broad release-quality check: `just check` or `just ci`.
