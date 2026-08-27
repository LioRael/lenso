# Plan 002: Derive one Plugin entry from source without executing it

> **Executor instructions**: This plan changes public Runtime authoring and
> immutable Bundle inputs. Follow the order exactly. Begin with the descriptor
> transport proof; do not publish an interface if the proof fails. Run each
> verification gate and stop on any listed condition.
>
> **Drift check (run first)**:
>
> ```sh
> git diff --stat b4ec847..HEAD -- \
>   crates/lenso crates/lenso-native-adapter-macros crates/lenso-guest-sdk \
>   crates/lenso-plugin-bundle crates/lenso-wasm-component-adapter docs
> ```
>
> Compare the current-state excerpts below with `origin/main`. A semantic
> mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: `plans/001-define-one-harness-plugin-authoring-model.md`
- **Category**: direction, migration, dx, tests
- **Planned at**: `lenso-runtime-rust` commit `b4ec847`, 2026-08-27

## Why this matters

The current external Wasm example exposes the dual model in its sharpest form:
business code implements a guest Module and runtime `describe()`, while a
publisher separately writes a 62-line Plugin Manifest template repeating the
same Capability and implementation facts. The Runtime must provide one
source-derived Plugin contract before the CLI can offer an honest
`lenso plugin new/check/pack` workflow.

## Current state

- `crates/lenso/src/lib.rs` describes itself as the stable Rust authoring
  interface for Modules and exports `module`, `ModuleConfig`, `ModuleResult`,
  and Module-named diagnostics.
- `crates/lenso-native-adapter-macros/src/lib.rs:123-141` implements
  `#[module]`; package identity comes from
  `[package.metadata.lenso].package-id`.
- `crates/lenso-guest-sdk/src/lib.rs:101-220` exposes
  `GuestProvidedCapability`, `GuestModuleDescriptor`,
  `encode_guest_descriptor`, and `guest_descriptor!`. The descriptor is
  produced at runtime.
- `crates/lenso-plugin-bundle/src/model.rs:6-25` requires a publisher-owned
  `PluginManifest` containing plural `module_contributions`, data, permissions,
  features, binding templates, and product metadata.
- `crates/lenso-plugin-bundle/src/lib.rs:115-186` starts from a publisher
  template and replaces artifact digest/size. It cannot derive contribution
  facts from the artifact without executing it.
- Component encoding occurs inside Bundle build. Any descriptor transport must
  survive Rust core-Wasm to Component conversion and be read from the exact
  final bytes.

## Target contract

For the first public Harness Plugin shape:

- author-owned inputs: Plugin ID, Release version, Plugin source, and ordinary
  Cargo metadata;
- source-derived facts: provided Tool Capability, operations, configuration
  Schema, and the single executable entry;
- builder-derived facts: exact bytes, digest, size, media type, and target;
- Harness-owned facts: allowed Capability, execution class, trust, support
  status, attachment, and permission policy;
- generated output: immutable `lenso-plugin.json` schema V2;
- no publisher-authored `module_contributions`, implementation variants,
  Descriptor digests, empty arrays, or placeholder hashes.

The generated V2 manifest may contain an internal runtime entry needed by the
Host, but its public schema name must be `entry`, not `module_contributions`.
Runtime lowering into `ModuleContribution` remains private to the control plane.

## Commands you will need

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Format | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo fmt --all -- --check` | exit 0 |
| Clippy | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0 |
| Check | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo check --locked --workspace --all-targets` | exit 0 |
| Guest SDK | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked -p lenso-guest-sdk` | all pass |
| Bundle | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked -p lenso-plugin-bundle` | all pass |
| Wasm Adapter | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked -p lenso-wasm-component-adapter` | all pass |
| Workspace | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo test --locked --workspace` | all pass |
| Browser target | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo check --locked -p lenso-browser-driver --all-targets --target wasm32-unknown-unknown` | exit 0 |
| WASIp2 target | `/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo check --locked -p lenso-wasip2-driver --all-targets --target wasm32-wasip2` | exit 0 |

## Scope

**In scope**:

- `crates/lenso-guest-sdk/**`
- `crates/lenso-plugin-bundle/**`
- `crates/lenso-wasm-component-adapter/tests/**`
- `crates/lenso-native-adapter-macros/**` only for additive `#[plugin]` facade
  parity and compile tests
- `crates/lenso-native-adapter/**` only for facade re-exports
- `crates/lenso/**` only for facade re-exports, names, and tests
- `docs/evidence/plugin-descriptor-transport.md` (create)
- repository changelogs/changesets required by existing release automation
- `plans/README.md` status row in the planning worktree

**Out of scope**:

- Kernel or App Plan wire-format changes.
- Plugin Store, admission, selection, Controller, Supervisor, or Harness code.
- Bun/QuickJS/Process/native-dylib Plugin authoring.
- Multiple entries, data-only Plugins, publisher Features, binding templates,
  state migration, or permissions.
- Removing existing Module authoring primitives; built-in App authors retain
  them and compatibility removal belongs to Plan 005.

## Git workflow

- Create `advisor/002-source-derived-plugin-entry` from current
  `lenso-runtime-rust/origin/main` with Worktrunk.
- Prefer one commit for the descriptor proof and one for the public additive
  interface. Use Conventional Commits.
- Do not publish; publishing requires the release phase in Plan 005 and explicit
  approval.

## Steps

### Step 1: Characterize the existing authorities

Add tests proving that today:

- runtime `describe()` is the only descriptor inside the guest behavior;
- the Bundle builder cannot package without the handwritten template;
- the template repeats Capability ID, operations, execution class, target,
  trust, support, and artifact identity;
- componentization preserves behavior but provides no packaging descriptor.

These tests establish the duplication before replacing it.

**Verify**: run Guest SDK, Bundle, and Wasm Adapter focused tests; all pass and
the new test names contain `publisher_template_duplicates_guest_descriptor` or
equivalent explicit wording.

### Step 2: Prove a non-executing descriptor transport

Evaluate, in this order:

1. a deterministic Wasm custom section emitted from source/build tooling and
   preserved or deliberately transferred during Component encoding;
2. a generated adjacent descriptor emitted from the same source authority and
   cryptographically closed into the final Bundle;
3. a fixed Harness Tool Plugin profile plus only artifact identity, if neither
   general transport can be made single-authority.

The proof must show:

- packaging never instantiates or invokes publisher code;
- identical source and locked dependencies produce identical descriptor bytes;
- the final Component and packaging descriptor cannot be mixed across builds;
- Ready-time `describe()` agrees byte-for-byte or semantically through one
  canonical encoder with the packaged descriptor;
- malformed, missing, duplicate, oversized, or conflicting descriptor evidence
  fails closed;
- componentization does not silently discard the authority.

Record measurements, commands, chosen option, rejected options, and exact
limitations in `docs/evidence/plugin-descriptor-transport.md`.

**Verify**: focused tests cover all six properties and the evidence document
contains `Chosen transport`, `Rejected`, and `No publisher code executed`.

### Step 3: Add the Plugin authoring facade

For Rust-authored Plugin source, add Plugin-named author constructs that lower
to existing Module runtime semantics:

- `#[plugin]` where an attribute macro is appropriate;
- Plugin-named configuration/result/diagnostic aliases where the author would
  otherwise see Module language;
- a guest Plugin declaration macro or generated entrypoint that emits both the
  package descriptor authority and runtime `describe()` from one input.

Do not implement `#[plugin]` as a second independent expansion. Route both the
Plugin facade and retained Module facade through one private lowering function
and test their generated runtime descriptors for equality.

New Plugin diagnostics must say Plugin. Existing `#[module]` diagnostics remain
Module-specific for built-in App authors.

**Verify**: compile-pass and compile-fail tests show a Plugin fixture contains
no `#[module]`, Module Descriptor input, or hand-written `describe()` while
producing the expected runtime descriptor.

### Step 4: Generate the one-entry V2 Bundle

Add a schema-versioned V2 authoring input/output path. The builder reads Plugin
identity/version from package metadata, reads the proven descriptor evidence,
computes exact artifact facts, and emits one immutable manifest.

Requirements:

- one `entry`, not plural `module_contributions`;
- no author-supplied digest, size, execution class, support channel, trust,
  target list, Capability table, or empty policy arrays;
- strict unknown-field rejection and canonical bytes;
- V1 verification remains readable during migration but V1 creation is not the
  new default;
- the Bundle verifier exposes only Plugin-readable status; internal lowering
  may construct existing control-plane Module records privately.

**Verify**: Bundle golden tests compare exact V2 bytes, reject each forbidden or
conflicting input, and continue to verify one existing V1 fixture.

### Step 5: Run the complete Runtime matrix

Run every command in “Commands you will need.” Also run:

```sh
rg -n 'module_contributions|Plugin Manifest template|placeholder digest' \
  crates/lenso-guest-sdk crates/lenso-plugin-bundle crates/lenso/tests
```

Expected: matches only V1 compatibility parsing/tests, never the V2 authoring
path or Plugin facade examples.

## Test plan

- Unit tests for deterministic descriptor encoding and all malformed evidence.
- Integration test building a Rust guest, componentizing it, extracting the
  descriptor without execution, creating a V2 Bundle, verifying it, then
  comparing Ready-time description.
- Compile tests for `#[plugin]`/guest facade success and Plugin-named errors.
- Compatibility test reading, but not authoring, one V1 manifest.
- Use existing `lenso-plugin-bundle` strict/canonical tests and
  `lenso-wasm-component-adapter` Rust guest fixtures as structural patterns.

## Done criteria

- [ ] One source declaration owns Plugin identity, version, entry, Capability facts, and runtime description.
- [ ] Packaging executes no publisher code.
- [ ] V2 package creation requires no Manifest template.
- [ ] A Plugin author fixture contains no Module authoring construct or descriptor file.
- [ ] V1 verification compatibility is explicit and isolated.
- [ ] All Runtime CI-equivalent commands pass.
- [ ] Public crates are ready for a semver-appropriate release but are not published.

## STOP conditions

Stop and report if:

- descriptor evidence cannot survive Component encoding without a second
  hand-authored authority;
- Ready-time description cannot be proven to match packaging-time evidence;
- the only working design executes publisher code during packaging/admission;
- V2 requires changing Kernel or Resolved App Plan bytes;
- `#[plugin]` would duplicate rather than share the existing source-lowering
  implementation;
- a real external consumer requires authoring multiple entries in the first V2
  schema.

## Maintenance notes

The custom section or sidecar is a supply-chain authority. Review byte limits,
path handling, canonicalization, and final-artifact binding carefully. Do not
call the facade complete if only native `#[plugin]` exists; the first Harness
tracer must be a distributable Wasm Component.
