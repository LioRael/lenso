# Lenso Engine

This page covers repository-level App and Host integration with the Engine. The
[packaged Engine guide](../../crates/lenso-cli/docs/engine.md) is the
authoritative reference for the Engine's processor protocol, workflows,
embedding API, incremental sessions, explainability, and core limits. It is
also included with the published `lenso-cli` crate.

## Packages

See the [Engine package ownership table](../../crates/lenso-cli/docs/engine.md#packages).
The Rust Engine, App authoring, Runtime, SDK, and CLI crates now share this
workspace while retaining their crate and dependency boundaries.

## Start without configuration

The [Engine guide](../../crates/lenso-cli/docs/engine.md#start-without-configuration)
shows the App-free Markdown workflow. App builds opt into the `AppProject`
processor; the CLI consumes Engine's App discovery and assembly APIs.

### App facts and MCP

`lenso app facts --root ./my-app --json` is the read-only Agent/tooling
projection. It reports exact adopted Plugin versions, resolved execution and
Capability metadata, source locations, Plugin Root revision,
discovered-but-not-adopted source candidates, and stable diagnostic codes.
Schema version 5 includes each instance's exact Plugin Root
configuration-source digest, including the absence of a Root override. That
digest does not identify Host defaults or resolved configuration values.
The Host target comes from persisted build or distribution evidence, not the
machine running the inspection. Missing evidence reports `unknown`;
conflicting target metadata reports `LENSO_HOST_TARGET_UNVERIFIED`. Runtime
state remains `not_observed` until a runtime control surface supplies evidence;
build artifacts are not proof that an App is running. Configuration values are
not included.

For an external Plugin Root, pass its exact distribution authority as
`--host-build DIST/.lenso/host-build.json` to `app facts`. This verifies and
includes external configuration status without treating the last activation
receipt as current process health. Host-default source locations then point to
the actual `host-build.json` rather than a nonexistent Host catalog.

`lenso mcp --root ./my-app` exposes the same resolver-backed facts projection
over stdio. It accepts the same optional `--host-build` for an external fixed
root and omits configuration values. Without permission flags,
`project_facts`, `project_explain`, and `project_check` inspect that root
without changing files. `project_explain` requires a built Host root with the
persisted bundle inventory and returns the same `lenso.app-explain.v1`
projection as `app explain --json`; `project_check` uses the same resolver as
`app check --json`. The `--host-build` option applies only to `project_facts`.
Selection and configuration previews do not publish changes and require local
Host authority. Inspection does not infer runtime readiness.

An opt-in `lenso mcp --root APP --linked-snapshot SNAPSHOT --trust TRUST`
also exposes `linked_catalog`. It reads the same signed source-only candidate
projection as `app linked-catalog`, with a query, target, offset, page limit of
20, and bounded output. It neither downloads a crate nor claims unverified
permissions, dependency compatibility, or runtime readiness. Catalog and
document inspection remain stateless. `app add` and opt-in MCP linked adoption
store an App-local accepted-catalog checkpoint; neither inspection path
discovers revocations without a newer signed snapshot. Add
`--allow-document-fetch` only when the MCP client may contact signed
third-party HTTPS documentation URLs. The `linked_document` tool verifies the
exact release, document revision, size, and digest before returning a bounded
Markdown chunk marked as untrusted data.

`--allow-changes` enables reviewed Plugin Root proposal application and exact
linked Cargo adoption or withdrawal. Adoption also requires fixed
`--linked-snapshot`, `--trust`, and `--linked-crate` inputs at server startup,
plus a separate build and check before use. `--allow-build` enables bounded
App build, status, and cancellation tools. `--allow-run` enables start,
status, and stop for a built App, with readiness reported from that run. These
operations use client request IDs; none publishes a release or deploys an App.

For an npm-only Plugin release, start the bridge with exact
`--package-snapshot`, `--package-trust`, and `--package-tgz` files.
`project_npm_preview` verifies the signed release and archive digest for the
fixed App without changing it. `project_npm_adopt` and
`project_npm_unadopt` additionally require `--allow-changes` and use the same
source App adoption path as the CLI. Adoption deliberately uses
`--no-install`: it does not contact a registry or run package scripts.
Dependencies, exact build-code trust (the MCP
`--trust-adopted-build` startup grant), build, check, and runtime activation
remain separate steps.

## Local plugin sources and presets

See the [Engine workflow and local processor guide](../../crates/lenso-cli/docs/engine.md#local-plugin-sources-and-presets).
These processor manifests are Engine inputs, not App Plugin Root authority.

## Language-neutral processing

See the [Engine process protocol](../../crates/lenso-cli/docs/engine.md#language-neutral-processing).
It remains distinct from App Plugin execution and Capability binding.

## Published App resources

An optional App Plugin can declare small, immutable data files for a consuming
Host without teaching Engine what those files mean. In Cargo metadata, use
`[package.metadata.lenso-cli]`, which is Engine build metadata and stays out of
the SDK-owned Plugin manifest; in Bun metadata, use the `lenso` object:

```toml
[package.metadata.lenso-cli]
outputs = ["wasm", "process"]
published_resources = [
  { path = "agent/deployment.json", schema = "example.agent-deployment@1" },
]
```

```json
{
  "lenso": {
    "pluginId": "example.orders",
    "rootSlot": "tool-providers",
    "runtime": "bun",
    "published_resources": [
      {"path": "agent/deployment.json", "schema": "example.agent-deployment@1"}
    ]
  }
}
```

`app build` accepts only declared regular files beneath the selected Plugin
project. It copies them to `dist/resources/<plugin-id>/…` and writes
`dist/resources.json`, including the owner, schema, relative output path,
SHA-256 digest, and size. It rejects duplicate declarations, links, path
traversal, invalid resource schemas, more than 64 files, and more than 16 MiB
per Plugin. The resource schema is owned by the consumer: Engine copies and
inventories bytes, but never activates a Plugin, runs a resource, or interprets
its payload.

Consumers should verify both the generic inventory and any related Bundle
identity before importing a resource. This supports optional conventions from
multiple languages and Plugin types while keeping `app/` layout out of Engine
core and runtime authority.

### Resource-only convention outputs

A selected convention compiler may contribute only immutable data. It writes
`lenso.convention-resources.json` into its otherwise bounded compiler output:

```json
{
  "schema": "lenso.convention-resources.v1",
  "resources": [
    {"path": "deployment.json", "schema": "example.deployment@1"}
  ]
}
```

The file is mutually exclusive with `Cargo.toml`, `package.json`, and a Plugin
Bundle manifest in that output. The Engine validates the same regular-file,
path, schema, count, and byte bounds as ordinary published resources, assigns
the selected convention surface identity as the inventory owner, and copies
the declared files to `dist/resources/<contribution-id>/…`. It does not
install Bun or Cargo dependencies, build a Bundle, add a Plugin to the Host,
or turn the contribution identity into runtime authority. This lets a
convention publish a Profile, route manifest, schema, or other consumer-owned
data without inventing an empty executable Plugin.

## Runtime and bootstrap boundary

The [Engine Runtime and bootstrap guide](../../crates/lenso-cli/docs/engine.md#runtime-and-bootstrap-boundary)
describes the selected processor's Capability, Kernel generation, and
precompiled bootstrap. App composition and Host policy remain separate from
that processing graph.

## Incremental results and resources

See the [Engine session and publication guide](../../crates/lenso-cli/docs/engine.md#incremental-results-and-resources).
App resource inventories above have their own consuming Host boundary.

## Versioned external configuration snapshots

`lenso-engine-authoring` accepts explicitly scoped external configuration
snapshots without adding a second App graph. The Host supplies both the source
identity and an object/top-level-field authorization; the document cannot
authorize itself. `FilePluginConfigurationSnapshotSource` reads a bounded
regular JSON file without following symlinks on Unix or reparse points on
Windows. `HttpsPluginConfigurationSnapshotSource` polls one Host-admitted
HTTPS origin with public-DNS enforcement, redirects and environment proxies
disabled, bounded identity responses, and optional ETag/HTTP 304 revalidation.
Its cursor binds the ETag to the exact endpoint and Host source identity; it
cannot be reused for another source.

An operator-pinned Process V2 Bundle may also provide the generated
`lenso.configuration.source@1` Request Capability. The Host verifies exact
Bundle/Artifact digests from its protected bootstrap policy, starts a separate
two-Instance Kernel Plan before the business App Plan, invokes `fetch`, and
closes that source generation. Its response contains only revision and values;
the Host binds identity and field authorization afterward. This first Process
path is a trusted native implementation with a 1 MiB wire-frame ceiling, not
an OS sandbox, subscription claim, or marketplace signature claim.
`propose_versioned_plugin_configuration_snapshot` routes every authorized
entry through the existing typed Plugin Root proposal, Host admission, and
revision checks without mutating the Root. Authorized field updates merge
into the current instance source, preserving fields owned by other
authorities.

Persist the returned `PluginConfigurationSnapshotIntent` before publishing
its paired proposal. The intent records both base and candidate Root
revisions, so recovery can distinguish not-yet-published from
crash-after-publication. Promote it to active Host state only after the
resulting App Generation becomes active. An external revision whose merged
values already match the Root is reported as `NoRootChange` and needs no Root
publication. Older revisions, reused revision numbers with different
content, changed sources, invalid Plugin values, and local Root drift fail
closed. Package fields marked `x-lenso-sensitive` accept only
`{ secret_ref = "..." }`; secret material remains with the Host's secret
provider and is not echoed by rejection diagnostics. File publication, App
Generation switching, and upstream source acknowledgement are separate
operations rather than a cross-system atomic write. Transport failure returns
no snapshot or acknowledgement and never mutates the Root. The Host retains
the last accepted intent/active Generation and may retry the same
source-bound cursor after connectivity returns.

For a source App in local development, `lenso app dev --root APP
--configuration-policy /absolute/policy.json --configuration-poll-seconds 10`
reconciles the configured file or HTTPS source on each bounded poll. An
accepted revision is desired configuration, not an activation (and it may
leave the Root bytes unchanged). For an accepted replacement, the
development supervisor stops the prior Host before checking and starting a
candidate, waits for its actual Ready receipt, then records the exact
activated Root revision. Failed readiness can therefore leave no running
preview. Invalid snapshots and transport outages retain the old Host only
while its accepted source proof is still fresh and its Host policy is
unchanged; expiry or policy change stops it. The last-activated revision is a
historical receipt, not a liveness claim. The default interval is 10 seconds
(allowed range 1–3600); Ctrl-C stops the supervised Host. Source rebuilds
use the same Ready Gate. A fixed listener that cannot coexist with the old
Host may prevent candidate readiness; this development loop does not promise
zero-downtime switching or a production configuration subscription.

For a built distribution, `lenso app start --from DIST
--configuration-policy /absolute/policy.json` continuously checks source
freshness and Host policy. A revision that resolves to the same Plugin Root
keeps the current Host; a Root change hard-stops it before another version
can activate. The supervisor cannot prove that independently grouped child
processes have stopped, so it leaves
`DIST/.lenso/supervised-start.uncertain` and refuses another supervised
start. An operator must verify that the Host and all descendants for that
distribution have exited before removing that fence and starting again. This
path does not automatically replace a running production Host.

This source adapter admits one source identity at a time. When a later
snapshot omits a previously source-owned field or object, reconciliation
restores the displaced App-owned value or removes a field that was absent
before source ownership; unrelated App-owned fields remain untouched. A
Root intent is not shared across independently versioned sources, and
changing source identity fails closed rather than comparing their revisions.
Multi-source ownership, source-identity migration, push subscriptions, and
full active-generation state are separate lifecycle work rather than implied
by this API.

## Limits and trust

The [Engine guide's limits](../../crates/lenso-cli/docs/engine.md#limits-and-trust)
cover core inputs, convention compiler budgets, trusted processors,
cancellation, and host signal policy.

## Development validation

Use the [Engine guide's focused validation](../../crates/lenso-cli/docs/engine.md#development-validation)
and the repository's candidate CI rules in [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Repository layout

Rust workspace members live under `crates/`. Each crate keeps its own tests,
assets, contracts, and focused examples alongside its implementation. Root
`examples/` contains repository-level usage examples. Run Cargo commands
from the repository root; all members share the root lockfile and target
directory.
