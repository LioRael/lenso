# Plugin authoring paths

## Native Rust linked into a Host

Use the public `lenso` facade and current dependency source:

- `#[lenso::plugin]` defines Plugin identity and generated descriptor/factory;
- `#[lenso::provides(...)]` binds typed Capability implementations;
- `PluginConfig` derives strict typed configuration;
- typed `Port<Client>` fields declare requirements; and
- `NativePluginRegistry::with_linked_factories()` exposes linked availability.

The Host Catalog, not the Plugin, owns default activation, root Slots, and any
private attachment needed to distinguish repeated providers.

## Portable Wasm Plugin

Use the public workflow:

```sh
lenso plugin new uppercase
cd uppercase
lenso plugin check
lenso plugin dev --operation execute \
  --request-json '{"name":"uppercase","arguments_json":"{\"text\":\"hello\"}"}'
lenso plugin pack
```

The scaffold owns one Plugin identity and embeds its descriptor in the built
component. Do not create a Manifest template or a separate internal behavior
unit. `pack` checks the exact bytes it writes.

## Configuration and lifecycle

Package defaults are conservative implementation defaults. Host configuration
is product policy. `plugins/<plugin-id>/<instance>.toml` is the App owner's
typed patch. Secrets remain external references. Use lifecycle only when the
Plugin owns resources or managed work; every prepared generation is fresh.
