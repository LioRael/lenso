# Contributing to Lenso vNext

Lenso is a local-first runtime built from replaceable Plugins, typed
Capabilities, Runtime Drivers, and Execution Adapters. Read [`CONTEXT.md`](CONTEXT.md)
for the vocabulary and invariants before changing framework behavior.

AI tools and any particular editor are optional. A contributor needs only Git,
a fork, and the tools required by the part they change.

## Propose a contribution

Use a GitHub Issue as the durable handoff when you do not have upstream write
access. Include:

- the problem or change summary;
- the fork URL and branch;
- the immutable full commit SHA;
- focused checks that passed; and
- known limitations or checks that were not run.

The maintainer reviews the pinned revision, not a moving branch. If a fork is
impractical, publish a `git format-patch` file at a durable accessible URL and
include its source SHA in the Issue. Issues are not arbitrary patch-upload
storage.

## Minimal fork workflow

```sh
git clone https://github.com/<you>/lenso.git
cd lenso
git remote add upstream https://github.com/LioRael/lenso.git
git fetch upstream main
git switch -c <topic> upstream/main
# edit with any editor, then run focused checks
git add <changed-files>
git commit -m "type(scope): describe the change"
git push -u origin <topic>
```

Open an Issue at <https://github.com/LioRael/lenso/issues/new> with the fork
branch URL, full commit SHA, validation, and limitations. A patch alternative
is:

```sh
git format-patch --stdout upstream/main..<topic> > /tmp/lenso-<topic>.patch
```

Publish that file somewhere durable and link it from the Issue.

## Validation

Choose the smallest check that exercises the changed behavior:

- prose or documentation: check links, examples, and formatting;
- Rust code: run the affected package tests and checks;
- workflow, executable script, Land skill, or build configuration: run focused
  syntax/configuration checks and the relevant script tests;
- unknown or cross-cutting changes: use the repository's broader gate.

Contributors do not need to run every platform or release check. The upstream
candidate gate supplies the repository's native and portable WebAssembly proof
when the final change requires it.

### CI feedback and dependency caches

The candidate gate retains workspace Clippy, all workspace tests (including
doctests), both portable WebAssembly targets, and Rust/Bun conformance. Native
`cargo check` with the same target/features is covered by Clippy. Formatting
runs before cache restoration and packaging tests.

`Compile workspace tests` measures the outer Cargo build separately from
`Test the workspace`. The latter still invokes Cargo normally so doctests are
not lost; it also includes any builds launched inside integration tests.
Clippy, test compilation, and target checks upload timestamped Cargo HTML
timings as a seven-day artifact, including reports available after a failure.
These reports do not measure nested Cargo invocations or test execution;
use the test logs and step durations for those.

Baseline: [candidate run 36392186537 at `07247a8`](https://github.com/LioRael/lenso/actions/runs/36392186537)
had a cold Rust cache. Clippy took 2m24s, the redundant native check 28s,
and workspace tests 25m50s (including 4m28s of outer compilation).
`configuration_source_dev` alone ran for 729.59s; its two cases serialize
independent release-mode App builds. This is the first test-internal
optimization target, not evidence that packaging or Wasm checks should be
removed. Cache reuse and timing instrumentation are not a measured 5–10 minute
gate yet.

See [CI feedback cost and coverage](docs/performance/ci-feedback.md) for
test-retention decisions, focused reproduction commands, and measured local
experiments. Local warm-cache results do not establish cold candidate latency.

GitHub caches are branch-scoped: one `candidate/**` branch cannot restore a
sibling's cache. Rust caches are therefore saved only on trusted `main`, with
the same workflow/job keys used by candidates. After this workflow lands,
seed the default-branch caches using the existing manual trigger:

```sh
gh workflow run ci.yml --repo LioRael/lenso --ref main
```

Repeat when dependencies or the toolchain change, or when caches expire.
This runs the full workflow once; it is not an automatic second gate after
every landing, and its result does not replace candidate CI. Inspect the
Rust cache restore logs on the next candidate to confirm reuse. A cache hit
does not eliminate workspace compilation or isolated fixture builds. Until
the first successful seed, candidates continue to work with cold caches.

## Maintainer integration

The maintainer imports the immutable Issue revision into an isolated checkout,
reviews untrusted workflow and script changes before using upstream
credentials, integrates it on the current `origin/main`, runs one final
candidate check, and lands that exact SHA with a normal fast-forward. Fork CI
is useful context but does not replace the upstream candidate result. The
maintainer preserves contributor authorship and links the Issue and final
commit.

Editors and agents are optional. They do not grant GitHub write, landing,
publication, or deployment authority. Plain Git maintainers can follow the
same immutable-candidate path.

## Maintainer Git landing

```sh
git fetch origin main
git switch -c land/<topic> origin/main
# import and review the contributor's immutable SHA
git push origin HEAD:refs/heads/candidate/<task>/<attempt>
gh run list --repo LioRael/lenso --workflow ci.yml \
  --branch candidate/<task>/<attempt>
gh run view <run-id> --repo LioRael/lenso \
  --json workflowName,event,headBranch,headSha,jobs,url
git fetch origin main
git push origin <candidate-sha>:refs/heads/main
gh api repos/LioRael/lenso/git/ref/heads/main --jq .object.sha
```

Accept only a successful `quality` job for the exact candidate SHA and
candidate push attempt. If `main` advances, integrate and validate again; if a
candidate is already reachable from the remote tip, keep its SHA unchanged.
Use normal pushes only. Publication, tags, and deployment are separate
authorized operations.

## Publication owner cutover

Before enabling this repository's `release-plz.yml` publish mode, inventory and
disable every legacy workflow that can publish the same crates, then read back
each workflow's inactive state. Migrate each affected crate's Trusted Publisher
to `LioRael/lenso` and `release-plz.yml`, and verify that ownership. Only then
may an administrator set the repository variable
`LENSO_RELEASE_OWNER_CUTOVER=complete`. Its absence blocks publication; a
successful dry run does not authorize the cutover.

## Scope and commits

Do not restore v0.3.x Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, migration, or TypeScript Service Kit code to this branch. If a
feature needs one of those concepts, express it first as a vNext Capability,
ordinary Plugin, Execution Adapter, authoring tool, or separate repository.

Use Conventional Commits:

```text
<type>[optional scope]: <imperative summary>
```

Stage only files belonging to the requested change. Keep generated lockfile
changes with the manifest change that caused them.
