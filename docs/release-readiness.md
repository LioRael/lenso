# Release Readiness

This checklist defines the first local release gate for the Lenso backend
workspace. It is meant to answer one practical question: can this workspace be
built and checked without extra product decisions?

## Release Gate

Run the local gate before tagging or cutting a release branch:

```sh
just release-check
```

`release-check` runs the backend repository quality gate:

- Rust formatting checks.
- Rust workspace compile check.
- Rust workspace tests.
- Contract and TypeScript SDK regeneration checks.
- Architecture guardrails.
- TypeScript SDK check.

If this command fails, treat the failure as release blocking unless it is a
documented local infrastructure issue.

Runtime Console checks run in the sibling `lenso-runtime-console` repository.

## Package Gate

Run the package preflight before publishing npm or crates.io artifacts:

```sh
just package-readiness
```

This verifies the backend-owned npm package tarball, verifies the public
`lenso` facade crate package, and keeps internal Rust workspace crates
non-publishable. The detailed package and examples split checklist lives in
[package-readiness.md](package-readiness.md).

## Local Smoke

Use this sequence for a manual service smoke:

```sh
just install
cp .env.example .env
just db-up
just migrate
just api
just worker
just console-api
```

In a separate shell, verify the installable remote module path:

```sh
pnpm --dir ../lenso-runtime-console demo:release
```

The release demo starts the internal `hello-action` fixture, reads its manifest,
checks schema-admin, HTTP route, runtime function behavior, installs the manifest
into a host fixture, and verifies local `REMOTE_MODULES` plus the install plan.
User-facing examples that install published packages live in
[LioRael/lenso-examples](https://github.com/LioRael/lenso-examples).

The manual first-user flow lives in [getting-started.md](getting-started.md).

## Troubleshooting

Most release-smoke failures are local setup issues:

- Docker is not running: start Docker, then run `just db-up` again.
- Postgres is not ready: run `just db-up`, wait for the container to be healthy,
  then run `just migrate`.
- Runtime Console dependencies are missing or stale: run
  `pnpm --dir ../lenso-runtime-console install`.
- API or Console ports are busy: change `HTTP_PORT`, `CONSOLE_PORT`, or
  `VITE_API_BASE_URL` for that shell.
- The remote module manifest URL does not respond: start the module process and
  open `/lenso/module/v1/manifest` in a browser or with `curl`.
- `REMOTE_MODULES` changed but the module is not visible: restart the API,
  worker, and Runtime Console.
- OTLP collector is not running: unset `OTEL_EXPORTER_OTLP_ENDPOINT` for normal
  local smoke, or start it with `just observability-up`.

## First Release Scope

The first publishable scope is intentionally narrow:

- Linked modules load through the app bootstrap composition root.
- Remote modules install through `lenso module add <manifest-url>`.
- Remote module manifests can declare schema-admin, HTTP routes, runtime
  functions, and lifecycle activation jobs.
- Runtime Console integration is provided by the separate
  `lenso-runtime-console` repository.
- Generated contracts and the TypeScript SDK are committed and reproducible.

## Non-Goals For The First Release

Do not block the first release on centralized marketplace hardening:

- publisher trust;
- registry review;
- install history;
- doctor flows;
- bundle import/export;
- provenance and signature verification.

Those can return later as optional advanced tooling. The default marketplace
path stays decentralized and low-friction: users choose a manifest URL, install
it, restart services, and inspect the loaded module in the Console.

## Release Notes Inputs

Before publishing, collect:

- the commit SHA;
- `just release-check` result;
- the corresponding `lenso-runtime-console` check result, when publishing a
  coordinated backend/frontend version;
- generated artifact status from `just generated-check`;
- any known local infrastructure caveats;
- the minimum supported local stack: Rust toolchain, Node/pnpm, Docker, and
  Postgres.

Use [release-notes-template.md](release-notes-template.md) for manual notes, or
run:

```sh
LENSO_RELEASE_VERSION=v0.1.0 just release-package
```

The end-to-end release branch and GitHub Actions flow lives in
[release-process.md](release-process.md).

The release workflow runs with a Postgres service because backend checks include
DB-backed Rust integration tests.

When triggered with both publish inputs set to `false`, the workflow performs
package dry-runs only. When `publish_ts_sdk` or `publish_rust_crate` is `true`,
it requires the matching `NPM_TOKEN` or `CARGO_REGISTRY_TOKEN` repository secret
and publishes the selected backend-owned registry artifact after the same gates
pass.
