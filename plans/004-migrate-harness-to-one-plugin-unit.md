# Plan 004: Make Harness consume one Plugin as one selectable unit

> **Executor instructions**: Execute after the CLI and Runtime contracts from
> Plans 002–003 are publicly released and verified. Use a clean Harness
> Worktrunk worktree; the primary checkout may contain unrelated
> `lenso.app.json` changes. Follow every gate and stop rather than broadening V1.
>
> **Drift check (run first)**:
> `git diff --stat 37a70a8..HEAD -- README.md CONTEXT.md AGENTS.md apps/lenso-agent-cli examples scripts docs Cargo.toml`
> Compare all cited symbols against live `origin/main` before editing.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: `plans/003-ship-one-plugin-cli-workflow.md` and verified CLI
  and Runtime releases
- **Category**: migration, dx, tests, direction
- **Planned at**: `lenso-agent-harness` commit `37a70a8`, 2026-08-27

## Why this matters

Harness currently calls optional features Plugins but expands them through
Plugin Profiles, Plugin Instances, plural Module contributions, locks, receipts,
and Generation authority. A user who wants to add one Tool must learn the
Module authoring path and then a separate Plugin packaging/control path. This
plan makes one generated Plugin Release the unit the Harness discovers,
validates, selects, replaces, and removes while retaining safe internal
Generation switching.

## Current state

- `apps/lenso-agent-cli/src/plugins.rs:64-116` exposes enable, disable,
  available, pack, install, remove, upgrade, rollback, status, history, and
  inspect commands.
- The same file imports public `ModuleContribution`, `LockedInstance`,
  `PluginSetLock`, Receipt, Store, and other control-plane implementation types.
- `examples/external-plugins/wasm-text-tools/guest/src/lib.rs` hand-writes a
  runtime `describe()` string.
- Its `lenso-plugin.template.json` repeats Plugin ID, version, artifact,
  Module contribution, Capability, operations, implementation, target,
  execution class, profile, support, trust, and many empty arrays.
- `README.md` first presents bundled Plugin enablement, then says those IDs
  become Module Instances, then documents a separate install/upgrade/rollback
  Store workflow.
- Harness invariants remain binding: untrusted third-party code uses reviewed
  Wasm or isolated process; one Turn keeps one Generation route lease until its
  terminal outcome; Kernel never discovers packages or mutates its graph.

## Target product contract

Normal Harness Plugin lifecycle:

```sh
harness plugins list
harness plugins add ./dist/uppercase.lenso-plugin
harness plugins status
harness "Use uppercase on hello"
harness plugins disable dev.example.uppercase
harness plugins enable dev.example.uppercase
harness plugins remove dev.example.uppercase
```

Adding a newer immutable Release for the same ID performs validate → stage →
Ready → switch → drain. The user does not invoke a separate `upgrade`, choose a
Generation, manage rollback history, or reference a Module Instance.

V1 external Plugin constraints:

- exactly one Wasm Component executable entry;
- exactly one `lenso.agent.tool-provider@2` provider;
- empty configuration or one source-derived bounded configuration Schema;
- no required Capabilities, permissions, state, data mounts, features, binding
  templates, provider replacement, or publisher-selected trust/policy;
- additive `many` attachment to the Harness Tool aggregate;
- one Plugin ID, Release version, configuration, and enable/disable state.

## Commands you will need

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Format | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo fmt --all -- --check` | exit 0 |
| Plugin tests | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked -p lenso-agent-cli --test plugins` | all pass |
| Headless | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked -p lenso-agent-cli --test headless` | all pass |
| Workspace | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked --workspace` | all pass |
| Contracts | `./scripts/check-contracts.sh` | exit 0 |
| Removal | `./scripts/check-removal.sh` | exit 0 |
| Public words | `rg -n 'Module author|Module Instance|module_contributions|Plugin Instance|Manifest template' README.md docs examples/external-plugins` | no matches outside an explicitly labelled migration/history page |

## Scope

**In scope**:

- `apps/lenso-agent-cli/src/main.rs`
- `apps/lenso-agent-cli/src/plugins.rs`
- `apps/lenso-agent-cli/src/plugin_profiles.rs`
- `apps/lenso-agent-cli/src/generation.rs` only at the Plugin selection/lowering seam
- `apps/lenso-agent-cli/src/provenance.rs` only to remove retired public command output
- `apps/lenso-agent-cli/tests/plugins.rs`
- `apps/lenso-agent-cli/tests/headless.rs`
- `examples/external-plugins/wasm-text-tools/**`
- `README.md`, `CONTEXT.md`, `AGENTS.md`, `docs/**`
- `scripts/check-contracts.sh`, `scripts/check-removal.sh`, and a new public
  workflow test script if needed
- Cargo manifests/lockfile and release metadata needed to consume published crates
- `plans/README.md` status row

**Out of scope**:

- Kernel/App Plan changes.
- General marketplace, registry, automatic update, distributed coordination,
  state migration, or data-only Plugins.
- Bun, QuickJS, Process, or native dylib public Plugin authoring.
- Rewriting control-plane internals unrelated to the one-entry consumer seam.
- Deleting old Bundle V1 read compatibility before Plan 005.

## Git workflow

- Create `advisor/004-one-harness-plugin-unit` from current
  `lenso-agent-harness/origin/main` with Worktrunk.
- Use Conventional Commits and published dependency versions.
- Do not publish, push, open a PR, or merge without instruction.

## Steps

### Step 1: Characterize one complete user lifecycle

Before changing production code, add an integration test for list → add → run →
replace → run new → disable → enable → remove → base run. Assert public output
contains Plugin ID/version/status only and never Module contribution/instance,
Plan, Receipt, Store, Controller, Supervisor, Generation digest, or routing epoch.

Keep separate internal tests for lease/drain and fail-closed recovery.

**Verify**: the new public lifecycle test fails against the old command set for
the intended reasons; existing Plugin/headless tests still pass.

### Step 2: Consume the V2 one-entry package

Use the released Runtime verifier to load one generated V2 Plugin. Validate the
fixed Harness profile against the derived entry and lower it internally into
the existing resolved Module/Capability/Plan structures.

The publisher must not provide or select:

- Module contribution or instance IDs;
- execution class, trust, support, target policy, or Slot/profile;
- binding templates, permissions, Features, or state policy.

Quarantine malformed or unsupported packages with one Plugin-readable problem.
Do not partially admit or fall back to publisher metadata.

**Verify**: table-driven tests reject every forbidden variation and accept the
exact one-entry Tool Plugin.

### Step 3: Collapse the normal command surface

Implement `list`, `add`, `status`, `enable`, `disable`, and `remove` as the only
normal commands. `add` handles first install and a newer Release for the same
ID; it validates and Ready-checks before committing selection. Failure leaves
the previous package and active Generation unchanged.

Move `install`, `upgrade`, `rollback`, `history`, `inspect`, and Harness-side
`pack` out of normal help. If compatibility is required, keep hidden aliases
that produce a migration warning and call the same new path; do not maintain a
second implementation.

**Verify**: a help/usage test asserts the six commands exactly and rejects the
retired normal forms with the documented migration message.

### Step 4: Remove Plugin-to-Module author-facing mapping

Internally, create one private lowering function from verified V2 Plugin entry
to the existing runtime plan contribution. Public status, problems,
configuration errors, README, examples, and CLI output use Plugin terms only.

Delete or privatize product-layer `PluginInstance`/`LockedInstance` assembly
where V1 always has one entry and one configuration. Do not rename Kernel/App
Plan Module types or weaken exact Plan resolution.

**Verify**: run the public-words audit. Internal Rust matches are allowed only
below the private lowering seam and in compatibility tests.

### Step 5: Replace the external example

Regenerate `wasm-text-tools` using the released `lenso plugin new` workflow.
Remove its hand-written `describe()` and Manifest template. Its README must use
only Plugin commands and the Harness six-command lifecycle.

Build and package it without workspace path dependencies, install it, invoke
`uppercase`, replace it with a new version that produces observably different
output, and remove it.

**Verify**: the lifecycle integration test uses the regenerated example and
passes without reading any author-owned Module or Manifest document.

### Step 6: Preserve Generation safety behind the seam

Keep the existing Ready gate, route lease, atomic switch, drain, and recovery
tests. One Turn that begins before replacement must finish on the old Plugin
Release; a new Turn after the switch must use the new Release. A failed candidate
must not alter the active selection.

Ordinary output must call these states `active`, `pending`, `blocked`, or
`disabled`; Generation diagnostics remain available only in explicit developer
diagnostics, not the Plugin lifecycle.

**Verify**: focused Plugin and headless tests cover old/new Turn pinning,
candidate rejection, restart recovery, and removal.

### Step 7: Run the full Harness gates

Run every command in “Commands you will need.” Inspect `git status` and confirm
the primary checkout's unrelated `lenso.app.json` was never touched.

## Test plan

- One black-box public lifecycle integration test.
- V2 admission table for all allowed/forbidden fields.
- Failed replacement preserves active Plugin test.
- Concurrent Turn lease/drain test.
- Crash/restart recovery of exact active Plugin test.
- Removal restores base App test.
- Compatibility read test for one V1 bundle, isolated and labelled deprecated.
- Use existing `apps/lenso-agent-cli/tests/plugins.rs` process helpers and
  `headless.rs` invocation patterns.

## Done criteria

- [ ] A generated Plugin is the only author/install/select/remove unit.
- [ ] One external Plugin contributes and runs a Tool without authored Module artifacts.
- [ ] Normal Harness help contains exactly six lifecycle commands.
- [ ] Plugin replacement is Ready-gated and preserves in-flight Turns.
- [ ] Public docs, examples, output, and errors do not map Plugin to Module.
- [ ] Kernel and immutable Plan invariants remain unchanged.
- [ ] Every Harness verification command passes.

## STOP conditions

Stop and report if:

- the first external Tool Plugin requires more than one executable entry,
  permission request, required Capability, or publisher binding template;
- V2 lowering requires Kernel discovery or mutable graph changes;
- replacement cannot preserve the previous active selection on failure;
- a second independent consumer requires the old plural contribution schema;
- published Runtime/CLI packages do not match the planned contracts.

## Maintenance notes

Reviewers should judge the black-box workflow, not reductions in Rust type
count. Internal Module and Generation machinery may remain if no Plugin author
or ordinary operator must understand it. Do not reintroduce a Host profile as a
publisher-authored manifest under a new name.
