# Release Process

This repository releases its own packages. Cargo and npm have separate
version streams; there is no repository-wide release plan, shadow registry, or
central publisher.

## Cargo packages

Release-plz runs on pushes to `main` in two jobs:

1. `release-pr` opens or updates a release pull request for changed public
   workspace crates.
2. `release` publishes the versions from a merged release pull request through
   crates.io Trusted Publishing and creates one package tag and GitHub Release
   per crate.

The workflow uses the existing crate tag convention, `<crate>@<version>`. The
crates.io registry is the source of truth for already published versions; an
older shadow candidate is never treated as a public release. Configure a
crates.io Trusted Publisher for each existing crate before merging the first
Release-plz publish PR. New crates still require their first publication to be
established according to crates.io's current onboarding rules.

The release job uses the repository `RELEASE_PLZ_TOKEN` secret because this
repository's immutable tag ruleset protects package tags from the default
GitHub Actions token. The token is used only for GitHub tag and release
metadata; crate publication still uses crates.io Trusted Publishing through
the workflow's OIDC identity.

When migrating a repository whose manifest is ahead only because of an old
shadow candidate, align that manifest and its workspace lock with the latest
public registry version first. The next Release-plz PR then carries the normal
SemVer bump without rewriting any public tag or archive.

Local checks:

```sh
just check
cargo package --locked -p lenso --allow-dirty
cargo publish --dry-run --locked -p lenso --allow-dirty
```

Run the package command for the changed crate set, not for unrelated workspace
tools or fixtures.

## npm packages

The TypeScript workspace uses Changesets:

```sh
pnpm changeset
pnpm --dir sdk/typescript check
```

The Changesets workflow opens a version pull request. Merging it publishes the
changed public npm packages through npm Trusted Publishing from
`.github/workflows/release-changesets.yml`. Configure the npm Trusted Publisher
for `@lenso/service-kit` with this repository and workflow before the first
publish. No long-lived npm token is part of the default path.

## Cross-repository compatibility

The framework, auth, organization, audit-log, CLI, and Console repositories
may release on different days. Compatibility is proven by SemVer requirements,
machine-readable contracts, consumer dependency updates, and focused
integration checks. A tested multi-repository combination is evidence, not a
shared version or central release object.

## Release hygiene

Existing public versions, tags, and changelog entries remain authoritative and
are not rewritten. A release pull request must contain the exact source commit
and package versions it intends to publish. Verify registry metadata and the
published package archive after the workflow completes. Do not publish from a
dirty working tree or bypass the repository workflow with a manually selected
version.
