# Release Readiness

This checklist defines the first local release gate for Lenso. It is meant to
answer one practical question: can this workspace be built, checked, and run
through the current remote-module happy path without extra product decisions?

## Release Gate

Run the local gate before tagging or cutting a release branch:

```sh
just release-check
```

`release-check` runs the full repository quality gate and the remote module
release demo:

- Rust and Runtime Console formatting checks.
- Rust workspace compile check.
- Rust workspace tests.
- Contract and TypeScript SDK regeneration checks.
- Architecture guardrails.
- TypeScript SDK check.
- Runtime Console format, lint, test, typecheck, and build.
- Remote module manifest/install/run demo through the `hello-action` example.

If this command fails, treat the failure as release blocking unless it is a
documented local infrastructure issue.

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
just demo-release
```

The release demo starts the `hello-action` example module, reads its manifest,
checks schema-admin, HTTP route, runtime function behavior, installs the manifest
into a host fixture, and verifies local `REMOTE_MODULES` plus the install plan.

The manual first-user flow lives in [getting-started.md](getting-started.md).

## Troubleshooting

Most release-smoke failures are local setup issues:

- Docker is not running: start Docker, then run `just db-up` again.
- Postgres is not ready: run `just db-up`, wait for the container to be healthy,
  then run `just migrate`.
- Frontend dependencies are missing or stale: run `just install`.
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
- Runtime Console shows loaded modules, remote calls, runtime functions, and
  lifecycle activation declarations.
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
