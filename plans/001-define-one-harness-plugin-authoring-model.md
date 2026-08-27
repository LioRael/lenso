# Plan 001: Define one Harness Plugin authoring model

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If a STOP condition occurs, report it and do not improvise. When
> done, update this plan's row in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat e457271f..HEAD -- CONTEXT.md AGENTS.md README.md docs skills`
> If in-scope language changed, compare the current excerpts below with live
> files before editing. A semantic mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: direction, migration, dx, docs
- **Planned at**: `lenso` commit `e457271f`, 2026-08-27

## Why this matters

The accepted model says Module is the sole authoring abstraction and Plugin is
its installable distribution role. The actual Harness extension journey makes
one person use both abstractions: author a Module, then author a Plugin Release
that repeats identity, version, configuration, Capabilities, implementation,
and artifact facts. Before code changes, the project needs one explicit persona
contract: Harness Plugin authors author Plugins end to end; the generated
runtime lowering may use Module semantics but is not their interface.

## Current state

- `CONTEXT.md:20-28` defines Module, Module Descriptor, and Module Instance as
  public canonical terms.
- `CONTEXT.md:42-50` defines Plugin, Plugin Release, and Plugin Instance; a
  Plugin Instance resolves to zero or more Module Instances.
- `docs/adr/0065-govern-dynamic-plugins-above-the-kernel.md` explicitly rejected
  a second Plugin authoring vocabulary, but then gives Plugin its own identity,
  release, configuration, enablement, and instance lifecycle above Module.
- ADR 0065 says authors write Modules and add packaging metadata afterward.
  That is exactly the dual-persona handoff this decision must supersede for
  Harness extension authors.
- ADR 0066 makes `#[lenso::module]` and `defineModule` the author entrypoints.
  The new decision preserves source derivation but gives Harness Plugin authors
  a Plugin-named facade that lowers into the same runtime facts.
- `docs/adr/README.md` says accepted ADR history is not rewritten. The next
  decision is ADR 0069 unless execution finds that number occupied.

The target relationship is:

```text
Harness Plugin source + package identity + version
  -> generated immutable Plugin Release
    -> generated internal Module Descriptor/Instance lowering
      -> immutable Plan -> Kernel
```

For V1, the first two boxes are one-to-one: one Plugin Release has one
executable entry, one configuration, and one enable/disable lifecycle.

## Commands you will need

Run Cargo through the shared wrapper in the task worktree.

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Format | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo fmt --all -- --check` | exit 0 |
| Repository seam | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo xtask check-core-repository-boundary` | exit 0 |
| Module size | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo xtask check-rust-module-size` | exit 0 |
| Workspace | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked --workspace` | all pass |
| Vocabulary audit | `rg -n 'Plugin Instance|zero or more Module|Plugin authors write Modules|Module author.*Plugin' CONTEXT.md AGENTS.md README.md docs skills` | matches only historical ADR text, research evidence, or explicitly labelled compatibility material |

## Scope

**In scope**:

- `docs/adr/0069-make-plugin-the-harness-extension-authoring-unit.md` (create;
  take the next free number if 0069 is occupied)
- `docs/adr/README.md`
- `CONTEXT.md`
- `AGENTS.md`
- `README.md`
- `docs/README.md`
- `docs/architecture/lenso-authoring.md`
- `docs/architecture/plugin-authoring-and-resolution.md`
- `docs/architecture/dynamic-plugins.md`
- `docs/agents/domain.md`
- `docs/agents/skills.md`
- `skills/lenso-start/**`
- `skills/lenso-module-authoring/**`
- `skills/lenso-app-composition/SKILL.md`
- `skills/lenso-app-composition/agents/openai.yaml`
- `skills/lenso-app-composition/references/dynamic-plugin-intent.md`
- `skills/README.md`
- `plans/README.md` status row

**Out of scope**:

- Rust types, macros, schemas, Cargo manifests, generated descriptors, or CLI
  behavior; those belong to later plans.
- Rewriting or deleting ADRs 0065 and 0066.
- Renaming Module in Kernel, App Plan, Execution Adapter, or built-in App
  authoring vocabulary.
- Multi-entry Plugins, data-only Plugins, Plugin Features, Slots, hot Plan
  Transitions, or control-plane refactoring.

## Git workflow

- Create `advisor/001-plugin-authoring-model` with `wt switch --create` from
  the current `origin/main`.
- Use Conventional Commits, for example
  `docs: define Plugin as the Harness authoring unit`.
- Do not push, open a PR, merge, or release without instruction.

## Steps

### Step 1: Record ADR 0069

Create a concise accepted/proposed ADR following repository convention. It must
state all of the following:

1. A Harness extension author authors one Plugin, not a Module followed by a
   Plugin wrapper.
2. V1 Plugin source has one identity, version, configuration, executable entry,
   and lifecycle unit.
3. Tooling generates the immutable release and internal Module lowering; the
   author does not write or name that lowering.
4. App owners may still author built-in Modules directly. Module is therefore
   not globally private; it is absent specifically from the Harness Plugin
   author interface.
5. Existing Module source remains a compatibility input during migration, not
   the target Plugin workflow.
6. Capability, immutable Plan, Kernel, and above-Kernel governance semantics do
   not change.
7. ADR 0069 supersedes the authoring-path portions of ADRs 0065 and 0066, not
   their runtime and authority decisions.

**Verify**: `rg -n 'Harness Plugin author|one executable entry|ADR 0065|ADR 0066' docs/adr/0069-*.md` → all four decisions are present.

### Step 2: Correct the canonical vocabulary

Keep definitions short and implementation-free. Define:

- **Plugin**: the authored, packaged, installed, configured, and selected
  Harness extension unit; one executable entry in V1.
- **Plugin Release**: one immutable version of that authored Plugin.
- **Module**: the App runtime composition unit used by built-in App authoring
  and generated runtime lowering.
- **Module Instance**: an App-local runtime instantiation; not a second Plugin
  author choice.

Remove `Plugin Instance` from canonical V1 vocabulary. Do not claim a Plugin
resolves to zero or more Modules. Avoid defining implementation types such as
Store, Controller, or Receipt in the glossary.

**Verify**: `rg -n '^[-*] \*\*(Plugin|Plugin Release|Module|Module Instance|Plugin Instance)' CONTEXT.md` → exactly four target terms and no `Plugin Instance` definition.

### Step 3: Route the two personas explicitly

Update architecture and skill routing so:

- “extend Harness or another Plugin-enabled product” routes to the Plugin
  authoring workflow planned in 002–004;
- “build behavior linked into an App” routes to Module authoring;
- no page calls the same ordinary user both a Module author and Plugin author;
- historical/research comparisons retain their original terminology.

The existing `lenso-module-authoring` skill may remain for built-in behavior.
Do not invent the final Plugin skill until the implemented CLI exists; document
the planned route and add a follow-up marker referencing ADR 0069.

**Verify**: run the vocabulary audit command. Remaining matches must be
historical, research, or migration text and labelled as such.

### Step 4: Run repository gates

Run all commands in “Commands you will need.” Documentation-only changes still
run the architecture seam and workspace test because the canonical decision
constrains future portable-core work.

## Test plan

- No new Rust tests are expected.
- Treat `rg` assertions as the terminology regression tests.
- Review ADR 0069 against the three ADR criteria: hard to reverse, surprising
  without context, and a real Module-only versus Plugin-only authoring tradeoff.

## Done criteria

- [x] ADR 0069 records one Harness Plugin authoring unit and its compatibility boundary.
- [x] `CONTEXT.md` no longer defines Plugin Instance or zero-to-many Module expansion as V1.
- [x] Module remains accurate for App runtime composition and built-in authoring.
- [x] Harness Plugin author routing never requires first becoming a Module author.
- [x] All verification commands pass.
- [x] Only the in-scope documentation/skill files and plan status changed.

Completed in `lenso` commit `5cd8b04d` on branch
`advisor/001-plugin-authoring-model`.

## STOP conditions

Stop and report if:

- a real shipped Harness Plugin already requires multiple independently
  configured executable entries under one install identity;
- another accepted ADR after 0068 already resolves this exact terminology;
- changing the vocabulary would require changing immutable Plan or Kernel wire
  formats in this plan;
- the executor cannot state one unambiguous user journey without using both
  “Module author” and “Plugin author” for the same person.

## Maintenance notes

Reviewers should reject wording-only internalization claims. Later plans must
prove the author does not write `#[module]`, `defineModule`, a Module Descriptor,
or a Plugin Manifest template. If those remain, ADR 0069 is not implemented.
