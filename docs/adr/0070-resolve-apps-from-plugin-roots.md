---
status: accepted
---

# Resolve Apps from Plugin Roots instead of App Definitions

An App is resolver output, not a document that an App owner must assemble.
Each Host declares its root Slots, exact embedded Plugin Releases, default
Plugin Instances, and replacement policy. The App owner expresses only
differences from those defaults in one Plugin Root: direct per-Instance TOML
configuration, optional exact Plugin bundles, and explicit disabled markers.
The resolver validates that input against generated Plugin Descriptors and the
Host Catalog, explains ambiguity as a Change Proposal, and materializes
the immutable App Composition and Resolved App Plan consumed by the Kernel.

This decision supersedes ADR 0068's `lenso.app.json` and separate local-overlay
authorities, the hand-authored App Composition input in ADR 0045, and the
App-Definition update in ADR 0057. It preserves their important invariants:
configuration is typed and fail-closed, package defaults are safe, acquisition
happens above Kernel, bindings are completely resolved before activation, and
Kernel executes only an immutable Plan.

## Considered options

- A smaller `lenso.app.toml` still makes every App owner understand packages,
  Instances, bindings, lanes, and Host defaults that the resolver already
  knows.
- One central Plugin table is simpler than the old App Definition but becomes
  a second catalog beside the Plugin files it names.
- Convention-only auto-discovery hides conflicts and authority changes. Lenso
  instead infers only from exact paths, generated Descriptors, and Host-owned
  Slot rules, and rejects every unresolved choice.

## Consequences

- `lenso.app.json` and `lenso.app.toml` are retired public inputs. There is no
  replacement central App manifest.
- `plugins/<plugin-id>/<instance>.toml` contains only that Plugin Instance's
  configuration. Identity comes from the path; execution, Capabilities, Slots,
  and configuration Schema come from its generated Descriptor.
- A required embedded Plugin with default configuration needs no project file.
  A non-embedded Plugin carries an exact `plugin.lenso-plugin` bundle in its
  Plugin directory. Generated lock and Plan artifacts may exist, but users do
  not author them.
- `<instance>.disabled` is the only absence marker. It keeps disablement local
  to the affected Plugin instead of creating a central enabled list.
- An explicit provider may replace a Host default only where the Host marks the
  Slot replaceable. More than one explicit candidate for a `one` Slot is an
  actionable error; the resolver never chooses by discovery order or version.
- Host Slots expose only cardinality, replacement, and deterministic ordering.
  The earlier `add`, `provide`, `intercept`, and `mount` kind labels are retired
  because they add no independent resolution behavior.
- App inspection operates on the derived result: `lenso app check` validates
  it and `lenso app show` explains defaults, replacements, bindings, and source
  provenance. `lenso app add/remove --definition` and `--definition` disappear.
- The exact Plugin Root contract and deterministic resolution rules live in
  [`plugin-root-resolution.md`](../architecture/plugin-root-resolution.md).
