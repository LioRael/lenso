# Package Readiness

This document defines the backend package boundary before publishing crates.io
artifacts.

The framework-facing package strategy lives in
[architecture/framework-public-surface.md](architecture/framework-public-surface.md).

## Current Publish Surface

Publish only backend packages with a current consumer-facing job:

- the `lenso-contracts` crates.io crate, which carries shared declaration data;
- the `lenso` Git facade crate, which re-exports declarations and exposes the
  `host` feature for generated hosts.

Do not publish the current internal Rust workspace crates directly. Names such
as `platform-core`, `platform-module`, `platform-runtime`, `lenso-api`,
`lenso-worker`, `lenso-migrate`, and `lenso-bootstrap` are implementation
details until their API contracts are intentionally designed for external
consumers.

`lenso-host` is a compatibility re-export for generated hosts that still import
`lenso_host::*`; do not publish it to crates.io.

Registry baseline as of the first release line:

- `lenso@0.1.0` is published on crates.io.
- `lenso-contracts@0.2.1` is the next crates.io publish candidate.
- `lenso@0.2.1` is the Git facade candidate for generated hosts.
- `lenso-cli` is owned by the standalone
  [`LioRael/lenso-cli`](https://github.com/LioRael/lenso-cli) repository.

## Published Baseline

Keep the first release line focused on packages a user can install from a blank
project:

| Order | Artifact | Version | Source repo | Publish stance |
| --- | --- | --- | --- | --- |
| 1 | `lenso` crates.io crate | `0.1.0` | `lenso` | Already published; keep internal workspace crates private. |
| 2 | `lenso-contracts` crates.io crate | `0.2.1` | `lenso` | Shared declaration contracts used by the facade and platform crates. |
| 3 | `lenso` Git crate | `0.2.1` | `lenso` | Facade for current module-authoring contracts and the Git-only `host` feature. |

## Backend Package Gate

Run:

```sh
just package-readiness
```

The gate verifies that `cargo package -p lenso-contracts --allow-dirty` can
assemble the crates.io contract package. It does not upload anything to
crates.io.

`cargo package -p lenso-host --allow-dirty --no-verify` is useful as a boundary
probe, but it is not a release gate yet. Today it stops at repository-internal
path dependencies, which is expected for the Git-pinned host facade.

## Crates.io Direction

The crates.io package named `lenso-contracts` carries the stable
module-authoring contracts. The Git-pinned `lenso` facade re-exports those
contracts and exposes the narrow generated-host boot API through its `host`
feature.

Before publishing a future version:

- add package metadata such as description, repository, homepage, and README;
- keep internal workspace crates `publish = false`;
- run `cargo package --list -p lenso-contracts`;
- run `cargo publish --dry-run -p lenso-contracts` from a release branch or the GitHub
  `release` workflow with `publish_rust_crate=false` when ready to validate
  against crates.io.

For the real upload, prefer the GitHub `release` workflow with
`publish_rust_crate=true` and the `CARGO_REGISTRY_TOKEN` repository secret
configured. Manual publishing should remain a fallback for registry outages or
workflow failures after the same dry-run checks have passed.

## Examples Repository

User-facing examples live outside this backend repository, starting with
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). That
repository owns its package dependencies and smoke CI. This backend repository
keeps only Rust fixtures needed for integration tests and contract coverage.
