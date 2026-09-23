# Lenso authoring tooling

Status: current public authoring contract.

The `crates/lenso-cli` package owns the executable command surface. In this
shared Rust workspace, `crates/lenso-engine-app` and
`crates/lenso-engine-authoring` own App filesystem workflows and embeddable
authoring, while `crates/lenso-app-plan` owns resolution. This follows
[ADR 0077](../adr/0077-consolidate-the-rust-main-chain-by-language-and-product.md):
the CLI invokes these APIs instead of defining a separate repository boundary.

Neither the CLI nor its library installs code into a running Kernel. A Host
resolves a complete candidate App, passes its Ready Gate, and then applies an
immutable Plan Transition or App Generation swap above Kernel.

## Plugin author interface

Every application-behavior author uses one command family:

```text
lenso plugin new <id>
lenso plugin check
lenso plugin dev
lenso plugin pack
```

`new` creates one Plugin project. `check` emits fast actionable diagnostics.
`dev` validates and runs a fresh development App. `pack` validates the exact
bundle it creates when the Plugin is distributed independently. A separate
`verify` command is deliberately absent: `pack` checks authored output, and an
installing Host independently verifies untrusted input.

These commands hide Descriptor lowering, binding closure, Plan serialization,
Execution Adapter assembly, and development Host mechanics behind one deep
interface. `Module` is not a public authoring type.

For a local Rust Capability package, `lenso app contract new <id> --source rust`
records `package.metadata.lenso.contract.source = "src/contract.rs"` in its Cargo
manifest. That metadata, not the filename alone, makes Rust source the
Descriptor authority. An existing code-first package that relied on the old
implicit `src/contract.rs` convention must add the `source` field. A
Descriptor-owned package omits it, even if it has an unrelated
`src/contract.rs` helper; its Descriptor and generated projection remain the
inputs to App builds.

## App-owner interface

A Host supplies its root Slots, embedded Plugin Releases, default Plugin
Instances, and replacement policy. An App owner expresses only differences in
the current project's Plugin Root:

```text
plugins/<plugin-id>/<instance>.toml
plugins/<plugin-id>/<instance>.disabled
plugins/<plugin-id>/plugin.lenso-plugin
```

The TOML body is direct Plugin configuration. Identity comes from its path;
Capabilities, Slots, execution, placement constraints, Schema, and safe
defaults come from generated Plugin and Host artifacts. There is no central
App manifest.

The ordinary commands are:

```text
lenso plugins list
lenso plugins add <bundle>
lenso plugins configure <plugin-id> [instance]
lenso plugins disable <plugin-id> [instance]
lenso plugins enable <plugin-id> [instance]
lenso plugins remove <plugin-id> [instance]
lenso app check
lenso app show
lenso run
```

Mutation commands stage the minimum Plugin Root edit, resolve and Ready-check
the complete candidate, then commit atomically. Direct file edits remain
supported and fail startup closed when invalid. `app check` validates the
derived App; `app show` explains Host defaults, explicit Instances,
replacements, bindings, authority, and provenance.

`lenso.app.json`, `lenso.app.toml`, `--definition`, manual bindings, package
tables, and lane selections are retired authoring interfaces. There is no
compatibility or migration command in the normal CLI.

See the exact [Plugin Root and App resolution
contract](plugin-root-resolution.md).

## Internal Plan seam

The resolver closes every Slot, Capability binding, execution variant, and
placement from the Host Catalog, Plugin Root snapshot, and generated
Descriptors. Library Hosts may inspect, persist, or exchange the resulting
canonical Plan bytes, but those bytes never become author input:

```rust,ignore
let candidate = resolver.resolve(&host_catalog, &plugin_root_snapshot)?;
candidate.require_ready()?;
write_derived_plan(candidate.canonical_plan_bytes())?;
host.apply(candidate).await?;
```

Changing Host defaults, a Plugin Release, direct Instance configuration, or a
disabled marker requires a fresh resolution. Kernel never discovers packages,
selects providers, rewrites locks, reads Plugin Root files, or accepts an
authoring recipe as runtime authority.
