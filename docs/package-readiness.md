# Package Readiness

This document defines the backend package boundary before publishing crates.io
artifacts.

The framework-facing package strategy lives in
[architecture/framework-public-surface.md](architecture/framework-public-surface.md).

## Current Publish Surface

Publish only backend packages with a current consumer-facing job:

- the `lenso-contracts` crates.io crate, which carries shared declaration data;
- the `lenso` crates.io facade crate, including its optional `host` feature;
- the internal host dependency chain needed by `lenso/host`, published with
  Lenso-owned package names such as `lenso-platform-core` while preserving the
  existing Rust crate names through dependency aliases and explicit `[lib]`
  names.

Registry baseline as of the first release line:

- `lenso@0.3.0` is published on crates.io without the `host` feature.
- `lenso-contracts@0.3.1` is published on crates.io.
- `lenso@0.3.17` is the current crates.io facade line with the `host`
  dependency chain.
- `lenso-cli` is owned by the standalone
  [`LioRael/lenso-cli`](https://github.com/LioRael/lenso-cli) repository.

## Published Baseline

Keep the first release line focused on packages a user can install from a blank
project:

| Order | Artifact | Version | Source repo | Publish stance |
| --- | --- | --- | --- | --- |
| 1 | `lenso` crates.io crate | `0.3.0` | `lenso` | Already published without `host`; superseded by the staged host publish line. |
| 2 | `lenso-contracts` crates.io crate | `0.3.1` | `lenso` | Shared declaration contracts used by the facade and platform crates. |
| 3 | `lenso-platform-*`, `lenso-module-*`, and host service crates | `0.1.0` | `lenso` | Internal host dependency chain required for `lenso/host` on crates.io. |
| 4 | `lenso` crates.io crate | `0.3.17` | `lenso` | Facade for current module-authoring contracts and the crates.io `host` feature. |

## Backend Package Gate

Run:

```sh
just package-readiness
```

The gate verifies that every staged package is addressable, then dry-runs
`cargo package` for `lenso-contracts` and the first unpublished host dependency
that can be independently checked before the staged publish begins. Downstream
host crates are dry-run verified one-by-one by `scripts/publish-crates.sh`
immediately before each real upload, after their upstream crates are visible in
the registry.

## Crates.io Direction

The crates.io package named `lenso-contracts` carries the stable
module-authoring contracts. The `lenso` facade re-exports those contracts and
exposes the host boot API through its optional `host` feature. Because Cargo
requires every path dependency in a published package to resolve through the
registry, the internal host crates are published in topological order before
the facade crate.

Before publishing a future version:

- add package metadata such as description, repository, and homepage;
- add a version requirement to every published path dependency;
- use Lenso-owned crates.io package names for internal packages that would
  otherwise collide with generic names such as `auth`;
- run `just package-readiness`;
- rely on `scripts/publish-crates.sh` for the staged dry-run and upload order.

For the real upload, prefer the GitHub `release` workflow with
`publish_rust_crate=true` and the `CARGO_REGISTRY_TOKEN` repository secret
configured. Manual publishing should remain a fallback for registry outages or
workflow failures after the same dry-run checks have passed.

## Examples Repository

User-facing examples live outside this backend repository, starting with
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). That
repository owns its package dependencies and smoke CI. This backend repository
keeps only Rust fixtures needed for integration tests and contract coverage.
