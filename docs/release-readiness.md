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
install-to-run demo:

- Rust and Runtime Console formatting checks.
- Rust workspace compile check.
- Rust workspace tests.
- Contract and TypeScript SDK regeneration checks.
- Architecture guardrails.
- TypeScript SDK check.
- Runtime Console format, lint, test, typecheck, and build.
- Remote module package/install/run demo.

If this command fails, treat the failure as release blocking unless it is a
documented local infrastructure issue.

## Local Smoke

Use this sequence for a manual service smoke:

```sh
just install
just db-up
just migrate
just api
just worker
just console-api
```

In a separate shell, verify the installable remote module path:

```sh
just remote-module-run-demo
```

The remote module demo scaffolds a module package, starts its backend, installs
the manifest into a host fixture, applies the console-package plan, and verifies
schema-admin, HTTP route, runtime function, and install-to-run behavior.

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
