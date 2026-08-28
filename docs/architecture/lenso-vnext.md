# Lenso vNext architecture

## Status

This document describes the vNext target and the implementation boundary that
started in the minimal vNext workspace. Accepted ADRs from 0030 onward are the
authoritative decisions; this overview routes readers through them without
repeating every detailed invariant. The final v0.3.x source is retained by the
`lenso@0.3.47` tag and Git history.

## Shape

```text
Host Catalog + Plugin Root
             |
          resolver
             |
     derived App Composition
             |
      Resolved App Plan
                  |
        +---------+----------+
        |      thin Runner   |
        | Runtime Driver     |
        |                    |
        | portable Kernel    |
        | graph / lifecycle  |
        | bindings / invoke  |
        | scopes / supervise |
        | diagnostics        |
        +----+-----------+---+
             |           |
      Native Rust     Bun process       future host-supported
      Adapter         Adapter           Execution Adapters
             |           |
           Plugin Instances providing and requiring Capabilities
```

The Kernel is deliberately smaller than the product. Its deletion would force
every Runner and Execution Adapter to reimplement deterministic composition,
lifecycle, bounded invocation, cancellation, supervision, and diagnostics.
HTTP, PostgreSQL, Auth, Console, Story, and similar features do not pass this
deletion test because their complexity disappears when their Plugins are not
selected.

Repository ownership is narrower than the current migration workspace. The
main repository owns Plan, Kernel, and Kernel conformance; host runtimes,
Adapters, protocols, Plugins, tooling, and examples depend inward and move to
their named owners under [ADR 0064](../adr/0064-keep-only-portable-core-ownership-in-the-main-repository.md).

## Composition and packages

App Composition is declarative, language-independent resolver output. A Host
supplies root Slots, exact embedded Plugin Releases, default Instances, and
replacement policy; the App owner expresses only differences in one Plugin
Root. Authoring tools combine those inputs into one exact Resolved App Plan.
Kernel executes that Plan and performs no package acquisition, SemVer
selection, schema diff, signature admission, or graph discovery.

[ADR 0065](../adr/0065-govern-dynamic-plugins-above-the-kernel.md)
defines the Plugin control plane for a precompiled App, attached through
product-owned Slots and kept above Kernel. [ADR
0066](../adr/0066-derive-module-descriptors-and-plans-from-source.md) makes
behavior source the single source of truth with generated, locked Descriptors,
Schemas, and Plans, and [ADR
0067](../adr/0067-transition-between-immutable-plan-snapshots.md) lets an App
execute a totally ordered sequence of immutable Plan Snapshots with validated
atomic Transitions, keeping App Generation swaps for structural change. The
[multi-execution companion](plugin-execution-classes.md) keeps Plugin
governance orthogonal to native, Process, Wasm, QuickJS, and trusted dylib
Execution Adapter choices and forbids runtime Artifact fallback.

ADR [0069](../adr/0069-use-plugin-as-the-only-application-behavior-unit.md)
retires Module as a public behavior model, and ADR
[0070](../adr/0070-resolve-apps-from-plugin-roots.md) retires the App Definition
and derives Apps from Host defaults plus a Plugin Root.

See ADRs [0031](../adr/0031-separate-capability-contracts-from-module-packages.md),
[0034](../adr/0034-make-app-composition-the-capability-binding-authority.md),
[0045](../adr/0045-materialize-a-resolved-app-plan-before-boot.md), and
[0057](../adr/0057-make-module-installation-an-authoring-operation.md).

The executable contract is documented in [Lenso authoring tooling](lenso-authoring.md)
and implemented by the `lenso-cli` repository. Its public CLI exposes Plugin
authoring through `plugin new`, `check`, `dev`,
and `pack`, plus Plugin Root management and derived App inspection. Plugin
packages provide locked deny-safe configuration defaults; direct Instance TOML
contains only explicit differences. The resolver validates and materializes
one complete canonical configuration per Instance before producing the next
Plan Snapshot; Kernel never reads or merges configuration sources. See the
[Plugin Root contract](plugin-root-resolution.md).
Resolution, Adapter assembly, and Plan execution remain library or product-Host
implementation; Kernel receives only immutable Plan bytes.

## Kernel and hosts

Kernel is one portable asynchronous state machine. A Runtime Driver supplies
host scheduling and monotonic time; Execution Adapters supply Plugin generation
and endpoint mechanics. Native Tokio is one Driver implementation rather than a
Kernel dependency. The same engine compile-checks for native,
`wasm32-unknown-unknown`, and `wasm32-wasip2`; each host supports only the
Adapters it can actually provide.

See ADRs [0047](../adr/0047-scope-runtime-work-to-module-lifecycles.md),
[0048](../adr/0048-make-supervision-execution-adapter-aware.md),
[0053](../adr/0053-run-the-kernel-on-a-portable-runtime-driver.md), and
[0054](../adr/0054-layer-the-rust-implementation-around-the-portable-kernel.md).

## Plugins and lifecycle

A Plugin Release publishes data Descriptors plus factories understood by its
Execution Adapters. A Resolved Plan may instantiate the same package several
times. Boot validates the graph, prepares every Instance, activates providers in
dependency order, and opens one App Ready Gate only after full activation.
Managed tasks and resources belong to one Instance generation; restarts create a
new generation and preserve stable consumer handles when the Adapter supports
recreation.

Native Rust Plugins are statically linked Cargo dependencies in v1. Bun Plugins
run through the first process Adapter, initially one process per Instance by
default. That topology is an Adapter choice, not a Kernel contract.

See ADRs [0032](../adr/0032-keep-static-capability-bindings-through-provider-restarts.md),
[0046](../adr/0046-use-staged-all-or-nothing-app-activation.md),
[0055](../adr/0055-statically-link-native-rust-modules-in-v1.md), and
[0056](../adr/0056-keep-the-module-runtime-interface-minimal.md).

## Capabilities and invocation

Capabilities are independently versioned deep role Interfaces. A small
Descriptor declares stable Operations and request, stream, or event interaction
kinds. Portable contracts use JSON Schema 2020-12 plus a minimal value profile
and generate Rust and TypeScript clients and providers. Native Rust dispatch can
remain typed and direct; cross-runtime Adapters validate their wire boundary.

The [`lenso-contract-codegen`](https://github.com/LioRael/lenso-protocols/tree/f8575ab93a6442dca96e02d4785db6f25f70846b/crates/lenso-contract-codegen)
authoring tool, now owned by `lenso-protocols`, treats the Descriptor and
resolved package-local Schemas as one source. It emits deterministic Rust,
TypeScript, Component Model WIT, and byte-Adapter Rust codec artifacts, checks
the decimal-string/base64/time/missing-value
profile, lints additive minor evolution, and fails when checked-in generated
artifacts drift.
Generated metadata carries the Capability `namespace.name@major` identity and
exact Descriptor SemVer; Plugin Release versions are not part of that identity.
Generated Rust values include serde wire codecs and generated TypeScript values
include JSON codecs; both preserve optional-versus-null fields and open unknown
Domain Error code/payload pairs. Unsupported unions and object shapes that
would discard wire data fail generation before artifacts are written.
Bindings belong to the Capability contract package rather than to every Plugin
implementation package. A Rust consumer and provider reuse one generated
crate for a selected Descriptor so their Rust types remain identical; a Bun
consumer or provider installs the corresponding generated TypeScript package.
Generation is an authoring/build step, and the initial Rust-plus-TypeScript
proof does not imply that every contract must publish bindings for every
language. Future targets are selected by the real Execution Adapters and
consumers that support them.
The binding surface covers Request, Stream, and Event Operations. The Kernel
owns their portable interaction semantics, while each selected Runtime Adapter
owns physical transport and host failure mechanics. Stream bindings preserve
the typed contract across Adapter-owned framing; Event bindings preserve
volatile independent bounded admission without implying durable delivery.

Request has one terminal result. Stream is bidirectional with independent
half-close, bounded flow, cancellation, and an explicit terminal outcome. Event
fan-out performs independent bounded admission and reports partial outcomes; it
is volatile and never implies persistence or redelivery.

See ADRs [0033](../adr/0033-use-request-stream-and-event-capability-interactions.md),
[0035](../adr/0035-allow-adapter-specific-capability-dispatch.md),
[0049](../adr/0049-define-a-portable-json-value-contract.md),
[0050](../adr/0050-generate-and-conformance-test-portable-bindings.md),
[0051](../adr/0051-use-explicit-partial-admission-for-ephemeral-events.md),
[0052](../adr/0052-give-streams-explicit-flow-and-terminal-semantics.md), and
[0059](../adr/0059-stream-bidirectionally-between-rust-and-bun.md).

## Identity and invocation authority

Kernel establishes caller Plugin identity and transports opaque ordinary or
sealed extensions without interpreting their domains. Auth Plugins turn
protocol-neutral Credential Evidence into short-lived ActorAssertions. Provider
SDKs validate and project those assertions into typed domain Actors, and the
target Plugin performs final authorization. There is no ambient System Actor,
universal grants array, or automatic anonymous identity.

See ADRs [0037](../adr/0037-preserve-provenance-of-sealed-invocation-extensions.md),
[0038](../adr/0038-separate-caller-module-actor-assertion-and-actor.md),
[0039](../adr/0039-make-authentication-protocol-neutral-and-explicit.md), and
[0040](../adr/0040-attenuate-actor-assertions-across-capabilities.md).

## State, diagnostics, and product Plugins

Kernel state is volatile. Stateful Plugins own schema meaning, migrations, and
transaction scope behind private persistence Adapters or deep semantic
Capabilities. Workflow, Outbox, durable Event delivery, Secrets, Audit,
OpenTelemetry, Story, and health are optional Plugins or owner-local behavior.

Runtime Diagnostics expose bounded, non-blocking, best-effort structural facts.
Observers cannot block or change App behavior, and diagnostic records exclude
payloads, secrets, configuration, and ActorAssertions.

See ADRs [0036](../adr/0036-expose-non-blocking-runtime-diagnostics.md),
[0041](../adr/0041-keep-persistence-owned-by-stateful-modules.md), and
[0042](../adr/0042-keep-migrations-and-distributed-consistency-out-of-the-kernel.md).

### Optional OpenTelemetry Plugin

The independently owned
[`lenso-otel-module`](https://github.com/LioRael/lenso-otel-plugin/tree/856190e128605479becb484a790368307085428c)
package subscribes to the externally supplied Runtime Diagnostics port and owns
its bounded asynchronous exporter tasks. It converts structural diagnostics to
OTel Logs and accepts explicitly authored OTel Span, Metric, and Log signals
only when the App selects the Plugin and its declared Capability binding.
Exporter failure, queue drops, and cancellation are best-effort telemetry
outcomes and do not change App behavior.

The package also owns W3C Trace Context propagation through the sealed
`lenso.otel.trace-context` extension. Issuer provenance, exact target audience,
and proof survive native and Bun Adapter hops; the receiver verifies them
before interpretation. The payload remains free of request bodies, secrets,
configuration, domain error bodies, arbitrary extensions, and ActorAssertions.

Telemetry is operational observation, not durable business evidence. Audit and
Story Plugins remain the owners for compliance history, durable events, and
replayable business narrative. Removing this package leaves Kernel diagnostics,
Invocation Context, and Adapter contracts unchanged; see
[ADR 0061](../adr/0061-export-opentelemetry-from-a-removable-module.md).

## Console and UI

A target-owned App Web UI is an optional ordinary Plugin composition inside the
target App. A Web Shell binds `many` UI Contribution providers, while a Browser
Adapter projects generated clients only for portable Capability requirements
declared by those contributions and resolved before boot. A Plugin Release may
ship explicit backend and UI entrypoints without introducing a Console or
Plugin runtime type.

A Console becomes an independent Lenso App when it is a cross-App operator
product with remote or multiple targets, an independent operator trust domain,
durable cross-target state, or an independent release lifecycle. Only that
shape requires target Apps to opt into a thin Connector with an explicit
portable Capability allowlist. PostgreSQL, Outbox, Story, Audit, and target
catalog choices remain conditional Plugins rather than Console or Kernel
requirements.

UI Contributions are Capabilities rather than Console-specific Surfaces. A
user-selected local bundle, package, or remote ESM is trusted application code;
generated Browser clients are a composition and ergonomics seam rather than a
same-realm security sandbox. An independent Console does not automatically
execute UI code advertised by a connected target; executable UI must be
selected independently by the Console App Composition.

The first executable proof is preserved at the pinned pre-extraction
[`fixtures/vnext-web-ui`](https://github.com/LioRael/lenso/tree/67d21499548d07e92c2f6529d7c8345e58c067d9/fixtures/vnext-web-ui)
baseline. Its Web Shell validates and assembles `many`
`lenso.ui.contribution@1` providers behind the portable `lenso.web.shell@1`
Interface. The Browser Adapter starts accepting HTTP only after the App Ready
Gate opens and has a fixed generated-client projection for the exact portable
requirement declared by the selected contribution. The client JavaScript is
generated from the same validated Capability Descriptor IR as the Rust and
TypeScript bindings and preserves the typed domain/runtime result envelope. The
profile may inject a recorder for tests or a host system-browser launcher;
either launcher is called only after readiness. The `fixture.orders` package
exposes separate `backend` and `ui` entrypoints; removing the UI entrypoint,
Shell, and Browser Adapter leaves the business binding usable. This trusted
same-realm fixture is historical composition evidence, not hostile-code
isolation or a reason to return product code to portable core.

See ADRs [0043](../adr/0043-represent-ui-contributions-as-capabilities.md) and
[0060](../adr/0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md).

## Deferred distributed direction

Local Interfaces intentionally preserve handles, portable contracts, deadlines,
cancellation, and Adapter seams that could support remote execution later. v1
does not define discovery, placement, replicas, rolling upgrades, dynamic graph
mutation, or a Lenso Control Plane. Revisit them only from a concrete App that
needs distribution; see
[`future-directions/distributed-plugin-runtime.md`](future-directions/distributed-plugin-runtime.md).

## Legacy cutover

The repository-wide ownership inventory, retained black-box behaviors,
compatibility boundaries, data and package transitions, rollback points, and
deletion gates are recorded in
[`legacy-migration-and-retirement.md`](legacy-migration-and-retirement.md).
The plan is pinned to explicit `main` and `next` source baselines and does not
turn the v0.3.x Service architecture into a vNext compatibility workspace.
