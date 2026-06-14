# Package Readiness

This document defines the package boundary before publishing npm or crates.io
artifacts and before moving examples into a separate repository.

The framework-facing package strategy lives in
[architecture/framework-public-surface.md](architecture/framework-public-surface.md).
This document is the backend packaging checklist for that strategy.

## Current Publish Surface

Publish only consumer-facing packages first:

- `@lenso/ts-sdk` from this backend repository.
- `@lenso/remote-module-kit` from the sibling `lenso-runtime-console`
  repository.
- the `lenso` crates.io facade crate, which is already reserved and should
  become the public Rust entrypoint when it has real source.

Do not publish the current internal Rust workspace crates directly. Names such
as `platform-core`, `platform-module`, `platform-runtime`, and `app-bootstrap`
are implementation details until their API contracts are intentionally designed
for external consumers.

## First Publish Order

Use `0.1.0` for the first npm packages unless a blocking contract change lands
before publication. Keep the first batch focused on packages a user can install
from a blank project:

| Order | Artifact | Version | Source repo | Publish stance |
| --- | --- | --- | --- | --- |
| 1 | `@lenso/ts-sdk` | `0.1.0` | `lenso` | Publish first after `just package-readiness` passes. |
| 2 | `@lenso/remote-module-kit` | `0.1.0` | `lenso-runtime-console` | Publish after the backend SDK package gate is green. |
| 3 | `lenso` crates.io crate | next real version after reserved `0.0.1` | `lenso` | Defer until the facade crate exists and passes `cargo publish --dry-run`. |
| 4 | examples repository | n/a | separate repository | Extract only after examples consume published packages or documented local overrides. |

The npm packages are independent artifacts, but publishing the SDK first keeps
the generated API contract available before examples and module tooling point
users at registry installs. The Rust crate is a separate facade decision and
should not force internal workspace crates into public API shape.

Before publishing, check registry state from a clean checkout:

```sh
npm view @lenso/ts-sdk version --json
npm view @lenso/remote-module-kit version --json
cargo info lenso
```

For npm, `E404` means the package name has no published version yet. For
crates.io, the current `lenso` package is a reserved placeholder at `0.0.1`; do
not publish workspace implementation crates to work around that.

## Backend Package Gate

Run:

```sh
just package-readiness
```

The gate checks that:

- `@lenso/ts-sdk` is not marked private.
- its package license is MIT and the tarball includes `LICENSE`.
- its npm publish config targets the public npm registry.
- `pnpm --dir packages/ts-sdk run build` produces a clean `dist/`.
- `npm pack --dry-run` includes only the package manifest, README, and compiled
  `dist/` files.
- every current Rust workspace package remains `publish = false`, so internal
  crates cannot be published accidentally while the facade crate is still being
  prepared.

This gate is intentionally a publish preflight. It does not upload anything to
npm or crates.io.

If the backend package gate is green and the registry check shows no existing
`@lenso/ts-sdk` version, publish from the package directory:

```sh
cd packages/ts-sdk
npm publish --access public
```

## Coordinated Runtime Console Package

Before examples move out of this repository, publish or dry-run the sibling
Runtime Console package that examples depend on:

```sh
pnpm --dir ../lenso-runtime-console run check
(
  cd ../lenso-runtime-console
  npm pack --dry-run --json packages/remote-module-kit
)
```

The `@lenso/remote-module-kit` package should expose built JavaScript and type
declarations from a stable package entrypoint. Examples must not depend on a
local `file:` path into `../lenso-runtime-console`.
Its package gate also verifies the MIT license metadata and `LICENSE` tarball
entry.

If the Runtime Console package gate is green and the registry check shows no
existing `@lenso/remote-module-kit` version, publish from the package directory:

```sh
cd ../lenso-runtime-console/packages/remote-module-kit
npm publish --access public
```

## Crates.io Direction

The crates.io package named `lenso` should be the first public Rust package. Its
job is to be a small facade over stable module-authoring contracts, not to expose
the full backend implementation.

Before replacing the reserved placeholder with a real version:

- decide the license for public Rust packages;
- add package metadata such as description, repository, homepage, and README;
- keep internal workspace crates `publish = false`;
- run `cargo package --list` and `cargo publish --dry-run` from an isolated
  facade crate checkout or release branch.

## Example Extraction Gate

Create a separate examples repository only after these are true:

- `@lenso/remote-module-kit` is published or has a successful publish dry-run.
- `@lenso/ts-sdk` has a clean package dry-run.
- examples use registry versions or documented local override instructions, not
  sibling `file:` dependencies.
- the examples repository has its own CI that can start the module, fetch
  `/lenso/module/v1/manifest`, and run a smoke command without this monorepo.

The first extracted repository should contain:

- the JavaScript `hello-action` remote module;
- a Rust remote module example after its dependencies are either public or
  vendored as a fixture;
- a short README pointing back to the backend and Runtime Console repositories.

Keep a minimal fixture inside this backend repository for integration tests. The
external examples repository is for users; this repository still needs local
fixtures for CI and contract coverage.
