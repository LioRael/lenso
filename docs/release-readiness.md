# Release Readiness

Release readiness is repository-local. Run the explicit quality gate and the
package check for the ecosystem that changed; do not wait for a
cross-repository plan.

## Repository gate

Run the same owner-local commands used by GitHub Actions:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p lenso --features host-transactions --test host_outbox_relay
cargo run --locked -p lenso-api-contracts --bin generate-contracts
cargo test --locked -p lenso-api-contracts --test architecture
cargo test --locked -p lenso-api-contracts --test generated_artifacts
```

The committed contract files must be byte-for-byte unchanged after generation.

## Cargo package gate

For each changed public crate:

```sh
cargo package --locked -p <crate> --allow-dirty
cargo publish --dry-run --locked -p <crate> --allow-dirty
```

Release-plz repeats package verification before publishing. Its release PR is
the review boundary for exact versions and dependency closure.

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

## Local verification

When runtime behavior changes, use the owning packages and Compose directly:

```sh
cp .env.example .env
docker compose -f infrastructure/local/docker-compose.yml up -d postgres
cargo run --locked -p lenso-migrate
cargo test --locked -p lenso-api --test first_user -- --nocapture
cargo run --locked -p lenso-api
cargo run --locked -p lenso-worker
```

Console checks run in the sibling `lenso-console` repository. User-facing
examples live in `LioRael/lenso-examples` and own their package dependencies.

## Evidence after publishing

Record the source commit, package version, registry URL, tag, archive digest,
and provenance URL in the repository or release notes. Keep evidence static and
credential-free. The registry and published archive are authoritative; do not
restore the retired `lenso-release` coordinator to repair a failed publication.
