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
git switch -c release/v0.1.0
```

Use the version you plan to publish. Keep the branch focused on release notes,
last-mile docs, and blocking fixes only.

## 3. Package The Release

Build local release artifacts:

```sh
LENSO_RELEASE_VERSION=v0.1.0 just release-package
```

This writes:

- `dist/release/lenso-v0.1.0-release-notes.md`
- `dist/release/lenso-v0.1.0-source.tar.gz`
- `dist/release/lenso-v0.1.0-artifact-readme.md`

The source archive is generated from `git archive HEAD`, so it contains committed
source files and excludes local build output, `.git`, `target/`, `node_modules/`,
and `dist/`.

## 4. Run The GitHub Workflow

Open the `release` workflow in GitHub Actions and trigger it with:

- `version`: `v0.1.0`
- `notes`: a short release summary
- `publish_ts_sdk`: `false`
- `publish_rust_crate`: `false`

With both publish inputs set to `false`, the workflow runs `just release-check`,
verifies that the release version matches the selected package metadata, runs
`just package-readiness`, dry-runs the crates.io publish, generates a release
notes draft, and uploads the source package plus artifact README. The workflow
starts a Postgres service for DB-backed checks.

## 5. Configure Registry Secrets

Before a real registry publish, configure these repository secrets in GitHub:

- `NPM_TOKEN`: npm token with publish access to `@lenso/ts-sdk`, only needed
  when `publish_ts_sdk=true`.
- `CARGO_REGISTRY_TOKEN`: crates.io token with publish access to `lenso`.

Run the workflow once with both publish inputs set to `false` before using the
secrets. The dry-run path does not upload package versions.

## 6. Publish Packages

After the dry-run workflow passes, trigger the same workflow again with the
artifact you intend to publish:

- `version`: `v0.1.0`
- `notes`: the release summary
- `publish_ts_sdk`: `false` by default; set `true` only when a real API-client
  consumer needs a new `@lenso/ts-sdk` version
- `publish_rust_crate`: `true` to publish `lenso@0.1.0` to crates.io

The publish path first repeats the full release and package gates, then uploads
only the selected artifacts. If a selected artifact's secret is missing, the
workflow stops before registry upload.

## 7. Tag And Publish The GitHub Release

After the publish workflow passes, check whether the tag already exists:

```sh
git rev-parse v0.1.0
```

If the tag already exists, do not move it. For the current `v0.1.0` line,
`@lenso/ts-sdk@0.1.0` was published before the Rust facade crate existed, so a
crate-only follow-up publish should be recorded from the workflow run and
crates.io package page instead of rewriting the tag.

For future coordinated versions where the tag does not exist yet:

```sh
git tag v0.1.0
git push origin v0.1.0
```

Create a GitHub Release from the tag, paste the generated release notes draft,
and attach the source package artifact plus artifact README.

## 8. Keep The First Release Narrow

The first release should ship the installable module happy path:

```sh
lenso module add <manifest-url>
```

Do not block this release on centralized marketplace features such as publisher
trust, registry review, install history, doctor flows, bundle import/export,
provenance, or signatures.
