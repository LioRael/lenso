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

The workflow runs `just release-check`, generates a release notes draft, and
uploads the source package plus artifact README. The workflow starts a Postgres
service for DB-backed checks.

## 5. Tag And Publish

After the release workflow passes:

```sh
git tag v0.1.0
git push origin v0.1.0
```

Create a GitHub Release from the tag, paste the generated release notes draft,
and attach the source package artifact plus artifact README.

## 6. Keep The First Release Narrow

The first release should ship the installable module happy path:

```sh
lenso module add <manifest-url>
```

Do not block this release on centralized marketplace features such as publisher
trust, registry review, install history, doctor flows, bundle import/export,
provenance, or signatures.
