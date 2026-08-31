# Plugin Root resolution roadmap

This roadmap tracks the remaining migration from public App Definitions and
Module configuration to one Plugin model. ADR 0069 and ADR 0070 own the
architecture decisions; this document records only the unfinished delivery
outcome and gates.

## Outcome

An App owner starts a Host with useful defaults and changes the derived App
only by adding, configuring, disabling, enabling, or removing Plugin Instances
under `plugins/`. The owner does not author an App manifest, package table,
binding, execution lane, generated descriptor, or Plan input.

Plugin authors keep one source-to-runtime workflow:

```text
lenso plugin new -> check -> dev -> pack
```

App owners use the derived App:

```text
lenso plugins list|add|configure|disable|enable|remove
lenso app check
lenso app show
lenso run
```

## Ownership

- `lenso-app-plan` owns portable Host Catalog and Plugin Root snapshot data,
  deterministic resolution, Change Proposals, and immutable Plan output. It
  does not read files or acquire packages.
- Runtime and SDK repositories derive Plugin Descriptors, embedded Releases,
  and the immutable Host Catalog from source.
- `lenso-cli` owns filesystem discovery, atomic Plugin Root edits, migration,
  and inspection. A mutation must resolve and pass readiness before files are
  committed.
- Product Hosts own root Slots, default Plugin Instances, replacement policy,
  authority ceilings, and runtime activation.
- Kernel receives one complete immutable Plan. It does not discover Plugins,
  read configuration files, acquire packages, or apply product policy.

## Delivery gates

1. Accept the Plugin Root contract and add conformance fixtures for defaults,
   direct configuration, replacement, disablement, multiple Instances, and
   ambiguous `one` rejection.
2. Resolve `HostCatalog` plus `PluginRootSnapshot` deterministically in
   `lenso-app-plan`; reject ambiguous providers and authority expansion before
   Plugin code runs.
3. Generate exact Plugin Descriptors, embedded Releases, and the Host Catalog
   from source without executing Plugin code during discovery.
4. Replace App Definition CLI flows with transactional `plugins` commands and
   derived `app check`/`app show`; migrate existing definitions only after
   proving semantic equivalence.
5. Express Agent Host defaults and differences as Plugin Releases and direct
   Instance configuration, then remove its App Definition, compatibility
   Module configuration, manual bindings, and central enabled list.
6. Remove remaining public `Module*`, App Definition, definition-path, and old
   configuration surfaces after every first-party consumer has migrated.

Runtime and CLI release checkpoints remain required before downstream
repositories consume new public contracts.

## Done

- A Host boots its supported default App with no authored App manifest.
- Authored App differences are Plugin bundles, Instance TOML, and disabled
  markers under `plugins/`.
- Embedded, bundled, and installed behavior share one Plugin identity,
  descriptor, lifecycle, configuration, and diagnostic model.
- Ambiguous Slot resolution fails closed and names the candidate Plugins.
- No public API, CLI help, template, schema, or diagnostic asks an App owner for
  a Module, App Definition, package table, binding, execution class, lane, or
  Plan input.
- Published Runtime, CLI, and Agent artifacts pass clean-room and rollback
  safety proof.

## Stop conditions

Return to architecture review if delivery requires a second central manifest,
discovery-order provider selection, Plugin execution during discovery,
filesystem or product policy in Kernel, unrestricted configuration overlays,
mutation before candidate readiness, or public compatibility `Module*` values.
