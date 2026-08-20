# Package Readiness

> **Legacy v0.3.x maintenance guide:** This page applies to the maintained
> Service-oriented release line on `main`. It does not define vNext
> architecture or authorize publication from `next`. See the
> [documentation map](README.md).

Package readiness is checked by the owning ecosystem workflow. The root
workspace contains public Cargo crates and the TypeScript `@lenso/service-kit`
package; tools and fixtures remain non-publishable.

## Cargo

Run the repository gate first, then package the crates changed by the release:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo run --locked -p lenso-api-contracts --bin generate-contracts
cargo test --locked -p lenso-api-contracts --test architecture
cargo test --locked -p lenso-api-contracts --test generated_artifacts
cargo package --locked -p <crate> --allow-dirty
cargo publish --dry-run --locked -p <crate> --allow-dirty
```

Release-plz resolves and publishes the changed dependency closure in Cargo
order. It ignores manifests with `publish = false` and uses the crates.io
registry as the version source of truth.

## npm

The public TypeScript package is built and packed from its workspace:

```sh
pnpm --dir sdk/typescript check
pnpm --dir sdk/typescript --filter @lenso/service-kit pack --dry-run
```

Changesets updates the package version and changelog in a reviewed pull
request. The merged version is published by the repository's npm Trusted
Publisher.

## Registry evidence

After a successful release, record the source commit, package version, tag,
archive digest, and registry/provenance URLs. Existing public versions, tags,
and changelogs are never rewritten to reconcile an old shadow candidate.
