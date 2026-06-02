# Architecture Rules

These rules are hard guardrails for future agent-driven development.

## Domain Structure

Domains must use the flat Rust-friendly structure:

```text
domains/{domain}/
  migrations/
  src/
    lib.rs
    config.rs
    module.rs
    public.rs
    routes/
    dto/
    commands/
    queries/
    models/
    repositories/
    events/
    jobs/
    runtime/
    tests/
```

Do not create DDD or Clean Architecture folders:

- `api`
- `application`
- `domain`
- `infrastructure`

## Domain Boundaries

- A domain must not directly access another domain's tables.
- A domain must not import another domain's internal implementation.
- Cross-domain synchronous calls must go through stable public interfaces.
- Cross-domain asynchronous work must use events.
- Shared platform behavior belongs in `crates/platform-*`, not in a business domain.

## Contracts

- No HTTP API without OpenAPI schema coverage.
- No event payload without a JSON Schema contract under `contracts/events/`.
- No runtime function without a JSON Schema contract under `contracts/runtime/functions/`.
- Error responses must use the standard error shape.
- Generated contract artifacts must be regenerated with `just generate-contracts`.
- Generated contract artifacts must not be manually patched.
- Handwritten contract artifacts must still parse and use names that match their path and title.

## SDK

- The TypeScript SDK is generated from `contracts/openapi/app-api.v1.yaml`.
- Do not hand-edit files under `packages/ts-sdk/src/generated/`.
- Handwritten SDK code belongs in `packages/ts-sdk/src/index.ts`.
- Regenerate the SDK with `just generate-ts-sdk` after changing OpenAPI.

## Runtime And Outbox

- The runtime must not own business logic.
- Domain commands that write data and emit events must use the transactional outbox.
- Domain event handlers may enqueue runtime functions, but function business behavior stays in the owning domain.
- Runtime function names must be stable, versioned, and documented under `contracts/runtime/functions/`.
- Do not add NATS, Kafka, service mesh, or Kubernetes complexity before there is a real extraction need.

## Enforcement

Run:

```sh
just arch-check
```

The checker fails on forbidden domain folders, forbidden cross-domain imports inside domain source code, stale generated contracts, stale generated SDK files, missing OpenAPI artifacts, malformed contract JSON/YAML, missing event contracts referenced by source code, event contract name/path mismatches, missing runtime function contracts for registered domain runtime functions, and runtime function contract name/path mismatches.
