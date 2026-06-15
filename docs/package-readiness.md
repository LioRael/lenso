# Package Readiness

This document defines the package boundary before publishing npm or crates.io
artifacts and before moving examples into a separate repository.

The framework-facing package strategy lives in
[architecture/framework-public-surface.md](architecture/framework-public-surface.md).
This document is the backend packaging checklist for that strategy.

## Current Publish Surface

Publish only consumer-facing packages:

- `@lenso/ts-sdk` from this backend repository.
- `@lenso/remote-module-kit` from the sibling `lenso-runtime-console`
  repository.
- the `lenso` crates.io facade crate, which carries the public
  module-authoring declaration surface.

Do not publish the current internal Rust workspace crates directly. Names such
as `platform-core`, `platform-module`, `platform-runtime`, and `app-bootstrap`
are implementation details until their API contracts are intentionally designed
for external consumers.

Registry baseline as of the first release line:

- `@lenso/ts-sdk@0.1.0` is published on npm.
- `@lenso/remote-module-kit@0.1.1` is published on npm.
- `lenso@0.1.0` is published on crates.io.

## Published Baseline

Keep the first release line focused on packages a user can install from a blank
project:

| Order | Artifact | Version | Source repo | Publish stance |
| --- | --- | --- | --- | --- |
| 1 | `@lenso/ts-sdk` | `0.1.0` | `lenso` | Already published; keep `publish_ts_sdk=false` for `v0.1.0` workflow reruns. |
| 2 | `@lenso/remote-module-kit` | `0.1.1` | `lenso-runtime-console` | Already published; examples can consume the registry package. |
| 3 | `lenso` crates.io crate | `0.1.0` | `lenso` | Already published; keep internal workspace crates private. |
| 4 | examples repository | n/a | separate repository | Grow examples against registry packages or documented local overrides. |

The npm packages are independent artifacts, but publishing the SDK first keeps
the generated API contract available before examples and module tooling point
users at registry installs. The Rust crate is a separate facade decision and
should not force internal workspace crates into public API shape.

Before publishing a future version, check registry state from a clean checkout:

```sh
npm view @lenso/ts-sdk version --json
npm view @lenso/remote-module-kit version --json
cargo info lenso
```

For npm, `E404` means the package name has no published version yet. For
crates.io, do not publish workspace implementation crates to work around missing
facade coverage.

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
- `lenso` is the only publishable Rust workspace package.
- internal Rust workspace packages remain `publish = false`, so implementation
  crates cannot be published accidentally.
- `cargo package -p lenso --allow-dirty` can assemble and verify the facade
  crate without depending on unpublished internal crates.

This gate is intentionally a publish preflight. It does not upload anything to
npm or crates.io.

If the backend package gate is green and the registry check shows the intended
`@lenso/ts-sdk` version does not already exist, prefer the GitHub `release`
workflow with `publish_ts_sdk=true`. For an emergency manual publish, publish
from the package directory:

```sh
cd packages/ts-sdk
npm publish --access public
```

## Coordinated Runtime Console Package

Before adding examples that depend on new remote-module-kit behavior, publish or
dry-run the sibling Runtime Console package that examples depend on:

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

If the Runtime Console package gate is green and the registry check shows the
intended `@lenso/remote-module-kit` version does not already exist, publish from
the package directory:

```sh
cd ../lenso-runtime-console/packages/remote-module-kit
npm publish --access public
```

## Crates.io Direction

The crates.io package named `lenso` is the first public Rust package. Its job is
to be a small facade over stable module-authoring contracts, not to expose the
full backend implementation.

Before publishing a future version:

- add package metadata such as description, repository, homepage, and README;
- keep internal workspace crates `publish = false`;
- run `cargo package --list -p lenso`;
- run `cargo publish --dry-run -p lenso` from a release branch or the GitHub
  `release` workflow with both publish inputs set to `false` when ready to
  validate against crates.io.

For the real upload, prefer the GitHub `release` workflow with
`publish_rust_crate=true` and the `CARGO_REGISTRY_TOKEN` repository secret
configured. Manual publishing should remain a fallback for registry outages or
workflow failures after the same dry-run checks have passed.

## Examples Repository

The first separate examples repository is
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). Its initial
`hello-action` example uses the published `@lenso/remote-module-kit` and
`@lenso/ts-sdk` packages and runs its own smoke CI.

Keep using this gate before adding more external examples:

- `@lenso/remote-module-kit` is consumed from npm or has a documented local
  override.
- `@lenso/ts-sdk` is consumed from npm or has a documented local override.
- examples use registry versions or documented local override instructions, not
  sibling `file:` dependencies.
- the examples repository has its own CI that can start the module, fetch
  `/lenso/module/v1/manifest`, and run a smoke command without this monorepo.

The first extracted repository should contain:

- the JavaScript `hello-action` remote module;
- a Rust remote module example after its dependencies are either public or
  vendored as a fixture;
- a short README pointing back to the backend and Runtime Console repositories.

Keep minimal fixtures inside this backend repository for integration tests. The
external examples repository is for users; this repository still needs local
fixtures for CI and contract coverage.
