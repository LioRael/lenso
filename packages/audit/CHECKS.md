# Audit validation

## Passed in this checkout

- Bun 1.4.2 with repository-pinned dependency versions.
- Required framework dependency builds, then `bun run --cwd packages/audit build`.
- `bun run --cwd packages/audit typecheck`.
- `bun run --cwd examples/notes typecheck` and `bun run --cwd examples/notes build`.
- `LENSO_REQUIRE_POSTGRES=1 bun test packages/audit/test`: **15 passed, 0 failed**.
  - Real Bun SQLite: reopen persistence, duplicates/conflicts, exact tenant/scope
    isolation, filters and tied-time keyset pagination.
  - Real disposable PostgreSQL: independent connections, concurrent duplicate
    inserts, JSON roundtrip, scope/filter/page checks; `fsync=on` and
    `synchronous_commit=on`; a strict intent is visible from another connection
    before continuing, and a duplicate intent never returns another receipt.
  - Actual local Miniflare/workerd D1 binding: explicit migration, insert/returning,
    duplicate/conflict lookup, tenant/scope isolation, filters and pagination.
  - Auth: genuine minted actors, copied/foreign-audience/foreign-runtime actors,
    revoked sessions, scope denial and Auth-valid opaque subject/issuer IDs.
  - Whitelisting/length limits, client identity rejection, append-only correction,
    safe failure reporting, strict admission, post-effect unknown, system subjects.
  - Exact plugin dependencies, invalid-config preflight, owned stop/rollback
    cleanup and borrowed database survival.
  - Existing Notes removal through real Manage/Engine/Auth/SQLite, including
    cross-owner denial; Audit query-only companion, scope denial, no counts,
    secret/body omission and 129-character target-ID filtering.
  - Tasks reconciliation registration: locator-only payload, per-attempt trusted
    principal, linked stable outcome and denied unauthorized job attempts.
  - Standalone root bundled without optional framework/provider imports.
- `bun test examples/notes/test/notes.test.ts examples/notes/test/operations.test.ts examples/notes/test/manage.test.ts`:
  **7 passed, 0 failed**, including real CLI inspect/call subprocesses.
- `bun packages/cli/src/bin.ts inspect notes-operations remove --root examples/notes --json`:
  existing strict business input and unchanged opt-out assembly.
- Focused `oxlint --deny-warnings`: **0 warnings, 0 errors**.
- Focused `oxfmt --check` and `git diff --check`: passed.

## Clean-install integration

After explicit authorization, the single root `bun.lock` was regenerated with
`bun install --lockfile-only --ignore-scripts`. Its changes are the Audit
workspace, Notes' Audit dev dependency, and existing Auth/Manage manifest peer
ranges that were already `^0.2.0` but stale in the previous lock. No external
dependency version or public Auth/Tasks/Manage source was changed.

A disposable non-Git source tree was copied from the current workspace without
`node_modules`, `dist` or Turbo cache. It passed:

```sh
bun install --frozen-lockfile
bun run build --filter=@lenso/audit... --filter=@lenso/example-notes... --concurrency=1 --cache=local:rw
bun run --cwd packages/audit typecheck
bun run --cwd examples/notes typecheck
LENSO_REQUIRE_POSTGRES=1 bun test packages/audit/test
bun test examples/notes/test/notes.test.ts examples/notes/test/operations.test.ts examples/notes/test/manage.test.ts
```

All **14 selected package builds** ran successfully on the first build, with
no cache hits and remote caching disabled. Audit again passed **15 tests**,
including actual PostgreSQL and local workerd D1; Notes again passed **7 tests**.
Focused lint and formatting checks also passed there. The frozen installation
left the copied lock byte-identical to the updated workspace lock.

## Not claimed or verified

- No cloud D1 deployment, replication/failover, production PostgreSQL/SQLite
  durability, power-loss recovery, or broad platform compatibility.
- No business/Audit atomic transaction or outbox integration. Notes uses
  best-effort; strict is explicit persisted-intent admission, not rollback or
  exactly-once external execution.
- No real notifications, payments, production mutations, credential provisioning,
  publishing, pushing or merging.
- Tasks registration/handler behavior was checked, **not a new durable Tasks
  provider/end-to-end queue run**. Use the existing provider and its tests;
  queue enqueue is an independent write and cannot replace a missing intent.
- No OTel SDK/exporter end-to-end run; diagnostics reuse the existing API and
  supplied logger, without acquiring or closing an SDK.
- No full-repository test/release suite, browser tests or compliance claim.

## Remaining consumer setup

Apply the selected Audit SQL baseline through the consumer's existing migration
history, supply the actual scope policy and diagnostic sink, and explicitly opt
in to the Notes Audit dependency or query companion. No default management
entry, listener, automatic migration or retry was enabled.
