---
name: land
description: >-
  Land requested Lenso TypeScript changes onto local main after verification.
  Invoke only when the user explicitly requests landing or merging changes,
  never merely for review, preparation, passing checks, or skill installation.
disable-model-invocation: true
metadata:
  delta-action: land
---

# Land

Complete the explicit landing request in the assigned Lenso checkout. The request
already authorizes the local merge; proceed without asking for the same permission
again. Default destination is local `main`. Honor a different explicitly requested
local destination only after confirming its identity and applicable policy.

Scope is local Git integration. Preserve the source branch. Pushes, including to
the `local` backlink, package publication, deployment, shared-history rewrites and
changes to Git/signing settings require separate authorization. Do not operate in
another checkout.

## 1. Establish the change and destination

Read `AGENTS.md`, applicable nested instructions, and any contribution, submission,
CI or landing policy present in the current source and destination. Repository
policy remains binding even when hosting tools do not enforce it. Apply conditional
review, signing, authorship, changelog and submission requirements only when their
conditions hold; obtain any required human-authored material rather than generating
it and treating approval as authorship.

Inspect the current state:

```sh
git --no-optional-locks status --short
git branch --show-current
git remote -v
git worktree list --porcelain
git log -8 --oneline
git rev-parse --verify refs/heads/main
```

Use the selected destination instead of `main` when one was explicitly requested.
Resolve the requested source from the current branch and conversation; stop if
scope is ambiguous. Record source and destination commit IDs. Inspect their diff,
including uncommitted changes. The `local` remote identifies the user's primary
repository, not a publication destination; this workflow does not update it.

Commit only requested uncommitted changes, using explicit paths and a noninteractive
message. Preserve the configured signing mechanism; if signing needs unavailable
authentication, stop rather than disabling it. Do not amend or rewrite existing
shared commits. If unrelated work cannot be safely separated, stop and explain what
must be isolated. Do not discard or automatically stash unrelated changes.

**Done:** the requested source is a pinned local commit, the destination is a known
local branch, applicable obligations are identified, and the checkout is safe to
switch without disturbing unrelated work.

## 2. Form a candidate without advancing the destination

If the pinned source is already an ancestor of the destination, verify that the
requested changes are present and report that they are already landed.

Otherwise create a uniquely named local integration branch from the pinned
destination. Keep its recovery name and commit IDs available.

- If the destination is an ancestor of the source, fast-forward the integration
  branch to the pinned source with `GIT_EDITOR=true git merge --ff-only <source>`.
- If histories diverged, use
  `GIT_EDITOR=true git merge --no-ff --no-commit <source>`, then create a merge commit
  with `GIT_EDITOR=true git commit -m "<merge summary>"`. Preserve Git's configured
  signing. Use concrete recorded IDs and branch names, not the placeholders.

Resolve conflicts automatically when the intended combined behavior is clear from
the source, destination and specification. Preserve unrelated destination changes.
For ambiguous intent, unsafe resolution, or an unresolved permission requirement,
stop and report that landing has not completed. Do not force a result or invoke an
interactive editor. Recheck the whole resolved diff, not just conflicted lines.

**Done:** one clean candidate commit contains the requested source and destination
work; the destination branch still points to its recorded commit.

## 3. Verify the exact candidate

Read the candidate's manifests and task definitions before choosing checks. Use
the declared Bun version, currently `1.4.2`, and the single root lockfile. Confirm
the version with `bun --version`, then run `bun install --frozen-lockfile`; do not
repair a failed frozen install by silently rewriting the lockfile.
Sources: `package.json` (`engines`, `packageManager`, `workspaces`), `mise.toml`,
`AGENTS.md`.

Use installed workspace tools after that install. Build affected framework packages
and their dependencies before consumers, because package exports resolve to `dist`.
Choose affected packages and consumers from the diff and their actual manifest
dependencies. For the Engine/CLI/greeting change family, the verified tasks are:

```sh
./node_modules/.bin/turbo run build --filter=lenso-cli --filter=@lenso/workers --filter=@lenso/example-greeting
./node_modules/.bin/turbo run typecheck --filter=@lenso/engine --filter=lenso-cli --filter=@lenso/example-greeting
bun run lint
bun test packages/engine/test packages/cli/test examples/greeting/src/greeting.test.ts
```

Sources: root `package.json` (`build`, `typecheck`, `lint`, pinned Turbo);
`turbo.json` (`build` and `typecheck` dependency ordering);
`packages/engine/package.json`, `packages/cli/package.json`,
`packages/workers/package.json`, `packages/web/package.json`,
`examples/greeting/package.json` (package names, scripts and dependencies);
`packages/engine/test/`, `packages/cli/test/`,
`examples/greeting/src/greeting.test.ts` (focused test entry points).
Turbo's `run --filter` option and Bun's test path arguments are supported by the
configured tool versions; consult their read-only `--help` if versions change.

Engine or CLI implementation, exports, dependencies, bundling or plugin-protocol
changes require the real packaged-consumer test:

```sh
bun test packages/cli/test/packaging.test.ts
```

It is already included by the focused suite above; do not run it twice. Its required
framework builds, real tarballs, standalone installs, typed API checks, external
plugins, bundled CLI dev and cleanup are defined in
`packages/cli/test/packaging.test.ts`. Workspace source resolution is not a
substitute. If this test is removed or replaced, identify its authoritative
replacement or stop rather than dropping packaged-consumer coverage.

For other changes, substitute the affected packages' actual declared build,
typecheck and test tasks, plus dependent consumers. Use root `bun run build`,
`bun run typecheck` and `bun run test` when cross-cutting changes require their
full scope; their definitions are in root `package.json` and `turbo.json`.
Do not invent task names or run package publishing/deployment as verification.
Documentation- or skill-only changes need content/link/frontmatter validation and
diff checks, not unrelated runtime builds.

For service input, CLI exposure, diagnostics or generated-output changes, follow
`docs/CLI.md`. When greeting's declared operation is affected, run:

```sh
bun run cli inspect greeting greet --root examples/greeting --json
printf '{"name":"Ada"}\n' | bun run cli call greeting greet --root examples/greeting --stdin --json
```

Sources: root `package.json` (`cli` script); `packages/cli/src/bin.ts`
(`inspect`, `call`, `--root`, `--stdin`, `--json` handling);
`examples/greeting/lenso.config.ts` (declared operation);
`examples/greeting/src/contracts.ts` (shared input schema).

Exercise affected real workflows and inspect reproducible output for unintended
changes. Keep generated/cache directories framework-owned. Register and clean up
only processes, ports and temporary consumers owned by this verification.
Sources: `AGENTS.md`, `docs/CLI.md`, `.gitignore`.

Database integration runs require an explicitly confirmed disposable database;
do not infer safety from a connection-string variable merely being present.
If a required check needs missing infrastructure or credentials, stop and report
the blocker rather than treating a skip as success.
Sources: `README.md` (development boundaries), `docs/DATABASE.md`,
`examples/notes/test/postgres.test.ts`.

Run `git diff --check`, confirm a clean tracked checkout, and record the tested
candidate commit. All applicable local checks and repository-required remote
checks/reviews must pass for this exact change under the current policy. Pending,
failing, missing or unverifiable requirements block landing. This repository
currently defines no CI/PR landing requirement; recheck for newly introduced policy
instead of assuming that remains true. Do not publish a branch to obtain checks
without separate authorization.

**Done:** the candidate is unchanged from the verified commit, every applicable
required check has passed, and no required review or submission obligation remains.

## 4. Land and confirm

Recheck that the destination still points to the recorded base and is not checked
out in another worktree. If it advanced, rebuild the candidate against its new tip
and repeat applicable verification. If another worktree owns it, stop rather than
overriding ownership.

Switch to the destination in the assigned checkout and fast-forward it to the
verified candidate:

```sh
git switch <destination>
GIT_EDITOR=true git merge --ff-only <verified-candidate>
git rev-parse HEAD
git merge-base --is-ancestor <pinned-source> HEAD
git --no-optional-locks status --short
```

Replace placeholders with the recorded names and IDs. Confirm that the destination
tip equals the verified candidate and contains the requested source. A prepared
branch, created commit, started check or incomplete merge is not successful landing.
If the merge fails, report that the changes have not landed and retain recoverable
state without discarding work.

Report the destination, landed commit, source, checks actually run, and any explicit
validation limitations. Preserve the source and integration branches; perform no
push, publication or deployment as part of this local landing.
