# Plugin authoring paths

Choose a path from shipped code, not from a target architecture document. Read
the selected repository instructions and package versions first; the commands
and APIs below are current anchors whose installed `--help` and owner source
remain authoritative.

## CLI Rust Agent Tool

`lenso plugin new` currently scaffolds one ordinary Rust Agent Tool. Its single
`src/lib.rs` can produce portable Wasm and trusted Process implementations of
one Plugin Contract:

```sh
lenso plugin new company.uppercase
cd company.uppercase
lenso plugin check
lenso plugin dev --operation execute \
  --request-json '{"name":"company.uppercase","arguments_json":"{\"text\":\"hello\"}"}'
lenso plugin pack
```

The default `--runtime multi` path emits a V3 `.lenso-plugin` Release with Wasm
and Process implementations. `--runtime wasm` and `--runtime process` narrow
the output. Inspect `lenso plugin new --help` before relying on these choices.
The generated project uses `lenso-plugin-sdk::AgentTool` and
`export_agent_tool!`; it is not a universal scaffold for arbitrary Capability
providers, stateful Plugins, Bun, Web UI, or every interaction kind.

`check` materializes and validates descriptor evidence in a temporary Bundle.
`dev` selects an implementation admitted by its local policy and crosses the
real Adapter boundary. `pack` builds, verifies, and reopens the exact Bundle it
writes. A receiving Host validates the Bundle again during `plugins add`.

## Linked native Rust Plugin

Use the public `lenso` facade and the exact dependency source selected by the
owner repository:

- `#[lenso::plugin]` defines Plugin identity and generated descriptor/factory;
- `#[lenso::provides(...)]` lowers typed Capability implementations;
- `PluginConfig` derives strict typed configuration;
- typed `Port<Client>` or `ManyPort<Client>` fields declare requirements and
  cardinality; and
- `NativePluginRegistry::with_linked_factories()` exposes linked availability.

The Host Catalog, not Plugin code, owns default Instances, root Slots, private
attachments, and activation policy. Generated registration makes a Plugin
available; it does not silently activate an App-owned Instance.

## Bun request Plugin

The supported Bun authoring surface is `@lenso/bun`. Implement a generated
Provider and export one Plugin definition:

```ts
import { definePlugin } from "@lenso/bun";
import { bindJobsProvider } from "@lenso/bun/capabilities/jobs";
import { jobs } from "./jobs.ts";

export default definePlugin({ providers: [bindJobsProvider(jobs)] });
```

The generated entrypoint imports this default export and starts the runtime.
Plugin code does not call `serve`, parse JSON-RPC, or implement process
handshakes. The current public SDK supports request Capabilities; Stream and
Event descriptors fail closed until their typed authoring sessions are shipped.
Use the `lenso-bun-adapter` repository's build, typecheck, package-smoke, and
real `bun_cross_runtime` gates. Do not claim that the Rust CLI scaffold or
`lenso plugin pack` packages a Bun project unless the selected CLI version and
owner repository demonstrate that path.

## Contract and implementation boundary

One Plugin Contract owns Plugin ID/release version, root Slot, configuration
Schema/defaults, provided and required Capabilities, restart policy,
criticality, and state semantics. Each executable implementation owns its exact
runtime package identity, entrypoint, target, and Execution Class.

A Release may publish several implementations only when all project the same
Contract and observable success, Domain Error, cancellation, lifecycle, and
state semantics. Host policy selects one compatible implementation before Plan
resolution. Runtime never benchmarks or falls back after readiness or
invocation failure; selecting another implementation creates a new App
Generation.

## Configuration and lifecycle

Package defaults are conservative implementation defaults. Host configuration
is product policy. `plugins/<plugin-id>/<instance>.toml` is the App owner's
typed patch. Secrets remain external references. Use lifecycle only when the
Plugin owns resources or managed work; every prepared generation is fresh.

The path is selected only when its SDK, interaction kinds, target Adapter,
package lock, and real execution test all exist. Otherwise return the exact
missing prerequisite and the owner repository instead of writing compatibility
glue inside product behavior.
