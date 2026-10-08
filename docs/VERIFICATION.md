# Verification — 2026-10-08

Current repository: `/Users/leosouthey/Projects/framework/lenso`. The naming follow-up moved the standalone repository from its former `framework/lenso-ts` directory after commit `6fae8cc`; that former name is retained only in historical evidence. Local working branch: `feat/typescript-v1`. No remote configured, push, main merge or publication. Existing Rust/UI repositories, toolchains and credentials were not modified.

The user's latest instruction cancelled Console for this phase. Final scope is SDK + CLI/TS Engine + optional Web + greeting service. No packages/console, React, Vite, UI, screenshot delivery or Console-specific generation remains. Earlier files created by this task are preserved outside this repository at `/Users/leosouthey/Documents/Codex/2026-10-08/task-5/console-deferred`.

## Initial slice checks before path migration

The following checks and measurements were run at the former TypeScript path. Raw evidence is preserved unchanged; it is not presented as a rerun at the new path. See [path migration verification](PATH-MIGRATION.md) for fresh checks.

| Check | Result / evidence |
| --- | --- |
| `bun install --frozen-lockfile` at the former path | Passed. Single root bun.lock; reduced fresh install used 28 packages; no Console/React/Vite in dependency graph. |
| `bun run build` | Passed, 4 workspace packages/tasks including private greeting example. SDK, CLI and Web JS + .d.ts outputs; example server output. |
| `bun run typecheck` | Passed all 4 packages and integration scripts. |
| `bun run test` | 21 tests, 0 failures, 55 assertions: SDK 11/32, Engine 7/17, Web 2/4, example 1/2. |
| SDK lifecycle | Duplicate/missing/cyclic dependency and exact-instance diagnostics, multiple instance identities, LIFO async cleanup, failing plugin resource rollback, collected cleanup errors, repeated/concurrent stop, declared dependency access and stopped-state rejection. |
| Built CLI | `bun packages/cli/dist/bin.js call greeting greet '{"name":"Ada"}' --root examples/greeting`: `Hello, Ada!`, count 1. See [cli.json](../output/cli.json). Business-only config excludes Web. |
| `bun run dev` | Passed before path migration; Bun server bound 127.0.0.1:3000, fresh process supervision and generated typed client. No Console/Vite listener. |
| Real generated typed HTTP client | `bun run client Ada`: `Hello, Ada!`, count 1, actual `greeting`, `web`, `http-listener` states ready. See [http-success.log](../output/http-success.log). |
| Expected HTTP error | `bun run client x`: `BAD_REQUEST`, 'Name must contain at least 2 characters', exit 1. This expected error is a passed check, not an unresolved failure. See [http-error.log](../output/http-error.log). Subsequent count 2 proves rejected input did not increment. |
| Business edit feedback | Source `Hello` → `Welcome`: new response in 517 ms; PID 64496 → 64517. Restore: 412 ms; PID 64524. Counter reset to 1, old process gone, exactly one listener. Source restored byte-for-byte. See [feedback.json](../output/feedback.json). These are one local observation, not a benchmark or latency guarantee. |
| Shutdown | Graceful supervisor stop; final server PID 64524 and supervisor PID 64483 gone; port 3000 has no listener. See [shutdown.json](../output/shutdown.json). |
| Actual package consumers | `bun run smoke`: packs SDK/CLI/Web with `bun pm pack`; workspace ranges converted; installs into fresh temporary directories. Built CLI runs with only core/CLI installed and no oRPC/React/Zod. Full consumer uses packaged Web for real typed HTTP and TypeScript rejects numeric `name`. Browser client builds to 48,427 bytes with no server lifecycle/Fetch handler code. See [package-smoke.json](../output/package-smoke.json). |
| Independent auth composition | Standalone oRPC middleware accepts/rejects a local test bearer token; no auth policy in SDK. Included in the 2 Web tests. |

Raw install/build/typecheck/test output: [verification.log](../output/verification.log). Package archive SHA-256 values: [package-sha256.txt](../output/package-sha256.txt). Verified archives remain locally under `output/packages` (ignored reproducible artifacts). Consumers remain at `/var/folders/hp/q9psfx3j2l58mrp6g7d8x8000000gn/T/lenso-package-smoke-RHkeBA`.

The temporary consumers use explicit local tarball overrides because these versions are not published. They do not use workspace aliases or source imports. Core runtime remains dependent on Effect; the 4.75 KB SDK entry size excludes external dependencies. No runtime footprint or performance advantage is claimed.

## Recovered implementation issues

Initial Web typechecks found oRPC headers/prefix type mismatches; fixed and verified. Engine generated .tsx suffixes before Console cancellation; generated imports now omit TS/JS extensions. Unpublished transitive versions initially attempted registry resolution; temporary consumers now explicitly override them with the actual local archives. Final checks have no remaining failures.

## Deferred and not validated

Console is cancelled and belongs to a separate future repository. Workers, databases/Drizzle/PG/D1, streaming and request cancellation guarantees, production auth/accounts, Rust, AI Relay migration, deployment and performance comparisons were not run or claimed. State is memory only. Plugins are trusted in-process code, not a sandbox. Dev has a five-second owned-child shutdown fallback and no state migration. Framework package edits require rebuilding outputs; the watcher covers example source/config only.

No remaining implementation blocker for this first slice. Local HTTP tests required execution outside the default tool sandbox, which otherwise denied listener creation; automatic review approved those safe loopback checks. No credential or permission settings were changed.
