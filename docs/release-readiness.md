# Release Readiness

Release readiness is repository-local. Run the quality gate and the package
check for the ecosystem that changed; do not wait for a cross-repository plan.

## Repository gate

```sh
just check
```

This runs Rust formatting, workspace compilation and tests, generated contract
checks, architecture guardrails, and the documented repository checks.

## Cargo package gate

For each changed public crate:

```sh
cargo package --locked -p <crate> --allow-dirty
cargo publish --dry-run --locked -p <crate> --allow-dirty
```

Release-plz repeats the package verification before publishing. Its release
PR is the review boundary for the exact versions and dependency closure.

## npm package gate

```sh
pnpm --dir sdk/typescript check
pnpm --dir sdk/typescript --filter @lenso/service-kit pack --dry-run
```

Changesets owns npm version and changelog updates. The npm workflow publishes
only packages changed by a merged Changesets version PR.

## Cross-repository checks

When a contract or dependency affects another repository, update that consumer
and run its focused integration check. Do not create a synchronized framework
version or a coordinator release plan to represent the combination.

## Local smoke

Use the existing service smoke commands when runtime behavior changed:

```sh
just db-up
just migrate
just api
just worker
```

Console checks run in the sibling `lenso-console` repository. User-facing
examples live in `LioRael/lenso-examples` and own their package dependencies.

## Evidence after publishing

Record the source commit, package version, registry URL, tag, archive digest,
and provenance URL in the repository or release notes. Keep evidence static and
credential-free. The registry and published archive are authoritative; do not
restore the retired `lenso-release` coordinator to repair a failed publication.
