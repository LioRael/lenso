# Plan 003: Ship one end-to-end Plugin CLI workflow

> **Executor instructions**: Execute only after Plan 002's Runtime crates are
> published and verified from crates.io with explicit operator approval. Use
> released dependencies, not Git revisions or sibling paths. Follow every gate
> and stop on the listed conditions.
>
> **Drift check (run first)**:
> `git diff --stat 38549ab..HEAD -- src README.md docs tests .github Cargo.toml`
> Compare current CLI command and scaffold excerpts with `origin/main` before
> editing.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: `plans/002-derive-one-plugin-entry-from-source.md` and a
  verified Runtime crate release
- **Category**: dx, migration, tests
- **Planned at**: `lenso-cli` commit `38549ab`, 2026-08-27

## Why this matters

The current CLI calls `lenso new/check/dev/verify` the Module golden path, then
requires the same author to switch to `lenso plugin build --manifest ...`.
This is the dual authoring model in executable form. The target is one Plugin
namespace that owns creation through immutable packaging without making a
Harness Plugin author name or author a Module.

## Current state

- `src/main.rs:7-40` exposes top-level Module commands and a separate Plugin
  namespace containing only Bundle build/verify.
- `src/main.rs:42-149` names Module IDs, Module runtimes, Module recipes,
  Module repositories, and Module verification artifacts in public help.
- `src/module.rs:322-352` scaffolds Rust source containing `#[module]` and a
  `*Module` type.
- `src/module.rs:436-487` emits `lenso.module-verification-manifest.v1`,
  `MODULE.md`, and `lenso-module-dev`.
- `src/module.rs:600-882` does the same for Bun with `defineModule` and
  `@lenso/bun-module`.
- `src/plugin.rs:17-34` requires a publisher Manifest template, output path,
  and repeated `ARTIFACT_ID=PATH` arguments.
- Repository release rules require changesets for npm-facing changes and
  release-plz for Cargo. Publishing is not authorized by this plan alone.

## Target command surface

The normal Harness Plugin author path is exactly:

```sh
lenso plugin new <id>
lenso plugin dev
lenso plugin check
lenso plugin pack
```

`plugin check` is the explicit CI/preflight command. `plugin pack` must run the
same validation and then verify the exact package it wrote before publishing
the output path. Harness `plugins add` independently verifies received bytes.
There is no normal `plugin verify` command because it would add no authority.

Advanced built-in App behavior moves under an explicit Module namespace:

```sh
lenso module new|check|dev|verify
lenso app add|remove|check|resolve
```

The old top-level `new/check/dev/verify` commands remain hidden deprecated
aliases for one documented compatibility window. They must not appear in normal
help, new docs, scaffold READMEs, or Plugin diagnostics.

Plugin V1 supports the Rust-authored Wasm Component tracer from Plan 002. Bun,
QuickJS, Process, and native dylib Plugin scaffolds are rejected with one clear
supported-target message; existing built-in Module scaffolds remain available.

## Commands you will need

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Format | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo fmt --all -- --check` | exit 0 |
| Tests | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked --workspace` | all pass |
| Metadata | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo metadata --locked --format-version 1` | valid JSON, exit 0 |
| Package authoring | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo package --locked -p lenso-authoring --allow-dirty` | exit 0 |
| Package CLI | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo package --locked -p lenso-cli --allow-dirty --no-verify` | exit 0 |
| NPM shim | `npm run check:npm-shim` | exit 0 |
| Changesets | `pnpm changeset status --output /tmp/lenso-cli-plugin-authoring-changesets.json` | exit 0 |
| Help audit | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo run -q -p lenso-cli -- plugin --help` | lists exactly `new`, `dev`, `check`, and `pack`; no `build --manifest` or `verify` path |

## Scope

**In scope**:

- `src/main.rs`
- `src/plugin.rs`
- `src/module.rs`
- `src/authoring.rs`
- CLI tests and fixtures under `src/**` and `tests/**`
- `README.md`
- `docs/migration-*.md`
- npm shim/help snapshots and generated checks under `npm/` and `scripts/`
- Cargo manifests/lockfile and one changeset required by repository policy
- `plans/README.md` status row

**Out of scope**:

- Runtime crate implementation or unpublished Git/path dependency overrides.
- Harness installation/selection commands.
- General Bun/QuickJS/Process/native-dylib Plugin authoring.
- App Definition, Kernel, Plan, or Capability contract changes.
- Removal of the internal Bundle verifier or V1 Bundle read compatibility;
  `pack` and Harness admission still require fail-closed byte verification.

## Git workflow

- Create `advisor/003-unified-plugin-cli` from current `lenso-cli/origin/main`
  with Worktrunk.
- Read `docs/release-process.md` before editing release metadata.
- Use Conventional Commits and add the required npm changeset.
- Do not publish, push, or merge without instruction.

## Steps

### Step 1: Lock public help and compatibility behavior

Add CLI tests that assert the target command hierarchy and current aliases.
Tests must distinguish:

- normal Plugin author help;
- advanced Module author help;
- hidden deprecated top-level aliases;
- unsupported Plugin runtime error;
- JSON output stability where existing automation depends on it.

**Verify**: focused CLI tests fail against the old command tree, then pass after
Step 2.

### Step 2: Introduce Plugin and Module namespaces

Move the existing Module command handlers under `lenso module` without
duplicating their implementation. Route deprecated top-level aliases to the
same handlers and emit one actionable warning to stderr.

Expand `lenso plugin` with `new`, `dev`, `check`, and `pack`. Share
low-level authoring orchestration where possible, but all Plugin-facing help,
errors, reports, scaffold names, and output paths must say Plugin.

Remove `plugin verify` from normal help. If the repository's compatibility
policy requires one deprecation window, keep it as a hidden alias that explains
that `pack` and `plugins add` already validate exact bytes, then route it to the
same internal verifier. Do not maintain separate verification logic.

**Verify**: the help audit passes; `lenso module --help` contains the retained
built-in workflow; normal root help does not advertise deprecated aliases.

### Step 3: Scaffold one Plugin source authority

`lenso plugin new uppercase` must create a Rust Wasm Plugin project using the
released facade from Plan 002. It must not create:

- `#[module]` or a `*Module` author type;
- `MODULE.md`;
- `lenso-plugin.template.json`;
- a Module Descriptor;
- a hand-written `describe()`;
- a Plan/App Definition solely to package the Plugin.

Plugin ID and version come from one package manifest. Capability source/imports
and business code produce the generated descriptor authority. The scaffold
README contains only the four Plugin commands.

**Verify**: scaffold golden tests and an ignored clean-room compile test; run
`rg -n '#\[module\]|defineModule|MODULE\.md|module_contributions|template\.json|fn describe' <generated-project>` and expect no matches.

### Step 4: Implement check, dev, and pack

- `plugin check`: validate source, locked Capability facts, generated descriptor
  evidence, package identity/version, and supported V1 shape.
- `plugin dev`: run the Plugin through the same Wasm Component path used after
  packaging, with deterministic local diagnostics. Native-only success is not
  acceptable evidence.
- `plugin pack`: build the release artifact, invoke the released V2 Bundle
  builder, and write a non-overwriting `.lenso-plugin` package/directory without
  a template or artifact mapping flags. Before atomically publishing the output,
  reopen it through the shared Bundle verifier and fail if the exact closure,
  digest, or generated identity differs.

Each command must accept `--json` where automation needs stable output. Do not
expose internal Module contribution IDs, Plan bytes, or execution-class policy
in ordinary output.

**Verify**: one integration test runs all four commands on the generated
project, proves `pack` rejects invalid/stale input, and independently reopens
the resulting package with the internal library verifier.

### Step 5: Migrate docs and add release metadata

Make the Plugin workflow the Harness-extension golden path. Document Module
commands only as advanced built-in App authoring. Add a migration table:

| Old | New |
| --- | --- |
| `lenso new` | `lenso module new` for built-ins, or `lenso plugin new` for installable Harness extensions |
| `lenso check/dev/verify` | matching explicit namespace |
| `lenso plugin build --manifest ...` | `lenso plugin pack` |
| `lenso plugin verify --bundle ...` | no normal replacement; `pack` and Harness `plugins add` validate automatically |

Add the required npm changeset. Do not publish.

### Step 6: Run packaging and workspace gates

Run every command in “Commands you will need,” plus the ignored clean-room
Plugin scaffold test with `--test-threads=1`.

## Test plan

- Parser/help snapshots for new namespaces and hidden compatibility aliases.
- Rust Plugin scaffold golden assertions.
- Clean-room create/dev/check/pack integration test.
- Negative tests: duplicate identity, unsupported runtime, stale descriptor,
  changed artifact after pack, existing output, malformed package.
- Existing Module Rust and Bun scaffold tests continue to pass under
  `lenso module`; do not delete that coverage.

## Done criteria

- [ ] A Harness Plugin author uses one `lenso plugin` namespace from creation through packaging.
- [ ] Generated Plugin source contains no public Module authoring vocabulary.
- [ ] Packaging requires no Manifest template or repeated artifact mapping.
- [ ] `pack` verifies its exact output and normal Plugin help has no separate `verify` command.
- [ ] Built-in Module authoring remains available under `lenso module`.
- [ ] Compatibility aliases are hidden, warned, tested, and documented.
- [ ] Cargo/npm packaging and all workspace tests pass.
- [ ] Released dependency versions, not Git/path overrides, are used.

## STOP conditions

Stop and report if:

- Plan 002 crates are not verifiably available from crates.io;
- Plugin dev would exercise a different execution path from the packaged Wasm
  Plugin;
- Plugin packaging still needs the author to supply Module identity,
  contribution arrays, Descriptor digests, or execution policy;
- preserving an old alias makes the normal help surface ambiguous;
- Bun Plugin support is required to claim the first independent tracer works.

## Maintenance notes

Do not delete the internal Module implementation merely to rename files. The
quality criterion is one Plugin author interface backed by one shared lowering.
Review JSON output and npm shim compatibility carefully because the command
tree is user-facing across both distributions.
