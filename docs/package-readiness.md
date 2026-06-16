# Package Readiness

This document defines the backend package boundary before publishing crates.io
artifacts.

The framework-facing package strategy lives in
[architecture/framework-public-surface.md](architecture/framework-public-surface.md).

## Current Publish Surface

Publish only backend packages with a current consumer-facing job:

- the `lenso` crates.io facade crate, which carries the public
  module-authoring declaration surface.

Do not publish the current internal Rust workspace crates directly. Names such
as `platform-core`, `platform-module`, `platform-runtime`, and `app-bootstrap`
are implementation details until their API contracts are intentionally designed
for external consumers.

Registry baseline as of the first release line:

- `lenso@0.1.0` is published on crates.io.

## Published Baseline

Keep the first release line focused on packages a user can install from a blank
project:

| Order | Artifact | Version | Source repo | Publish stance |
| --- | --- | --- | --- | --- |
| 1 | `lenso` crates.io crate | `0.1.0` | `lenso` | Already published; keep internal workspace crates private. |

## Backend Package Gate

Run:

```sh
just package-readiness
```

The gate verifies that `cargo package -p lenso --allow-dirty` can assemble the
facade crate without depending on unpublished internal crates. It does not upload
anything to crates.io.

## Crates.io Direction

The crates.io package named `lenso` is the first public Rust package. Its job is
to be a small facade over stable module-authoring contracts, not to expose the
full backend implementation.

Before publishing a future version:

- add package metadata such as description, repository, homepage, and README;
- keep internal workspace crates `publish = false`;
- run `cargo package --list -p lenso`;
- run `cargo publish --dry-run -p lenso` from a release branch or the GitHub
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
