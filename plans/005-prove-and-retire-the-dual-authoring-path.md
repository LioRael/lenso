# Plan 005: Prove the clean-room workflow and retire the dual path

> **Executor instructions**: This is a coordinated release and proof plan.
> Publishing, PR creation, merging, and remote repository creation each require
> explicit operator authorization. Without that authority, run all local and
> registry-read checks possible, mark the remaining gates BLOCKED, and stop.
>
> **Drift checks (run first)**:
>
> ```sh
> git -C /Users/leosouthey/Projects/framework/lenso-runtime-rust diff --stat b4ec847..HEAD
> git -C /Users/leosouthey/Projects/framework/lenso-cli diff --stat 38549ab..HEAD
> git -C /Users/leosouthey/Projects/framework/lenso-agent-harness diff --stat 37a70a8..HEAD
> git -C /Users/leosouthey/Projects/framework/lenso diff --stat e457271f..HEAD -- CONTEXT.md docs skills
> ```
>
> Confirm Plans 001–004 are merged and their exact contracts still match this
> plan. Otherwise stop and refresh the proof.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: MED
- **Depends on**: Plans 001–004 and explicitly approved Runtime/CLI releases
- **Category**: migration, tests, docs, release
- **Planned at**: `lenso` `e457271f`, `lenso-runtime-rust` `b4ec847`,
  `lenso-cli` `38549ab`, and `lenso-agent-harness` `37a70a8`, 2026-08-27
- **Current result**: DONE — crates.io `lenso-cli 0.4.7` and npm
  `@lenso/cli 0.14.0` complete the registry-only lifecycle in Harness.

## Why this matters

The migration is not successful because monorepo fixtures compile. It succeeds
only when an author with no sibling checkout can create a Plugin, write
business behavior, package it from public releases, add it to Harness, replace
it, remove it, and never encounter Module authoring. This plan records that
proof before old template and dual-command compatibility paths are retired.

## Current state

- Runtime uses release-plz and crates.io Trusted Publishing.
- CLI publishes Cargo and npm distributions independently; its
  `docs/release-process.md` requires registry verification and changesets.
- Harness now contains source-derived Plugin examples without a hand-written
  Manifest template.
- Existing research correctly notes that mature Plugin systems expose one
  understandable install unit while internal wiring stays behind the host.
- Public clean-room evidence is merged in `lenso-agent-harness` PRs #78 and #79
  at `docs/evidence/plugin-clean-room-v1.md`.

## Commands you will need

Exact versions are outputs of approved release PRs; substitute them without
using floating `latest`.

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Runtime registry | `cargo info lenso@<version> && cargo info lenso-guest-sdk@<version> && cargo info lenso-plugin-bundle@<version>` | each exact version and repository shown |
| CLI registry | `cargo info lenso-cli@<version>` | exact version shown |
| npm registry | `npm view @lenso/cli@<version> version dist.integrity` | exact version and integrity shown |
| Clean directory | `test -z "$(git status --short)"` in the generated proof repository | no output |
| Forbidden author terms | `rg -n '#\[module\]|defineModule|Module Descriptor|module_contributions|lenso-plugin\.template|ResolvedAppPlan|Plan Snapshot' . --glob '!target/**' --glob '!Cargo.lock'` | no matches |
| Harness regression | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked --workspace` | all pass |

## Scope

**In scope**:

- approved release PR metadata and changelogs in `lenso-runtime-rust` and
  `lenso-cli`
- registry verification evidence
- a temporary independent Git repository created with `mktemp -d` for proof;
  if the operator explicitly requests a permanent example, create it only in
  the named location/repository
- `lenso-agent-harness/docs/evidence/plugin-clean-room-v1.md` (create)
- migration/compatibility docs in the four repositories
- V1 compatibility parser/tests in Runtime, CLI, and Harness only after proof
- `plans/README.md` status row

**Out of scope**:

- Publishing without explicit approval.
- Creating a public GitHub repository without explicit approval.
- Renaming compatibility-era private runtime structs in the same release.
- Adding more Plugin shapes before the first proof.
- Control-plane deepening unrelated to retiring author-visible dual paths.

## Git workflow and prerequisite delivery evidence

Before this plan starts, the two operator-authorized release checkpoints in
`plans/README.md` must already have completed:

1. Plan 002 Runtime changes are merged, published, and verified on crates.io.
2. Plan 003 CLI changes consume those releases and are merged, published, and
   verified through Cargo/npm.
3. Plan 004 Harness changes consume the released contracts and are merged.

This plan begins by independently re-verifying those records, then runs the
clean-room proof. Only after proof may it retire deprecated creation paths in
semver-appropriate follow-up releases.

Use one Worktrunk worktree per repository and preserve all unrelated dirty
checkouts. After a PR is confirmed merged, follow repository cleanup policy;
never force-remove a dirty or unmerged worktree.

## Steps

### Step 1: Verify public release artifacts

After explicit approval and release automation completion, record:

- repository commit and tag;
- crates.io/npm exact version and checksum/integrity;
- package dependency graph showing no Git/path dependency on sibling repos;
- CLI `plugin --help` from the installed public artifact.

Do not treat a local build, GitHub workflow success, or release PR merge alone
as publication evidence.

**Verify**: run the registry commands and install the exact CLI in an isolated
Cargo/npm environment.

### Step 2: Create the clean-room Plugin repository

Use `mktemp -d`, initialize a new Git repository, and run the publicly installed
`lenso plugin new uppercase`. Do not copy any Harness fixture or use a sibling
path dependency.

Implement one observable Tool behavior using only scaffolded Plugin concepts.
Commit the initial source so subsequent generated/untracked drift is visible.

**Verify**: the forbidden-author-terms audit returns no matches and `cargo tree`
contains registry sources only for Lenso packages.

### Step 3: Prove the complete lifecycle

Record commands and observable results for:

1. `plugin check`;
2. `plugin dev` invoking the Tool;
3. `plugin pack` and exact package digest;
4. Harness `plugins add` and Ready status;
5. one Tool invocation;
6. source/version change and second package digest;
7. adding the new Release and observing new behavior;
8. an in-flight Turn retaining old behavior during replacement;
9. disable/enable;
10. remove and successful base-App invocation.

Also corrupt one package byte and prove add fails while the previous Plugin
remains active.

**Verify**: every step has a command, exit status, digest, and bounded observed
output in `plugin-clean-room-v1.md`. Do not include machine-local absolute paths.

### Step 4: Audit author-visible vocabulary

Run targeted searches over:

- generated Plugin project;
- installed CLI help and errors;
- Harness normal help/status/errors;
- public Plugin tutorials and example READMEs.

There must be no Module authoring construct, Plugin-to-Module mapping, Manifest
template, Plan, Store, Receipt, Controller, Supervisor, or Generation detail in
the normal path. Advanced developer diagnostics may retain runtime terms only
behind an explicit verbose/developer command.

**Verify**: forbidden-term searches return no normal-path matches; every allowed
match is listed with its advanced/historical justification in the evidence.

### Step 5: Retire compatibility creation paths

After the proof and the documented compatibility window:

- stop generating/advertising `lenso plugin build --manifest`;
- remove the hidden `lenso plugin verify` compatibility alias; keep one shared
  internal Bundle verifier used automatically by `pack` and Harness admission;
- remove V1 Manifest template creation support while retaining read/diagnostic
  support only for the promised semver window;
- remove hidden top-level Module aliases if their deprecation window elapsed;
- remove Harness compatibility aliases for install/upgrade/rollback/history/
  inspect when their window elapsed;
- preserve `lenso module ...` for deliberate built-in App Module authors.

Each removal must be a separate semver-reviewed change with a migration error,
not a silent parser failure.

**Verify**: compatibility tests assert the final supported/rejected matrix and
all repository CI gates pass.

### Step 6: Record the next boundary

Update the roadmap with empirical findings:

- actual time and authored file count from scaffold to running Tool;
- which internal Module/control-plane terms still leak;
- whether a second Plugin shape is justified;
- whether Store/Controller/Supervisor deepening now has measurable leverage.

Do not automatically schedule multi-entry, permissions, state, or marketplace
work. Each needs a user tracer and a separate decision.

## Test plan

- Public registry clean-room scaffold/build/package test.
- Harness black-box lifecycle and corrupted-package rejection.
- Old/new in-flight Turn behavior during replacement.
- Forbidden author-vocabulary audit.
- Compatibility matrix tests before each deprecated path is removed.
- Full CI-equivalent tests in Runtime, CLI, and Harness after their respective
  release/pin changes.

## Done criteria

- [x] Exact Runtime and CLI public artifacts are registry-verified.
- [x] A repository with no sibling checkout completes the canonical workflow.
- [x] Authored Plugin source contains none of the forbidden Module/Manifest/Plan terms.
- [x] Replacement, corruption rejection, in-flight consistency, and removal are proven.
- [x] Compatibility paths are retired only after their documented window.
- [x] Evidence separates current shipped behavior from deferred direction.
- [x] No publication or remote mutation occurred without explicit approval.

The normal CLI and Harness paths now expose only App and Plugin. Hidden
compatibility parsers remain only where their removal depends on the embedded
behavior migration and compatibility window in Plan 006; they are not a second
supported authoring model.

## STOP conditions

Stop and report if:

- publishing or remote-repository authority has not been explicitly granted;
- any clean-room step needs a sibling path/Git dependency or copied fixture;
- the generated author project contains Module authoring or a Manifest template;
- Harness requires manual Plan, Store, Receipt, or Generation operations;
- registry packages differ from the reviewed source/tag;
- retiring compatibility would violate an existing semver commitment.

## Maintenance notes

This proof is the product gate for subsequent Plugin expansion. Keep raw
commands reproducible but redact tokens and avoid machine-local paths. A second
Plugin shape should extend the one Plugin interface, not reintroduce
Plugin-to-many-Module authoring unless independent evidence makes that
cardinality unavoidable.
