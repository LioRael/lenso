# Release Process

This repository releases the Cargo CLI and its npm distribution independently.
There is no repository-wide release plan, shadow registry, central publisher,
nonce, or receipt channel.

## Cargo crate

Release-plz is currently dispatch-only and read-only. Its workflow runs
`release --dry-run` to inspect pending workspace releases; it does not open a
release pull request, publish crates, or create tags. A separately authorized
maintainer change must restore those operations after this rollout.

The crates.io registry is the source of truth for existing versions. Public
versions, tags, and the historical `CHANGELOG.md` are not rewritten. When live
publication is explicitly reauthorized, configure a crates.io Trusted Publisher
for each package (`lenso-cli`) and use no long-lived `CARGO_REGISTRY_TOKEN`.

## npm distribution

Create a changeset for every user-facing npm distribution change:

```sh
pnpm changeset
```

The Changesets workflow is currently dispatch-only and builds/inspects the
platform payload. It does not create a version pull request or publish. A
separately authorized maintainer change must restore that release boundary.

Historically, the Changesets workflow created a version pull request. After it
was merged,
the workflow builds `darwin-arm64`, `darwin-x64`, `linux-x64`, and `win32-x64`
artifacts, verifies the npm payload, and publishes `@lenso/cli` through npm
Trusted Publishing. The Cargo and npm versions are separate streams; the npm
wrapper may publish a packaging-only change without forcing a Cargo release.

Configure an npm Trusted Publisher for `@lenso/cli` before the first live
publish after this migration. The workflow uses the checked-in binary payload,
not a long-lived `NPM_TOKEN`.

## Local checks

```sh
pnpm install --frozen-lockfile
pnpm changeset status --output /tmp/lenso-cli-changesets.json
npm run check:npm-shim
cargo fmt --all -- --check
cargo test --locked --workspace
cargo metadata --locked --format-version 1
cargo package --locked --workspace --allow-dirty --no-verify
cargo publish --dry-run --locked --workspace --allow-dirty --no-verify
```

The independent Engine repository owns the portable catalog and its Wasm/package
verification. A **release-ready** cross-repository candidate may use an
ephemeral, separately recorded source patch map and exact source snapshots to
prove its local closure; do not commit that patch map. A **published** CLI
package still requires its Engine, Core, Runtime, and Protocol dependency
versions to exist in crates.io. Keep the registry package gate enabled;
publishing the CLI does not publish its independently owned dependencies.

To inspect an npm archive locally, build the current platform payload first:

```sh
npm run package:npm
npm run check:npm-publish
npm pack --dry-run --ignore-scripts
```

Cross-repository compatibility is proven by SemVer requirements, contracts,
and focused integration checks. Do not restore the retired `lenso-release`
runtime or a shared release channel to coordinate the two package streams.

## Release boundary

The release workflows have no push-to-`main` trigger and no package publication
path in this rollout. Maintainers may dispatch the read-only inspections after
review, but dispatching them does not approve a release. Restoring release PR
creation, registry publication, OIDC/environment identities, or version/tag
creation requires a separately reviewed and authorized change.

## Engine extraction

Engine owns the portable catalog and its Wasm/package verification. The CLI
consumes released Engine crates from crates.io. Before publication, preserve the
source-closure lock and exact evidence map with the candidate receipt. Publish
changed Engine dependencies before regenerating a registry-sourced CLI lockfile
or releasing the CLI. The normal Cargo package gate then validates that registry
closure; npm binary publication uses the same reviewed source and lockfile.
