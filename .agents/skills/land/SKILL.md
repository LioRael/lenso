---
name: land
description: >-
  Verify and land requested Lenso changes onto the remote destination branch,
  directly or through the repository's required pull-request workflow.
disable-model-invocation: true
metadata:
  delta-action: land
---

# Land

Complete an explicit landing request by updating the remote destination branch
(default `main`). A local merge or a pushed feature branch is preparation, not
landing. Use the attached checkout for preparation and verification; leave the
user's primary checkout untouched.

Invoke only for an explicit landing or merging request. Editing this skill,
reviewing changes or passing checks does not authorize publication. An explicit
request to land remotely authorizes the necessary branch push and remote merge
within the confirmed scope; honor any instruction to prepare only or wait for
approval. Deployment, package publication, force-pushes, shared-history rewrites
and Git/signing configuration changes require separate authorization.

## 1. Establish the change and destination

Read `AGENTS.md`, applicable nested instructions, and any contribution, submission,
CI or landing policy present in the source and fetched destination. Repository
policy remains binding even when hosting tools do not enforce it. Apply conditional
review, signing, authorship, changelog and submission requirements only when their
conditions hold; obtain any required human-authored material rather than generating
it and treating approval as authorship.

Inspect the current state:

```sh
git --no-optional-locks status --short
git branch --show-current
git remote -v
git log -8 --oneline
```

Resolve the requested source from the current branch and conversation. Inspect
configured fetch and push URLs to identify the intended hosting repository;
do not assume `origin` or use the `local` filesystem backlink for publication.
If no publishing remote exists or the repository/destination is ambiguous, ask
for the intended remote URL and branch before configuring or publishing anything.
Confirm a differing push URL targets the intended repository.

```sh
git remote get-url <remote>
git remote get-url --push --all <remote>
git ls-remote --exit-code <remote> refs/heads/<destination>
git fetch --no-tags <remote> refs/heads/<destination>
git rev-parse FETCH_HEAD
```

Replace placeholders with confirmed names. Record the repository URL, remote,
destination branch and fetched base ID. A missing destination requires explicit
authorization to create it. Inspect hosting branch protection, required checks,
reviews and allowed merge methods using available hosting tools. If required
policy cannot be determined, stop rather than bypassing it.

Commit only requested uncommitted changes, using explicit paths and a noninteractive
message. Use `GIT_EDITOR=true` for every commit and merge command. Preserve the
configured signing mechanism; if signing needs unavailable
authentication, stop rather than disabling it. Do not amend or rewrite existing
shared commits. If unrelated work cannot be safely separated, stop and explain what
must be isolated. Do not discard or automatically stash unrelated changes.

**Done:** the source is a pinned commit, the remote repository and destination are
confirmed, publication is authorized, and the direct-push or PR route is established.

## 2. Form a candidate without advancing the destination

Compare the pinned source with the fetched remote base. If the source is already
contained in that base, confirm the live remote state and report it as already
landed. For prior squash/rebase merges, use hosting merge records and the resulting
diff rather than requiring the original source ID to be an ancestor.

Create a uniquely named integration branch from the fetched base in an authorized
clean checkout, preserving the source branch and unrelated work. If the current
checkout is dirty, use an authorized isolated workspace or stop; do not stash,
discard or commit unrelated changes. Record the recovery branch and commit IDs.

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

**Done:** one clean candidate contains the requested source and remote base work;
the remote destination has not been changed.

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
./node_modules/.bin/turbo run build --filter=@lenso/cli --filter=@lenso/workers --filter=@lenso/example-greeting
./node_modules/.bin/turbo run typecheck --filter=@lenso/engine --filter=@lenso/cli --filter=@lenso/example-greeting
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
candidate commit. All applicable local checks must pass before publication.
Repository-required remote checks/reviews must pass before the destination is
updated. Pending, failing, missing or unverifiable requirements block that update.
This repository
may define additional CI/PR requirements at execution time; recheck repository
files and hosting protection rather than assuming there are none. For a PR route,
publish the verified candidate to a uniquely named feature branch and open or
update the correctly targeted PR within the authorized scope. Wait for required
checks and reviews on the current PR head before merging. A pending PR is not
successful landing.

**Done:** the candidate is unchanged from the verified commit, every applicable
required check has passed, and no required review or submission obligation remains.

## 4. Land and confirm

Fetch the destination again immediately before landing. If it advanced, rebuild
the candidate against its new tip and repeat applicable verification. For PRs,
refresh the PR head and its checks/reviews after any update.

For an authorized direct-push route, push only the verified candidate with an
explicit refspec:

```sh
git push <remote> <verified-candidate-id>:refs/heads/<destination>
```

Use a normal fast-forward push, never force or force-with-lease. If rejected,
inspect the new remote state or policy and rebuild/reverify as needed; do not
weaken protection. For a PR route, merge through the hosting service using its
allowed method and pin the expected PR head where supported. Recheck the head
before merging; automatic merge being scheduled is not completion.

Confirm the result from the hosting service and remote Git ref:

```sh
git ls-remote --exit-code <remote> refs/heads/<destination>
git fetch --no-tags <remote> refs/heads/<destination>
git rev-parse FETCH_HEAD
```

For a direct push, confirm the destination equals the tested candidate or contains
it if another legitimate update followed. For a PR merge, confirm the PR is merged
into the intended branch and its recorded merge/squash/rebase result is contained
in the fetched destination. For history-transforming methods, check the landed
diff against the verified PR change and required hosting checks; report the actual
result ID rather than claiming the candidate ID landed unchanged.

If authentication, permissions, checks, reviews or remote confirmation block
completion, report landing as incomplete and retain recoverable state. Preserve
source and integration branches; branch deletion requires a separate request.

Report the remote repository URL, destination branch, landed commit, source,
PR URL when applicable, checks run and validation limitations. Do not update the
user's primary checkout or deploy as part of remote landing.
