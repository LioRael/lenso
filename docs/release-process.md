# Release Process

Use this process when cutting a local Lenso release candidate or first release.

## 1. Prepare Main

Make sure `main` is clean and current:

```sh
git switch main
git status --short
```

Run the local release gate:

```sh
just release-check
```

## 2. Create A Release Branch

```sh
git switch -c release/vX.Y.Z
```

Use the version you plan to publish. Keep the branch focused on release notes,
last-mile docs, and blocking fixes only.

## 3. Package The Release

Build local release artifacts:

```sh
LENSO_RELEASE_VERSION=vX.Y.Z just release-package
```

This writes:

- `dist/release/lenso-vX.Y.Z-release-notes.md`
- `dist/release/lenso-vX.Y.Z-source.tar.gz`
- `dist/release/lenso-vX.Y.Z-artifact-readme.md`

The source archive is generated from `git archive HEAD`, so it contains committed
source files and excludes local build output, `.git`, `target/`, and `dist/`.
The Runtime Console is published separately by `lenso-runtime-console` and
installed into hosts with `lenso host update-console`.

## 4. Run The GitHub Workflow

Open the `release` workflow in GitHub Actions and trigger it with:

- `version`: `vX.Y.Z`
- `notes`: a short release summary
- `publish_rust_crate`: `false`

With `publish_rust_crate=false`, the workflow runs `just release-check`,
verifies that the release version matches the `lenso` crate metadata, runs
`just package-readiness`, dry-runs the `lenso-contracts` crates.io publish,
generates a release notes draft, and uploads the source package plus artifact
README. The workflow starts a Postgres service for DB-backed checks.

## 5. Configure Registry Secrets

Before a real registry publish, configure these repository secrets in GitHub:

- `CARGO_REGISTRY_TOKEN`: crates.io token with publish access to `lenso-contracts`.

Run the workflow once with `publish_rust_crate=false` before using the secret.
The dry-run path does not upload package versions.

## 6. Publish Packages

After the dry-run workflow passes, trigger the same workflow again with the
artifact you intend to publish:

- `version`: `vX.Y.Z`
- `notes`: the release summary
- `publish_rust_crate`: `true` to publish the staged Rust crates to crates.io

The publish path first repeats the full release and package gates, then uploads
the selected artifact. If the secret is missing, the workflow stops before
registry upload.

## 7. Verify The GitHub Release

When `publish_rust_crate=true`, the workflow publishes `lenso-contracts` to
crates.io, then creates the GitHub Release from the same commit. The release
uses the requested version as the tag, the generated release notes as the body,
and attaches the source package plus artifact README.

After the publish workflow passes, verify the release:

```sh
git rev-parse vX.Y.Z
gh release view vX.Y.Z
```

## 8. Keep The First Release Narrow

The first release should ship the installable module happy path:

```sh
lenso module install <manifest-url>
```

Do not block this release on centralized marketplace features such as publisher
trust, registry review, install history, doctor flows, bundle import/export,
provenance, or signatures.
