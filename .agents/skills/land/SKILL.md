---
name: land
description: >-
  Land requested Lenso TypeScript changes onto main in the user's primary checkout
  after verification; updating a temporary clone does not count as landing.
  Invoke only when the user explicitly requests landing or merging changes,
  never merely for review, preparation, passing checks, or skill installation.
disable-model-invocation: true
metadata:
  delta-action: land
---

# Land

Complete the explicit landing request by updating `main` in the user's primary
checkout, not a same-named branch in a temporary source clone. Honor another
explicitly requested destination only after confirming its checkout, branch and
policy. The merge request supplies landing intent; do not ask for it again.

Operate only in authorized checkouts. If the primary destination is outside the
authorized workspace, stop and request access before changing its files, index,
objects or refs. Resolving a path does not authorize modifying it. Never substitute
a source-clone merge for an unavailable primary destination.

Scope is local Git integration, including transferring verified commits between
authorized local repositories. Preserve source branches and unrelated primary
checkout work. Remote publication, deployment, shared-history rewrites and changes
to Git/signing settings require separate authorization. Do not push into a
checked-out branch through the `local` backlink.

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
git remote get-url local
git rev-parse --absolute-git-dir
git worktree list --porcelain
git log -8 --oneline
```

Resolve the requested source from the current branch and conversation. The `local`
filesystem backlink identifies this project's primary repository. Resolve its
actual checkout and Git directory; do not treat a network URL or bare repository
as a checkout. If the backlink is missing or ambiguous, obtain the intended primary
checkout rather than guessing. If already working in the explicitly identified
primary checkout, use it directly.

Record the canonical source and primary checkout paths, their Git directories, the
destination branch (default `main`), and both commit IDs. Inspect the primary state
read-only before requesting any missing access:

```sh
git --no-optional-locks -C <primary-checkout> status --porcelain=v1
git -C <primary-checkout> rev-parse --show-toplevel
git -C <primary-checkout> rev-parse --absolute-git-dir
git -C <primary-checkout> rev-parse --verify refs/heads/<destination>
git -C <primary-checkout> worktree list --porcelain
```

Replace placeholders with resolved paths and names. Record staged and unstaged
diffs and untracked file content hashes, not just status labels, so unrelated
primary changes can be preserved and checked after landing.
Unrelated dirty work is not permission to reset, restore, stash or commit it.
Stop if the requested result cannot be applied without disturbing it.

Commit only requested uncommitted changes, using explicit paths and a noninteractive
message. Use `GIT_EDITOR=true` for every commit and merge command. Preserve the
configured signing mechanism; if signing needs unavailable
authentication, stop rather than disabling it. Do not amend or rewrite existing
shared commits. If unrelated work cannot be safely separated, stop and explain what
must be isolated. Do not discard or automatically stash unrelated changes.

**Done:** the source is a pinned commit, the primary checkout and its destination
branch are identified and authorized, applicable obligations are met, and unrelated
work can be preserved. A clone-local `main` is not the destination.

## 2. Form a candidate without advancing the destination

Compare the pinned source with the actual primary destination commit. A matching
or newer branch in the source clone proves nothing about the primary checkout.
If the requested source is already contained in the primary destination, verify
the primary state and report that it is already landed.

If repositories differ, fetch the authorized primary destination into the source
repository without updating the primary branch:

```sh
git fetch --no-tags <primary-git-directory> refs/heads/<destination>
```

Confirm `FETCH_HEAD` equals the recorded primary destination commit; if the primary
advanced, refresh the recorded base. Create a uniquely named local integration
branch from that pinned base, leaving the actual primary branch untouched. Keep
the recovery name and commit IDs available. If source and destination share one
repository, create the integration branch directly from the pinned destination.

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

**Done:** one clean candidate commit contains the requested source and primary
destination work; the primary destination still points to its recorded base.

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

Recheck the actual primary checkout's destination ref, branch ownership and
uncommitted state, not the source clone's `main`. If the destination advanced,
rebuild the candidate against its new tip and repeat applicable verification. If
unrelated primary work changed, reassess preservation before proceeding.

It is normal for `main` to be checked out in the primary checkout. If another
linked worktree owns the destination, use that owner only if it is an authorized
destination; otherwise stop and request access instead of overriding ownership.

When repositories differ, transfer the exact verified candidate into the primary
repository without updating a branch:

```sh
git -C <primary-checkout> fetch --no-tags <source-git-directory> <verified-candidate-id>
git -C <primary-checkout> rev-parse FETCH_HEAD
```

Confirm the fetched ID equals the tested candidate. Then, in the authorized
primary checkout, switch to the destination if needed and fast-forward it:

```sh
git -C <primary-checkout> switch <destination>
GIT_EDITOR=true git -C <primary-checkout> merge --ff-only <verified-candidate-id>
git -C <primary-checkout> rev-parse refs/heads/<destination>
git -C <primary-checkout> merge-base --is-ancestor <pinned-source> refs/heads/<destination>
git --no-optional-locks -C <primary-checkout> status --porcelain=v1
```

Replace placeholders with the recorded paths, names and IDs. Confirm the primary
destination tip equals the verified candidate, contains the requested source, and
its checked-out files reflect that commit without unexpected changes. Verify that
pre-existing unrelated staged, unstaged and untracked work remains intact.

Success requires this verification in the primary checkout. A source-clone merge,
prepared branch, transferred commit, started check or incomplete merge is not
successful landing. If access or the final merge is blocked, report that the
primary destination has not been updated; retain recoverable state.

Report the actual primary checkout path, destination branch, landed commit, source,
checks run and validation limitations. Preserve source and integration branches.
Perform no remote publication or deployment as part of this local landing.
