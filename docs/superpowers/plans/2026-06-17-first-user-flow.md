# First User Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the first-user path reproducible: create a host project, add app-owned business routes, install a remote module, run API/worker, and verify the Console-facing backend surfaces.

**Architecture:** Keep the current split. `crates/lenso` stays the small public declaration facade, `crates/lenso-host` remains the transitional host boot facade, linked app code stays app-owned in the starter, and remote modules keep using `lenso module add <manifest-url>` plus host-owned proxy/runtime behavior. Do not add marketplace trust, hot reload, or a CRUD framework in this pass.

**Tech Stack:** Rust 2024, Cargo, Axum, SQLx, `just`, shell smoke scripts, sibling `../lenso-runtime-console` CLI for future `lenso host init`.

---

## File Map

- Modify `templates/starter-host/Cargo.toml`: pin `lenso-host` to an explicit release ref before external release.
- Modify `templates/starter-host/README.md`: make the supported flow and restart points exact.
- Create `scripts/starter-host-check.sh`: compile/test the starter against local workspace code without touching the template dependency.
- Modify `justfile`: add `starter-check`.
- Modify `docs/getting-started.md`: make the first-user path point at the checked starter and install commands.
- Modify `docs/release-readiness.md`: include the starter smoke in release readiness.
- Defer `lenso host init <dir>` until the starter template has a single
  distribution source instead of copying template files into the CLI package.

## Task 1: Add A Starter Compile Gate

**Files:**
- Create: `scripts/starter-host-check.sh`
- Modify: `justfile`

- [x] **Step 1: Add the smoke script**

Create `scripts/starter-host-check.sh`. It copies the starter into a temporary
directory, rewrites the `lenso-host` Git dependency to the local
`crates/lenso-host` path for validation, then runs `cargo check --bins` and
`cargo test --lib` against the temporary copy.

- [x] **Step 2: Make it executable**

Run:

```sh
chmod +x scripts/starter-host-check.sh
```

- [x] **Step 3: Wire it into `just`**

Add to `justfile`:

```make
starter-check:
    sh scripts/starter-host-check.sh
```

- [x] **Step 4: Verify**

Run:

```sh
just starter-check
```

Expected: starter binaries compile and starter module unit tests pass.

- [ ] **Step 5: Commit**

```sh
git add scripts/starter-host-check.sh justfile
git commit -m "test(starter): add host template smoke check"
```

## Task 2: Pin The Starter Dependency For Release

**Files:**
- Modify: `templates/starter-host/Cargo.toml`
- Modify: `docs/getting-started.md`
- Modify: `docs/release-readiness.md`
- Modify: `scripts/verify-release-version.sh`

- [x] **Step 1: Replace branch dependency before release**

Change `templates/starter-host/Cargo.toml` from:

```toml
lenso-host = { git = "https://github.com/LioRael/lenso", branch = "main", package = "lenso-host" }
```

to a release tag after the tag exists, or to a known-good commit while the
starter remains transitional:

```toml
lenso-host = { git = "https://github.com/LioRael/lenso", rev = "0c0efd2b5d8d8a94762951e1d8274373363ccfee", package = "lenso-host" }
```

`v0.1.0` predates `lenso-host`, so do not pin the starter to that tag.

- [x] **Step 2: Add a release guard**

Append this check to `scripts/verify-release-version.sh`:

```sh
if grep -R 'branch = "main"' templates/starter-host/Cargo.toml >/dev/null; then
  echo "starter host must not depend on branch = main for release" >&2
  exit 1
fi
```

- [x] **Step 3: Document the rule**

In `docs/getting-started.md`, keep the local template flow but state:

```md
Release templates pin `lenso-host` to a tag or commit. Local development may
temporarily use `branch = "main"`, but release branches must not.
```

In `docs/release-readiness.md`, add `just starter-check` to the release smoke list.

- [x] **Step 4: Verify**

Run:

```sh
just starter-check
just release-version-check
```

Expected: both pass.

- [ ] **Step 5: Commit**

```sh
git add templates/starter-host/Cargo.toml docs/getting-started.md docs/release-readiness.md scripts/verify-release-version.sh
git commit -m "chore(starter): pin host dependency for release"
```

## Task 3: Keep `lenso-host` Small And Explicit

**Files:**
- Modify: `docs/architecture/framework-public-surface.md`
- Test: `crates/lenso-host/src/lib.rs`

- [x] **Step 1: Confirm the exported surface remains narrow**

The existing `prelude_exports_host_authoring_types` regression test continues to
cover the transitional exported surface without adding repository helpers, CRUD
helpers, session abstractions, or a module factory.

- [x] **Step 2: Clarify the public surface doc**

In `docs/architecture/framework-public-surface.md`, keep this policy explicit:

```md
`lenso-host` is a pressure-test facade. Promote only boot helpers and linked HTTP
authoring helpers that survive the starter flow; app-owned SQL and CRUD code stay
in the starter.
```

- [x] **Step 3: Verify**

Run:

```sh
cargo test --locked -p lenso-host
```

Expected: `prelude_exports_host_authoring_types` passes.

- [ ] **Step 4: Commit**

```sh
git add docs/architecture/framework-public-surface.md
git commit -m "docs(host): keep starter facade boundary explicit"
```

## Task 4: Add A First-User Backend Smoke

**Files:**
- Create: `scripts/first-user-smoke.sh`
- Modify: `justfile`
- Modify: `docs/getting-started.md`

- [x] **Step 1: Add the smoke script**

Create `scripts/first-user-smoke.sh`. It starts local Postgres, runs
migrations, starts the remote module fixture, API, and worker on smoke-specific
defaults (`FIRST_USER_SMOKE_HTTP_PORT` defaults to `3300`,
`FIRST_USER_SMOKE_REMOTE_MODULE_ADDR` defaults to `127.0.0.1:4107`), then
verifies the remote proxy call, `/admin/data/modules`, and
`/admin/runtime/remote-proxy-calls`.

- [x] **Step 2: Make it executable**

Run:

```sh
chmod +x scripts/first-user-smoke.sh
```

- [x] **Step 3: Wire it into `just`**

Add:

```make
first-user-smoke:
    sh scripts/first-user-smoke.sh
```

- [x] **Step 4: Verify**

Run:

```sh
just first-user-smoke
```

Expected: health, remote proxy, module metadata, and remote call list endpoints all return success.

- [ ] **Step 5: Commit**

```sh
git add scripts/first-user-smoke.sh justfile docs/getting-started.md
git commit -m "test: add first user backend smoke"
```

## Task 5: Make Module Install UX Honest

**Files:**
- Modify: `docs/getting-started.md`
- Modify: `docs/release-readiness.md`
- Later modify: `../lenso-runtime-console/packages/console-package-cli/src/remote-module.ts`
- Later modify: `../lenso-runtime-console/packages/console-package-cli/src/index.test.ts`

- [x] **Step 1: Keep the install contract short**

Document this as the supported install path:

```sh
lenso module add http://127.0.0.1:4100/lenso/module/v1/manifest
lenso console-package apply-plan
```

Then state:

```md
Restart API, worker, and Runtime Console after module install because remote
sources and console package exports are loaded at process startup.
```

- [x] **Step 2: Add CLI output in the sibling repo**

In `../lenso-runtime-console/packages/console-package-cli/src/remote-module.ts`, ensure successful `module add` prints:

```text
Added remote module <name>.
- lenso console-package apply-plan
- pnpm install
- restart Runtime Console after applying the plan
- restart the API and worker
```

- [x] **Step 3: Test CLI output in the sibling repo**

In `../lenso-runtime-console/packages/console-package-cli/src/index.test.ts`, assert the output contains:

```ts
expect(output).toContain("lenso console-package apply-plan");
expect(output).toContain("restart Runtime Console after applying the plan");
expect(output).toContain("restart the API and worker");
```

- [x] **Step 4: Verify**

Run:

```sh
pnpm --dir ../lenso-runtime-console exec vitest run packages/console-package-cli/src/index.test.ts
pnpm --dir ../lenso-runtime-console check
```

Expected: console-package CLI tests and repo check pass.

- [ ] **Step 5: Commit**

Commit backend docs in `/Users/leosouthey/Projects/framework/lenso`. Commit CLI
changes separately from `/Users/leosouthey/Projects/framework/lenso-runtime-console`.

## Task 6: Add `lenso host init` After The Starter Is Stable

Deferred deliberately. The starter is now verified, but shipping `lenso host
init` from the sibling CLI needs a template distribution decision. Duplicating
`templates/starter-host` into the CLI package would create two starter sources
of truth; add this command when the template is bundled or moved intentionally.

**Files:**
- Later create: `../lenso-runtime-console/packages/console-package-cli/src/host.ts`
- Later modify: `../lenso-runtime-console/packages/console-package-cli/src/index.ts`
- Later modify: `../lenso-runtime-console/packages/console-package-cli/src/index.test.ts`

- [ ] **Step 1: Add the command only after Tasks 1-5 pass**

Add to CLI help:

```ts
program
  .command("host")
  .description("create and manage Lenso host projects");
```

Add subcommand:

```ts
hostCommand
  .command("init <directory>")
  .description("create a starter Lenso host project")
  .option("--name <name>", "Cargo package name")
  .option("--dry-run", "print files without writing them")
  .action(async (directory: string, options: CliOptions) => {
    await initHost({ directory, options });
  });
```

- [ ] **Step 2: Keep init boring**

`initHost` should copy the starter template, replace package name, and stop. It must not install dependencies, start Docker, create accounts, or scaffold product CRUD beyond the existing starter `app` module.

- [ ] **Step 3: Test host init**

Add a test that runs:

```ts
await runConsolePackageCli(["host", "init", appDir, "--name", "acme-host"]);
```

Assert:

```ts
expect(await pathExists(path.join(appDir, "Cargo.toml"))).toBe(true);
expect(await readText(path.join(appDir, "Cargo.toml"))).toContain('name = "acme-host"');
expect(await pathExists(path.join(appDir, "src/modules/app/routes.rs"))).toBe(true);
```

- [ ] **Step 4: Verify**

Run:

```sh
pnpm --dir ../lenso-runtime-console exec vitest run packages/console-package-cli/src/index.test.ts
pnpm --dir ../lenso-runtime-console check
```

- [ ] **Step 5: Commit**

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
git add packages/console-package-cli/src/index.ts packages/console-package-cli/src/host.ts packages/console-package-cli/src/index.test.ts
git commit -m "feat(cli): add host init command"
```

## Final Acceptance

- `just starter-check` passes.
- `just first-user-smoke` passes.
- `cargo test --locked -p lenso-host` passes.
- Focused remote-module tests pass:

```sh
cargo test --locked -p app-api --test remote_module_smoke
cargo test --locked -p app-api --test runtime_console service_actor_can_list_remote_proxy_calls
cargo test --locked -p app-api --test admin_data_console available_modules_reports_local_install_state
```

- `docs/getting-started.md` describes the same flow the smoke scripts run.
- The starter dependency is pinned to a tag or commit before release.

## Deliberate Skips

- No marketplace trust, signatures, registry review, provenance, payments, ratings, or install history.
- No hot-loading installed modules without restart.
- No generic CRUD framework in `lenso-host`.
- No `lenso` crate `host` feature until `lenso-host` has survived the starter flow and can be made publishable without leaking internal crates.
