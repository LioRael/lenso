# Framework Public Surface

Lenso should be packaged as a framework for people building backend systems and
modules, not as an application repository that users clone and edit directly.
The public surface is the set of packages, imports, commands, and templates a
user needs to install before writing their own backend or module.

## Product Shape

The intended first-user flow is:

```sh
cargo add lenso@0.1.0
pnpm add @lenso/remote-module-kit@0.1.1
pnpm add @lenso/ts-sdk@0.1.0
```

Not every project needs every package:

- Rust linked-module authors use the `lenso` crate.
- JavaScript or TypeScript remote-module authors use
  `@lenso/remote-module-kit`.
- API consumers use `@lenso/ts-sdk`.
- Application starters and example repositories compose those packages into a
  runnable backend, worker, migration, Runtime Console, and remote module demo.

The source repositories can stay organized around implementation ownership. The
package boundary is the user-facing contract.

Current registry baseline:

- `lenso@0.1.0` is published on crates.io.
- `@lenso/remote-module-kit@0.1.1` is published from the Runtime Console
  repository.
- `@lenso/ts-sdk@0.1.0` is published from this backend repository.

## Rust Facade Crate

The crates.io package named `lenso` is the public Rust facade crate. It should
not expose the whole backend implementation.

The first useful facade focuses on serializable module declarations:

- manifest construction and linting;
- schema-admin declarations;
- runtime function declarations;
- event handler declarations;
- HTTP route declarations;
- console surface declarations.

These declaration contracts live in `crates/lenso` and are re-exported by
`crates/platform-module` for backend workspace compatibility. Behavior seams
that depend on host internals, such as linked binding builders, admin data
sources, and event/function registration contexts, remain in `platform-module`
until a stable external host-authoring API exists. Internal crates such as
`platform-core`, `platform-http`, `platform-runtime`, `platform-admin`,
`platform-admin-data`, `platform-module-remote`, `platform-testing`, and
`app-bootstrap` should remain `publish = false` until a specific external use
case justifies publishing them.

Host application assembly is a later facade layer. It may eventually expose
helpers for booting an API, worker, and migration runner, but the first package
should avoid promising a complete hosted application API before the shape is
clear.

The intended long-term shape is still one `lenso` facade crate with feature
gates for different authors. Module authors should be able to use the default
module-authoring surface, while host applications can opt into a `host` feature
that exposes narrow boot helpers such as API, worker, and migration runners. The
`host` feature must not leak internal app or platform crate types in public
signatures; it should stay a small facade over intentionally stable host
operations.

Until that package shape can be made cycle-free and publishable, `crates/lenso-host`
is the internal pressure-test crate for the same API. It may depend on the
workspace app crates, but starter templates should treat only its thin boot
helpers as the host-facing surface.

The transitional starter host template in `templates/starter-host` is the
pressure test for that future facade. It keeps the current API, worker, and
migration entrypoints visible from a blank project while depending on internal
host crates through Git dependencies. It uses Cargo's system-Git fetch mode so
private repository credentials follow normal Git configuration. Treat
duplication in that template as a signal for the next host facade extraction,
not as a stable public API.

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

## TypeScript SDK

`@lenso/ts-sdk` is a generated client for the host HTTP API. It should be
published independently as an npm package, but its source should stay with the
backend contracts for now.

Keeping the SDK source in this backend repository preserves an atomic workflow:

- Rust handlers define OpenAPI.
- committed contracts are regenerated.
- generated SDK files are regenerated.
- package dry-runs prove the publish artifact is clean.

An independent SDK source repository would add cross-repository synchronization
without changing the user install story. Revisit that only if the SDK gains a
large handwritten surface, multiple release trains, or non-TypeScript language
targets that need their own maintainers.

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
- `@lenso/ts-sdk` is installed from npm or an explicitly documented local
  override;
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

1. Keep the backend-owned `@lenso/ts-sdk` source with the OpenAPI contracts and
   continue publishing generated npm artifacts from this repository.
2. Keep `@lenso/remote-module-kit` in the Runtime Console repository and grow it
   as the remote-module authoring facade.
3. Keep the crates.io `lenso` facade limited to stable module-authoring
   declarations until a host application API is intentionally designed.
4. Add a starter host path after the examples prove the published package
   install story end to end. The first transitional path is
   `templates/starter-host`.
5. Grow the external examples repository without reintroducing sibling
   workspace dependencies.
