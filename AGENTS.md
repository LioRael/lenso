# Lenso TypeScript

Work only in the assigned checkout. Preserve other workspaces and user changes. Local commits are allowed; push, publishing, deployment and merging into main need explicit authorization.

Core is `packages/lenso`; command semantics are `packages/cli`; Web, Auth, DB and Workers are separate optional packages. Keep business services ordinary async. Use the versions and scripts in package.json; one Bun lockfile belongs to the integration owner. Keep tests focused on changed behavior.

Implementation agents leave root lockfile integration to the integration owner. An explicit land request assigns that role to the Land agent and authorizes root `bun.lock` synchronization required by the requested changes, unless the user explicitly restricts lockfile edits during landing. Preserve unrelated changes and dependency resolutions; follow [Land](.agents/skills/land/SKILL.md) for candidate synchronization and frozen-install verification.

When changing service input, CLI exposure, diagnostics or generated files, read [CLI development](docs/CLI.md). Rebuild framework packages before running consumers: package exports resolve to dist. Source lives outside `.lenso` and `dist`; those directories are framework-owned, reproducible output. Config top-level code must avoid resource acquisition. Inspect imports trusted config but does not run setup.

For example, after an app declares `greeting.greet`: run `lenso inspect greeting greet --root <app> --json`, edit the shared schema/service, then `lenso call greeting greet --root <app> --stdin --json` with JSON input and run the related tests. Register resource cleanup immediately during setup; close only processes and ports owned by the task.

Keep request paths free of schema creation/migrations and unrelated queue initialization. Reuse a verified request identity; still evaluate each operation's scoped permission. Register persistent resources during explicit setup, not imports or config inspection.

Use the smallest affected checks first; `bun run scripts:test` and `bun run scripts:typecheck` cover CI/bootstrap/release tooling. Root test/typecheck already include them. Keep real tarball consumers, authorization/revocation, payment idempotency and durable-state integration checks. Remove only assertions with demonstrated overlapping coverage; measure real I/O and serial round trips when reporting latency.

Use the built-in browser or isolated headless sessions, never the user's Chrome. Report the tested SHA, commands and observed timings separately from merged, published or deployed state; reuse evidence only for unchanged inputs. Never weaken a required check to shorten feedback.
