# Lenso authoring tooling

The `lenso-authoring` crate is the authoring boundary for vNext App projects.
It changes project data and package-manager inputs, validates the exact locked
artifacts, and materializes a new immutable Resolved App Plan. It does not
install packages, discover Modules, or mutate a running Kernel.

## Project document

An authoring project is a JSON document containing:

- `composition.modules`: App-local Module Instance declarations, including
  package identity, entrypoint, non-secret configuration, Capability endpoints,
  requirements, and an optional Execution Adapter class;
- `composition.bindings`: explicit consumer-to-provider Capability bindings;
- `packages`: package-manager inputs selected by the author;
- `lock`: exact artifact locators, versions, sources, and `sha256:` digests;
- `contracts`: Descriptor paths and checked-in generated Rust/TypeScript files;
- `profiles`: named Web composition recipes that select Module Instance keys
  before resolution.

The current project and lock schema versions are both `1`. The lock is read as
an exact input: a missing, mismatched, or locally modified artifact fails
`check` and `resolve`. For local Bun/npm artifacts, the resolved entrypoint
must be the exact digest-checked artifact that the Bun Adapter executes.
Remote locators may carry an externally verified digest, but cannot contain
credentials and must be materialized by the host before `run`.

Secret values are rejected from Module configuration; Modules receive
references such as `{ "secret_ref": "NAME" }` instead. Configuration schemas
support the structural keywords `type`, `const`, `enum`, `required`,
`properties`, `additionalProperties`, and `items`; unsupported validation
keywords fail `check` instead of being silently ignored.

## Commands

The `lenso` binary exposes the same authoring boundary for local workflows:

```text
lenso add --project lenso.json --key greeter --package example.greeter \
  --source cargo --version 1.0.0 --manifest Cargo.toml
lenso check --project lenso.json --execution-class lenso.native-rust@1
lenso resolve --project lenso.json --execution-class lenso.native-rust@1 \
  --output .lenso/resolved-plan.json
lenso run --project lenso.json
```

`add` updates App Composition and an existing selected package-manager input.
`check` verifies schema, lock/artifact integrity, generated contracts,
configuration, execution classes, and Capability resolution. `resolve` writes
canonical bytes for the immutable plan. `run` resolves again and passes that
plan to the caller-selected Runtime Driver and Execution Adapter catalog; a
new project or lock change therefore requires a new resolution before restart.
The standalone binary has no linked third-party factories, so a non-empty App
must call the library API from a host that supplies its Adapter catalog.

The library API is the integration seam for real host adapters:

```rust,ignore
run_project(&project, root, driver, adapters, timeout, options).await?;
```

The Kernel only receives the resulting plan. It does not read this document,
package manifests, lock files, or secret stores.
