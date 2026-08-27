# Plan 006: Resolve Apps from Plugin Roots

## Status

- **Priority**: P1
- **Effort**: XL
- **Risk**: HIGH
- **Depends on**: ADR 0069, ADR 0070, and the completed Plan 005 public proof
- **Status**: IN PROGRESS — ADR 0070 and the Plugin Root contract are drafted

## User outcome

An App owner starts a Host and gets its useful default App without creating an
App manifest. To change that App, the owner adds, configures, disables, or
removes a Plugin under `plugins/`. The owner never writes packages, bindings,
execution lanes, generated descriptors, or an App Definition.

Plugin authors continue to use one behavior model from source through runtime:

```text
lenso plugin new -> check -> dev -> pack
```

App owners work only with Plugins and a derived App:

```text
lenso plugins list|add|configure|disable|enable|remove
lenso app check
lenso app show
lenso run
```

There is no `lenso.app.json`, `lenso.app.toml`, `--definition`, or separate
`plugin verify` workflow.

## Target project shape

```text
plugins/
  lenso.agent.loop/
    agent.toml
    subagent.toml
  lenso.agent.workspace-read/
    default.toml
  company.uppercase/
    plugin.lenso-plugin
    default.toml
```

The directory supplies Plugin identity, the TOML filename supplies Instance
identity; together they form the App-local Instance identity. The TOML body is
direct Plugin configuration. Generated Plugin
Descriptors and the Host Catalog supply Slots, Capabilities, execution,
placement constraints, and safe defaults. `<instance>.disabled` is the only
explicit absence marker.

## Ownership and seams

- `lenso-app-plan` owns portable Host Catalog and Plugin Root snapshot data,
  deterministic resolution, Change Proposal data, and immutable Plan output.
  It does not read files or acquire packages.
- Runtime/SDK repositories derive Plugin Descriptors, embedded Releases, and
  the immutable Host Catalog from source. Existing `Module*` values may remain
  private lowering only during migration.
- `lenso-cli` owns filesystem discovery, atomic Plugin Root edits, migration
  UX, and inspection. Every mutation runs the same pure resolver and Ready Gate
  before committing files.
- Product Hosts such as Agent Harness own their root Slots, default Plugin
  Instances, replacement policy, authority ceiling, and runtime activation.
- Kernel remains unchanged: it receives one complete immutable Plan and does
  not discover Plugins, read TOML, acquire packages, or apply product policy.

## Delivery sequence

### 1. Freeze the contract

- Accept ADR 0070 and the Plugin Root resolution contract.
- Update canonical vocabulary and agent guidance so App Definition and Module
  cannot reappear as public concepts.
- Add architecture conformance fixtures for no-root defaults, direct
  configuration, explicit replacement, disablement, multiple Instances, and
  ambiguous `one` rejection.

Exit gate: every public choice has one owner and one deterministic resolution
rule; no fixture needs a hand-authored binding or placement.

### 2. Add the portable resolution seam

- Introduce serializable `HostCatalog`, `PluginRootSnapshot`, and source
  provenance inputs in `lenso-app-plan`.
- Resolve Host defaults plus explicit Plugin Instances into the existing exact
  App Composition and Plan Snapshot.
- Validate Plugin ID/path agreement, one exact Release per Plugin ID, direct
  configuration after package defaults, disabled markers, Slot cardinality,
  unique Capability binding, execution choice, and authority ceilings.
- Define the TOML authoring profile explicitly, including omission semantics
  for optional values and rejection of any invented generic null/deletion token.
- Produce actionable structured diagnostics and Change Proposals; do not expose
  internal bindings as a fix.
- Replace public `AppDefinition`/`CargoAppDefinition` entrypoints. Keep any
  temporary adapter private and test-only, then delete it before this plan is
  done.

Exit gate: pure resolver tests prove byte-identical inputs produce an identical
Plan and every ambiguity fails closed before Plugin code runs.

### 3. Generate the Host and Plugin inputs

- Make Plugin source derive the exact Descriptor required by the portable
  resolver, including safe package defaults and Slot Entries.
- Add `embedded-rust` as a Plugin Release distribution mode that reuses the
  linked Host implementation without exposing a second authoring abstraction.
- Generate one immutable Host Catalog from root Slot declarations, embedded
  Releases, default Instances, replacement policy, execution constraints, and
  authority ceilings.
- Reject runtime descriptor discovery and any Host catalog that is assembled by
  executing Plugin code.

Release checkpoint: publish and verify exact Runtime/SDK versions and source
tags before the CLI or Harness consumes the contract.

### 4. Replace App Definition CLI flows

- Discover one `plugins/` root from the current project and snapshot it without
  following escaping symlinks or accepting normalized-name collisions.
- Implement `plugins list/add/configure/disable/enable/remove` as candidate
  transactions: stage, resolve, Ready-check, then atomically commit the minimum
  file change.
- Make `app check` and `app show` operate on the derived App without a Definition
  argument. Keep `app resolve --output` only as advanced derived evidence.
- Add `lenso app migrate` that converts an existing App Definition and proves
  semantic equivalence before moving the old file aside.
- Retire `app add/remove --definition`, `--definition`, App Definition schemas,
  central enabled lists, and hand-authored binding or lane options with
  actionable diagnostics.

Release checkpoint: publish and verify exact Cargo/npm CLI artifacts before
the Harness consumes the new workflow.

### 5. Migrate Agent Harness defaults and differences

- Express every embedded Harness behavior as an exact embedded Plugin Release
  in the generated Host Catalog.
- Classify root Slots as required, optional, or replaceable and prove the empty
  Plugin Root boots the useful base Harness.
- Move current per-Instance values from `lenso.app.json`, `config/modules/`, and
  any Plugin enabled list into direct `plugins/<plugin-id>/<instance>.toml`
  files only where they differ from safe package or Host defaults.
- Remove the Harness App Definition, central local overlay, compatibility
  Module configuration, manual bindings, decisions, lanes, and host-package
  metadata.
- Preserve secrets as references and fail closed on authority expansion.

Exit gate: a clean checkout, an empty Plugin Root, a configured embedded
Instance, a bundled third-party Plugin, replacement, disable/enable, removal,
and failed candidate all have end-to-end runtime proof.

### 6. Retire compatibility and prove the public model

- Remove public `Module*`, App Definition, Definition-path, and old config-path
  code after all first-party consumers migrate.
- Search shipped CLI help, schemas, diagnostics, templates, documentation, and
  generated examples for retired vocabulary and workflows.
- Independently install the published CLI and execute clean-room Plugin author
  and App-owner workflows against the published Harness.
- Prove failed migration, invalid config, ambiguous provider, corrupt bundle,
  denied authority, and failed Ready Gate leave both Plugin Root and running
  App unchanged.

## Done criteria

- Starting the Harness with no App manifest and no explicit Plugin files runs
  its complete supported base behavior.
- The only authored App differences are Plugin bundles, direct Instance TOML,
  and disabled markers under `plugins/`.
- Required embedded, optional bundled, and installed behavior share one Plugin
  identity, Descriptor, lifecycle, configuration, and diagnostic model.
- A uniquely compatible Plugin resolves automatically; an ambiguous `one`
  Slot names every candidate and tells the owner which Plugin to disable or
  remove without exposing a binding editor.
- No public API, CLI help, diagnostic, template, or product configuration asks
  for App Definition, Module, package table, binding, execution class, lane,
  `host_package`, or Plan input.
- Generated Plans remain complete and deterministic, and Kernel conformance is
  unchanged.
- Published Runtime, CLI, and Harness artifacts pass the clean-room and
  rollback-safety proof.

## Escape conditions

Stop and return to architecture review if implementation requires any of the
following:

- a second central App or Plugin manifest;
- choosing among ambiguous providers by discovery order, priority, or newest
  version;
- executing Plugin code during discovery or admission;
- placing filesystem, acquisition, or product policy in Kernel;
- an unrestricted local configuration overlay;
- mutating Plugin Root files before candidate resolution and readiness pass;
  or
- exposing compatibility `Module*` values to an author, App owner, or operator.
