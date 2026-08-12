# Framework Public Surface

Lenso should be packaged as a framework for people building backend systems and
modules, not as an application repository that users clone and edit directly.
The public surface is the set of packages, imports, commands, and templates a
user needs to install before writing their own backend or module.

## Product Shape

The intended first-user flow is:

```sh
cargo add lenso@0.3.35 --features host
pnpm add @lenso/service-kit@0.1.5
```

Not every project needs every package:

- Rust linked-module authors use the `lenso` crate.
- JavaScript or TypeScript Provider authors use `@lenso/service-kit` for the
  `lenso.service.v1` tier.
- Rust Autonomous Service authors use `lenso-service` declarations and the
  `lenso-autonomous-service` runtime for the `lenso.service.v2` tier.
- API consumers use the OpenAPI contract directly.
- Application starters and example repositories compose those packages into a
  runnable backend, worker, migration, Console, and service demo.

The source repositories can stay organized around implementation ownership. The
package boundary is the user-facing contract.

Current registry baseline:

- `lenso@0.3.35` is the crates.io facade line for generated hosts with the
  `host` feature.
- `@lenso/service-kit@0.1.5` is owned and published from
  `sdk/typescript/packages/service-kit` in this framework repository.

## Rust Facade Crate

The crates.io `lenso` package is the public Rust facade crate. It should not
expose the whole backend implementation.

The first useful facade focuses on serializable module declarations:

- manifest construction and linting;
- Business API route declarations;
- runtime function declarations;
- event handler declarations;
- HTTP route declarations;
- console surface declarations.

These declaration contracts live in `crates/lenso-contracts`, are re-exported
by `crates/lenso`, and are re-exported by `crates/platform-module` for backend
workspace compatibility. Behavior seams that depend on host internals, such as
linked binding builders, route handlers, and event/function registration
contexts, remain behind internal crates and are exposed to users through the
narrow `lenso::host` facade. Those host dependency crates are published with
Lenso-owned package names, such as `lenso-platform-core`, only so Cargo can
resolve the `lenso/host` feature from crates.io.

Host application assembly is exposed through the narrow `lenso::host` facade.
Keep that surface small: boot the API, worker, and migration runner, compose
linked modules, and expose linked HTTP authoring helpers.

The current host-facing surface is intentionally narrow:

- `HostBuilder`, `HostComposition`, and `HostLinkedModule` for composing
  host-owned linked modules;
- `run_api_from_env_with_composition`, `run_worker_from_env_with_composition`,
  and `run_migrations_from_env_with_composition` for booting the three host
  entrypoints;
- System Plane capability providers are composed explicitly beside the Host
  and mounted on an independent SPIFFE mTLS-only router. They are never enabled
  implicitly through environment configuration or mounted on the Data Plane;
- `run_api_with_embedded_worker_from_env_with_composition` for explicit
  single-process local or small-host boot when independent worker scaling is
  not needed;
- `Migration` and `ModuleManifest` re-exports for starter module metadata;
- `lenso::host::http` re-exports for linked HTTP handlers, including
  `OpenApiRouter`, `routes!`, `Path`, `JsonBody`, standard error response
  helpers, `AppContext`, and `LinkedHttpContribution`.
- `lenso::host::runtime` re-exports for behavior-bearing linked Modules,
  including `Module`, `LinkedBinding`, runtime function definitions and
  handlers, retry policy, execution context, and the standard app error types.
  This seam registers only Module-owned behavior already declared in the
  manifest; runtime queues and scheduling remain Host-owned.
- `lenso::host::transaction::LinkedTransaction` for the one stable persistence
  boundary shared by host-owned linked modules: a scoped idempotency claim,
  app-owned SQL, and Outbox publication can commit or roll back atomically. It
  is independently available through the lightweight `host-transactions`
  feature; the complete `host` feature includes it for compatibility.

`lenso::host` should not grow a repository layer, query builder, CRUD framework,
or auth/session abstraction just because the starter needs one example. The
starter may use normal Rust crates such as `sqlx`, `serde`, `axum`, and
`utoipa` directly for app-owned business code. Keep promoting only boot and HTTP
authoring helpers that stay stable across real starter data slices. App-owned
SQL and CRUD code stay in the starter.

The transaction seam does not change that rule. Applications continue to use
ordinary `sqlx` queries against module-owned tables. The facade owns only the
platform invariants callers cannot reproduce safely: the idempotency claim and
Outbox insert use the exact same caller transaction, while platform table names
and `lenso-platform-core` remain outside application imports.

The starter host template lives in the standalone
[`LioRael/lenso-cli`](https://github.com/LioRael/lenso-cli) repository and is
the single source for the `lenso host init <dir>` scaffolder. It keeps the
current API, worker, and migration entrypoints visible from a blank project
while depending on the crates.io `lenso` package with the `host` feature. Treat
new needs in that template as a signal for the next host facade extraction.

The exact ownership and language support matrix lives in
[Service Capability Tiers](service-capability-tiers.md).

## TypeScript Service Kit — Provider

`@lenso/service-kit` is the primary TypeScript package for Provider Services:
independently running backends that provide one or more Modules to a Host. It
should provide:

- service and module manifest types and builders;
- a small development server for the Lenso service protocol;
- helpers for declared business HTTP routes, runtime functions, and Event
  handlers;
- stable request and response envelopes that match the host protocol.

This package implements `lenso.service.v1`. It does not provide Autonomous
Service parity for `lenso.service.v2` and must not advertise Service-owned
storage, direct Service-to-Service HTTP/gRPC, Durable Workflow, or Workload
Identity runtime ownership.

Examples must consume the registry package or an exact integration-set override
to this repository's SDK workspace. The package has its own build output,
declarations, metadata, tests, and package packaging coverage.

## Rust Autonomous Services

The Rust `lenso.service.v2` surface combines `lenso-service` declarations with
the `lenso-autonomous-service` runtime. It currently owns Service Stores,
migrations, Inbox and Outbox delivery, runtime queues, health, shutdown, and
local Story Segments. Its public Data Plane capabilities include direct HTTP,
direct gRPC, Event Contracts, Durable Workflows, Workload Identity, and
Delegated Actor Context.

## Starter And Examples

The examples repository is the learning surface after packages are publishable.
It is not the first package boundary; it consumes package boundaries after they
exist.

The first examples repository is
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). It starts
with JavaScript service providers such as `hello-service`,
`account-profile-service`, and `support-service`, and uses registry packages
instead of sibling workspace paths. The support-ticket example is the preferred
business-shaped service path for first-user documentation.

Grow examples only when:

- `@lenso/service-kit` is installed from npm or an explicitly documented
  local override;
- Rust examples either depend on the public `lenso` facade crate or explicitly
  vendor fixture-only code;
- example CI can start a service, fetch `/lenso/service/v1/manifest`, and run a
  focused check without this monorepo.

The backend repository should still keep minimal fixtures for integration tests
and contract coverage. External examples are for users; internal fixtures are
for CI.

## Public Surface Admission Rules

A package, crate, command, or template should become public only when it has:

- a clear target author: Rust linked-module author, service author, API
  client author, or operator;
- a minimal install command;
- a stable import path or binary name;
- README usage that starts from a blank project, not from this monorepo;
- package dry-run or build output checks;
- an explicit statement about what remains internal.

Do not publish implementation crates or packages merely because examples need a
local dependency. If an example cannot run without an internal package, either
promote a small facade or keep the example inside this repository until the
facade exists.

## Current Direction

1. Keep `@lenso/service-kit` in the framework SDK workspace as the Service
   authoring facade for Provider `lenso.service.v1` only.
2. Keep the crates.io `lenso` facade limited to stable declarations and narrow
   Host composition seams.
3. Use the standalone `lenso-cli` starter as the Host facade pressure test.
4. Leave app-owned SQL, repositories, CRUD shape, auth/session policy, and
   Console UI out of `lenso::host`.
5. Keep examples on registry packages or exact reviewed integration sets.
