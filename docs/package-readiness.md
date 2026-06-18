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
as `platform-core`, `platform-module`, `platform-runtime`, `lenso-api`,
`lenso-worker`, `lenso-migrate`, and `lenso-bootstrap` are implementation
details until their API contracts are intentionally designed for external
consumers.

`lenso-host` is the current host-facing Rust API, but it is distributed to
generated hosts as a Git-pinned dependency. Do not publish it to crates.io until
the `lenso-*` boot crates underneath it also have a registry publish strategy.

Registry baseline as of the first release line:

- `lenso@0.1.0` is published on crates.io.
- `lenso@0.2.1` is the next facade crate publish candidate.
- `lenso-cli@0.1.1` is the first CLI publish candidate: it scaffolds host
  applications with `lenso host init <dir>`, creates linked and remote module
  scaffolds with `lenso module create`, installs remote modules with
  `lenso module add <manifest-url>`, and creates or applies Runtime Console
  package registrations. It embeds the starter template and the prebuilt
  Runtime Console payload, and depends only on registry crates, so it is
  publishable independently of the internal workspace crates. Publish it after
  its CLI gate (`just cli-check`) is stable.

## Published Baseline

Keep the first release line focused on packages a user can install from a blank
project:

| Order | Artifact | Version | Source repo | Publish stance |
| --- | --- | --- | --- | --- |
| 1 | `lenso` crates.io crate | `0.1.0` | `lenso` | Already published; keep internal workspace crates private. |
| 2 | `lenso` crates.io crate | `0.2.0` | `lenso` | Next facade crate release for current module-authoring contracts. |
| 3 | `lenso-cli` crates.io crate | `0.1.0` | `lenso` | Unified CLI for host, module, remote module, and Runtime Console package workflows. |

## Backend Package Gate

Run:

```sh
just package-readiness
```

The gate verifies that `cargo package -p lenso --allow-dirty` can assemble the
facade crate without depending on unpublished internal crates. It does not upload
anything to crates.io.

`lenso-cli` is publishable for the same reason: its dependencies are registry
crates, and the embedded starter template plus Runtime Console payload are
bundled inside the crate. Its package gate is separate:

```sh
just cli-package-readiness
```

`cargo package -p lenso-host --allow-dirty --no-verify` is useful as a boundary
probe, but it is not a release gate yet. Today it stops at repository-internal
path dependencies, which is expected for the Git-pinned host facade.

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
configured. Use `publish_lenso_cli=true` only when intentionally publishing the
CLI from this repository. Manual publishing should remain a fallback for
registry outages or workflow failures after the same dry-run checks have passed.

## Examples Repository

User-facing examples live outside this backend repository, starting with
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples). That
repository owns its package dependencies and smoke CI. This backend repository
keeps only Rust fixtures needed for integration tests and contract coverage.
