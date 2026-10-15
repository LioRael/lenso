# Recipe verification — 2026-10-15

Verified on macOS with Bun 1.4.3 and Node 26.10.0. A fresh local clone had no
`node_modules`, framework `dist`, or prepared SDK workspace. The pending recipe
source changes were copied into it before running the documented commands.
The package/download caches were warm; these are local measured times, not cold
network install benchmarks. No root lockfile or dependency version changed.

| Command from checkout root                                                                                         | Wall time | Result                                      |
| ------------------------------------------------------------------------------------------------------------------ | --------: | ------------------------------------------- |
| `bun run prepare:runtime`                                                                                          |  1,893 ms | pinned SDK prepared                         |
| `bun install --frozen-lockfile`                                                                                    |    487 ms | passed                                      |
| `bunx turbo run build --filter=@lenso/example-notes --filter=@lenso/example-tasks`                                 |  1,492 ms | passed                                      |
| `bun run --cwd examples/notes typecheck`                                                                           |    432 ms | passed                                      |
| `bun run --cwd examples/tasks typecheck`                                                                           |    343 ms | passed                                      |
| `bun run --cwd examples/notes recipes:inspect`                                                                     |     95 ms | explicit providers/routes/dependencies only |
| `bun run --cwd examples/notes recipes:demo`                                                                        |    139 ms | loopback HTTP; measured flow 51 ms          |
| `bun test examples/notes/test/recipes.test.ts examples/notes/test/files.test.ts examples/tasks/src/recipe.test.ts` |    229 ms | 8 passed, 124 assertions                    |

The final server was started through the documented `lenso dev --config
recipes.config.ts --entry src/recipes/server.ts` command. Engine reported ready in
847 ms; readiness plus an actual HTTP login completed in 878 ms. SIGINT exited 0.
The first verification harness incorrectly expected a `[lenso:ready]` text marker;
the CLI actually prints `Ready in`. The harness was corrected; no product change
was needed. Both owned server processes and temporary data were cleaned up.

The real PostgreSQL background-task fixture separately passed producer stop,
fresh worker execution and fresh reader access (116 ms test case / 210 ms test
process). A new local database migrated in 0.16 s; the task demo ran in 0.16 s and
returned `succeeded`, `sum: 16`, `count: 3`. The broader Tasks suite had 18 passes
and 4 environment-dependent skips; the new PostgreSQL opt-in test was explicitly
run, not counted as a skip. The owned PostgreSQL process was stopped and its
fixture removed.

`bun test packages/storage/test/r2.test.ts` passed all 3 tests / 25 assertions in
338 ms, including the existing native workerd R2 binding test (237 ms). This proves
the provider's local binding behaviour, not a deployed Auth + D1 + R2 application.
No cloud credentials, resources or permissions were used.

Coverage includes exact Auth/DB instance reuse, single-owner cleanup, private-file
persistence across restart, owner checks on GET/HEAD/DELETE, missing and revoked
evidence, upload limits, bounded login input, omitted HTTP entries, and missing
plugin dependencies. Inspection was verified not to read a throwing Config source
or create its configured database. It does not establish migration or network
health. The fixture login and task credentials are synthetic and never printed.
