# Framework Public Surface

Lenso should be packaged as a framework for people building backend systems and
modules, not as an application repository that users clone and edit directly.
The public surface is the set of packages, imports, commands, and templates a
user needs to install before writing their own backend or module.

## Product Shape

The intended first-user flow is:

```sh
cargo add lenso@0.3.8 --features host
pnpm add @lenso/remote-module-kit@0.1.1
```

Not every project needs every package:

- Rust linked-module authors use the `lenso` crate.
- JavaScript or TypeScript remote-module authors use
  `@lenso/remote-module-kit`.
- API consumers use the OpenAPI contract directly.
- Application starters and example repositories compose those packages into a
  runnable backend, worker, migration, Runtime Console, and remote module demo.

The source repositories can stay organized around implementation ownership. The
package boundary is the user-facing contract.

Current registry baseline:

- `lenso@0.3.0` is published on crates.io without the `host` feature.
- `lenso@0.3.8` is the crates.io facade line for generated hosts with the
  `host` feature.
- `@lenso/remote-module-kit@0.1.1` is published from the Runtime Console
  repository.

## Rust Facade Crate

The crates.io `lenso` package is the public Rust facade crate. It should not
expose the whole backend implementation.

The first useful facade focuses on serializable module declarations:

- manifest construction and linting;
- schema-admin declarations;
- runtime function declarations;
- event handler declarations;
- HTTP route declarations;
- console surface declarations.

These declaration contracts live in `crates/lenso-contracts`, are re-exported
by `crates/lenso`, and are re-exported by `crates/platform-module` for backend
workspace compatibility. Behavior seams that depend on host internals, such as
linked binding builders, admin data sources, and event/function registration
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
- `run_api_with_embedded_worker_from_env_with_composition` for explicit
  single-process local or small-host boot when independent worker scaling is
  not needed;
- `Migration` and `ModuleManifest` re-exports for starter module metadata;
- `lenso::host::http` re-exports for linked HTTP handlers, including
  `OpenApiRouter`, `routes!`, `Path`, `JsonBody`, standard error response
  helpers, `AppContext`, and `LinkedHttpContribution`.

`lenso::host` should not grow a repository layer, query builder, CRUD framework,
or auth/session abstraction just because the starter needs one example. The
starter may use normal Rust crates such as `sqlx`, `serde`, `axum`, and
`utoipa` directly for app-owned business code. Keep promoting only boot and HTTP
authoring helpers that stay stable across real starter data slices. App-owned
SQL and CRUD code stay in the starter.

The starter host template lives in the standalone
[`LioRael/lenso-cli`](https://github.com/LioRael/lenso-cli) repository and is
the single source for the `lenso host init <dir>` scaffolder. It keeps the
current API, worker, and migration entrypoints visible from a blank project
while depending on the crates.io `lenso` package with the `host` feature. Treat
new needs in that template as a signal for the next host facade extraction.

## Remote Module Kit

`@lenso/remote-module-kit` is the primary package for out-of-process module
authors. It should provide:

- remote manifest types and builders;
- a small development server for the Lenso module protocol;
- helpers for schema-admin data, HTTP routes, runtime functions, and event
  handlers;
- stable request and response envelopes that match the host protocol.

Examples must not depend on a sibling `file:` path into
`../lenso-runtime-console`. Before examples move into an external repository,
this package needs a clean build output, declarations, package metadata, and
`npm pack --dry-run` coverage.

## Starter And Examples

The examples repository is the learning surface after packages are publishable.
It is not the first package boundary; it consumes package boundaries after they
exist.

The first examples repository is
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). It starts
with the JavaScript `hello-action` remote module and uses registry packages
instead of sibling workspace paths.

Grow examples only when:

- `@lenso/remote-module-kit` is installed from npm or an explicitly documented
  local override;
- Rust examples either depend on the public `lenso` facade crate or explicitly
  vendor fixture-only code;
- example CI can start a module, fetch `/lenso/module/v1/manifest`, and run a
  smoke check without this monorepo.

The backend repository should still keep minimal fixtures for integration tests
and contract coverage. External examples are for users; internal fixtures are
for CI.

## Public Surface Admission Rules

A package, crate, command, or template should become public only when it has:

- a clear target author: Rust linked-module author, remote-module author, API
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

## Near-Term Sequence

1. Keep `@lenso/remote-module-kit` in the Runtime Console repository and grow it
   as the remote-module authoring facade.
2. Keep the crates.io `lenso` facade limited to stable module-authoring
   declarations until a host application API is intentionally designed.
3. Keep the standalone `lenso-cli` starter template as the host facade pressure
   test until its boot, migration, HTTP, and app-owned data slices stabilize.
4. Leave app-owned SQL, repositories, CRUD shape, auth/session policy, and
   console UI out of `lenso::host`.
5. Grow the external examples repository without reintroducing sibling
   workspace dependencies.
