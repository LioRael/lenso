# Module authoring, Slots, and dynamic resolution

Status: accepted companion contract for
[ADR 0065](../adr/0065-govern-dynamic-plugins-above-the-kernel.md),
[ADR 0066](../adr/0066-derive-module-descriptors-and-plans-from-source.md), and
[ADR 0067](../adr/0067-transition-between-immutable-plan-snapshots.md);
implementation in progress.

This document owns the Interface between Module authors, App owners, product
Slots, the resolver, and the Reconciler. The
[dynamic Plugin control-plane contract](dynamic-plugins.md) continues to own
admission, exact authority documents, App Generation staging, routing, drain,
and rollback. [Plugin execution classes](plugin-execution-classes.md)
continues to own execution mechanics.

`lenso-module-authoring` in `lenso-protocols` owns the portable
`CapabilityClient` and lifecycle-bound `Port<C>` foundation. Generated
Capability bindings now also emit hidden requirement, provider, and native
endpoint metadata. `lenso-runtime-rust` lowers a named-field Module struct and
one annotated generated Provider implementation into configuration decoding,
a type-derived configuration Schema, Port requirements and activation,
endpoint construction, a complete static Descriptor artifact, factory, and
link-time registration. Agent Harness Agent Loop is the first deep proof and
resolves to the previously reviewed Plan byte-for-byte without its former raw
Descriptor, Schema file, factory, endpoint construction, or connection
lifecycle glue.

The first public Rust authoring facade now lives in `lenso-runtime-rust` as the
`lenso` crate. Agent Loop consumes `lenso::prelude`, `lenso::module`,
`lenso::provides`, and `lenso::ModuleConfig` without direct dependencies on
`lenso-module-authoring` or `lenso-native-adapter`; macro expansion keeps the
Adapter, Kernel, inventory, and endpoint glue behind a hidden support seam.
Capability crates remain portable because the consuming Module injects that
support seam rather than making generated Capability bindings depend on one
Execution Adapter. The facade is source-available but remains unpublished
while its package dependencies are Git-pinned.

This is still not the complete public shape below. Request, Stream, and Event
Capabilities generate hidden lowering glue, so the ordinary Rust path is an
inherent `async fn` rather than a generated Provider trait implementation;
boxing, dispatch, endpoint construction, and Provider conformance stay behind
the macros. A Request method may return its domain `Result` directly. A Stream
method returns `ModuleResult<ProviderStream<C>, DomainError>` and writes typed
messages through `ProviderStreamChannel<C>`; `ModuleError` keeps Domain Errors
separate from `RuntimeFailure`. Agent Loop now proves this path without a
module-local `NativeStreamSession`, `Any` erasure, generated Invocation Error,
or Kernel stream item construction. Kernel vocabulary remains inside the
facade and generated lowering rather than the Module implementation. An Event
subscriber receives its generated Event type in an async method and returns
either `()` or `ModuleEventResult`; because publication reports admission rather
than handler completion, a handler Runtime Failure goes to diagnostics and
Module supervision instead of becoming a publisher-visible Domain result. A
freshness-checked executable Event Capability fixture proves async lowering,
wrong-type rejection, and Runtime Failure preservation. No current product
repository owns an Event Module to migrate, so the fixture is deliberately not
presented as product behavior.

The implementation still supports one provided Capability per Module and has a
first portable scalar/container configuration type profile. Bidirectional
provider input is typed, bounded, cancellation-aware, and single-consumer, but
Agent Loop currently uses only the output direction. A broader Schema profile,
multi-Capability provider aggregation, remaining Module migrations, TypeScript
`defineModule`, package/Slot generation, complete Desired State resolver,
Reconciler, and hot Plan Transitions remain. The existing
control-plane slice begins from an exact lock and caller-supplied Instances and
bindings; it is migration input, not evidence that the complete experience
exists.

### Implementation checkpoint: 2026-08-25

Against the 16 final acceptance gates below, the repository family currently
has **0 fully closed, 7 partial, and 9 missing**. The new struct compiler
deepens gates 1 and 15 but does not close them: gate 1 still lacks the complete
TypeScript and install/hot-transition proof, while gate 15 still lacks the
remaining derived artifact classes, publication readiness, and final
domain-method authoring shape.

| State | Gates | Current evidence or principal gap |
|---|---|---|
| partial | 1, 4, 6, 7, 10, 12, 15 | Rust Tool and deep Module proofs, generated Request/Stream/Event domain-method lowering, deterministic static `many`, permission and Generation authorities, data-only Skill behavior, structural replacement slices, provenance inspection, source-first Capability drift plus byte-identical Agent Loop derivation |
| missing | 2, 3, 5, 8, 9, 11, 13, 14, 16 | hot Transition mechanics and conformance, structured owner decisions, named Instance/state proof, atomic Host+Web package, full state evolution, hooks, mounted subgraphs |

## Outcome

One authoring abstraction — **Module** — serves built-in, bundled, and
installed behavior. Everything an author used to restate by hand is derived
from source; everything an App owner used to enumerate by hand is derived
from intent. What each persona must learn:

| Persona | Concepts | Count |
|---|---|---|
| Tool author | the product export (`agent.tool`) | 1 |
| Deep Module author | Module, Capability (Port), Slot, configuration | 4 |
| End user | Plugin (install, enable, configure), occasional decision | ~2 |
| App author | App Definition, generated lock snapshot | 2 |
| Platform developer | everything below | unbounded |

The pipeline:

```text
Module source (one construct per language)
  -> generated Module Descriptor, Schemas, factory, Slot Entries, manifest
App Definition + installed Plugin selections + owner decisions
  -> Desired State
Resolver
  -> derived App Composition -> immutable Plan Snapshot (lockfile)
Reconciler
  -> hot Plan Transition, or App Generation swap when structural
Kernel
  -> executes the snapshot sequence; each snapshot immutable
```

Generated artifacts are inspectable, diffable, and signable authorities. They
are linker output: no persona hand-authors or hand-edits them.

## Authoring shapes

These shapes are the acceptance baseline. A design change that makes any of
them longer or adds a concept to its persona budget must be rejected or must
revise this contract explicitly.

### Stateless Tool, TypeScript

```ts
import { agent } from "@lenso/agent-harness"

export default agent.tool({
  id: "github.search",
  description: "Search GitHub repositories",
  input: { query: z.string().max(256) },
  async run({ query }, ctx) {
    return ctx.http.get("https://api.github.com/search/repositories", { q: query })
  },
})
```

No Lenso JSON, Capability IDs, bindings, lifecycle, factory, registration, or
Composition knowledge. `ctx.http` exists only because the package manifest
requests the HTTPS permission; unapproved handles do not exist at runtime.

### Stateless Tool, Rust

```rust
use lenso_agent::prelude::*;

/// Convert one bounded UTF-8 string to uppercase.
#[agent::tool(id = "text.uppercase")]
async fn uppercase(text: Bounded<String, 4096>) -> Result<ToolOutput, ToolError> {
    Ok(text.to_uppercase().into())
}
```

The macro derives the input Schema from the signature, the Slot Entry for the
Agent Tools Slot, the Module Descriptor, the Execution Adapter factory, and —
for a statically linked build — the host registration. The current 140-line
`NativeModuleFactory` implementation plus four registration sites collapse to
this file.

### Deep Module, Rust

```rust
use lenso::prelude::*;
use lenso_agent::capabilities as agent;

#[lenso::module(id = "com.example.deep-loop")]
struct DeepLoop {
    #[config]
    config: DeepLoopConfig,        // serde type -> configuration Schema
    model: Port<agent::Model>,     // requires, cardinality one
    tools: Port<agent::Tools>,
    session: Port<agent::Session>,
}

#[lenso::provides(agent::Agent)]
impl DeepLoop {
    async fn run_turn(
        &self,
        ctx: Ctx,
        req: TurnRequest,
    ) -> ModuleResult<ProviderStream<agent::Agent>, TurnError> {
        let (stream, output) = ProviderStream::channel(&ctx, 8);
        self.spawn_turn(ctx, req, output)?; // private managed-task helper
        Ok(stream)
    }
}
```

`provides`, `requires`, cardinality, configuration Schema, and state identity
are derived; the author writes domain behavior and typed stream messages while
the facade and generated glue own dispatch, boxing, type erasure, cancellation
plumbing, terminal validation, and translation into `RuntimeFailure`. Offering
this Module to the Agent Loop Slot is one generated Slot Entry
(`agent.loop(DeepLoop)` in the package manifest builder or a macro argument).
The same construct scales from the uppercase tool to an Agent Loop replacement;
there is no second system.

### Capability contract as types

```rust
#[lenso::capability(id = "lenso.agent.tool-provider", major = 1)]
pub trait ToolProvider {
    async fn catalog(&self, ctx: Ctx<'_>) -> Result<Catalog, CatalogError>;
    async fn execute(&self, ctx: Ctx<'_>, req: Execute) -> Result<ToolOutput, ExecuteError>;
}
```

The build emits the canonical JSON Schema and Capability Descriptor as a
locked snapshot committed beside the source. TypeScript and other bindings
generate from the locked snapshot. The portable value profile, additive
minor-evolution lint, and drift gates of ADR 0050 remain in force; only the
direction reverses.

### Data-only package

A Prompt or Skill package declares data entries in its package manifest and
contains no executable Module. Its content is admitted by digest and mounted
into the explicitly selected interpreter Module of its Slot. It installs,
enables, disables, and uninstalls like any Plugin and never gains runtime
callbacks.

### One package, several execution targets

A package may author one Module for Host execution and one for Web execution.
Each derives its own Descriptor and failure semantics; the resolver
materializes separate Module Instances; the user sees one Plugin that
activates atomically or not at all.

## Generated artifacts and lock discipline

`lenso pack` (and the ordinary build for in-tree Modules) derives:

- one Module Descriptor per Module: identity, configuration Schema digest,
  provided Capability Descriptors, required Capabilities with cardinality,
  optional state identity and Schema digest;
- Capability Schema snapshots for contracts the package owns;
- Slot Entries: which Module or data item is offered to which Slot;
- the package manifest: identity, version, entries, Artifacts, permissions,
  target variants; and
- Execution Adapter factories and, for static builds, host registrations.

Every derived artifact is deterministic, committed or published beside its
source, and CI-diffed: regeneration that changes bytes fails the build exactly
as generated-binding drift fails today. Admission, resolution, and signing
consume only these artifacts; they never execute package code to discover
anything.

## App Definition and derived Composition

The App Definition is the only hand-authored composition input:

```rust
lenso::app("agent-cli")
    .with((Cli, Prompt, Tools, Sessions, FileSkills))
    .slot(agent::MODEL, OpenAiCompatible::from_env())
    .lane("io", [Sessions])
```

The resolver closes bindings from Descriptors: a `Port<agent::Tools>` binds to
the one provider of `lenso.agent.tools@1`; the Tools Module's `many` Port
collects every selected Tool provider. Capability IDs, Descriptor versions,
admission limits, and `execution_class` strings never appear in the App
Definition; they are Descriptor and policy facts. Real ambiguity — two Model
providers for a `one` Port — is a structured decision, never silent selection.

The derived App Composition and the Plan Snapshot are lockfile outputs:
committed for built-in Apps, generated per change for dynamic Apps, diffable
in review, and authoritative for execution. The hand-written
`composition/fragments/*.json` layer is deleted, not migrated.

## Slots

A Slot is product-owned and versioned. Its declaration fixes attachment kind,
cardinality, Descriptor constraints, ordering rules, and explanation text, as
ADR 0065 defines. The first Agent Harness Slot Catalog:

| Slot | Kind | Cardinality | Notes |
|---|---|---|---|
| Agent Tools | add | many | deterministic order, per-Instance keys |
| Prompt sources | add | many | ordered assembly |
| Skills | add (data) | many | interpreter: Skills Module |
| Model | provide | one | replacement is an owner decision |
| Agent Loop | provide | one | built-in default; candidate offer only |
| Turn hooks | intercept | ordered | typed phases; owner resolves order |
| Agent Runtime | mount | optional | one closed subgraph, one root Capability |

Adding a compatible candidate to any Slot must not require editing or
rebuilding Agent Harness. A later package-published Slot is allowed only under
the owner-scoped rules in ADR 0065; it is not a first-delivery gate.

## Desired State and Change Proposal

Desired State is App-owner intent: the App Definition plus enabled Plugin
Releases, named Plugin Instances, Slot choices, configuration values or secret
references, and approved permission scopes. CLI and product UI own this
Interface; nobody edits its canonical projection by hand.

`propose` deterministically resolves one Desired State change into a Change
Proposal:

- `ready`: every binding, selection, grant, placement, and state transition is
  closed, and the delta is classified hot or structural;
- `needs_decision`: structured choices (ambiguous `one`, ordering, waivable
  soft conflict) or grant requests await the App owner; or
- `rejected`: a hard incompatibility has no valid decision.

A proposal explains itself in product vocabulary:

```text
Enable com.example.github@1.0.0

Adds
  + Tool github.search -> Agent Tools

Requests
  + HTTPS access to api.github.com

Apply
  hot — no restart; active work is unaffected

No Model, Prompt, Skill, or existing Tool is replaced.
```

## Reconciler and runtime change

The Reconciler resolves Desired State into the next Plan Snapshot, computes
the Plan Transition, and classifies it under the ADR 0067 whitelist:

- hot: `many`/`keyed_many` membership changes, Interface-identical provider
  replacement, configuration-only changes — applied atomically inside the
  running Kernel; and
- structural: everything else — applied as a staged App Generation swap under
  the [control-plane contract](dynamic-plugins.md).

Both paths are one user experience. `install`, `enable`, `update`,
`configure`, `disable`, and `rollback` never expose the mechanism; `inspect`
and diagnostics do. Failure before a Transition commit or a Generation switch
leaves the running snapshot fully authoritative.

Install is one action: acquire, verify and admit, confirm requested
permissions, resolve, apply. Install-disabled is a secondary flag. Every step
after confirmation is automatic when the proposal is `ready`.

## Diagnostics and inspection

Ordinary proposals and errors use Plugin, Slot, and product vocabulary.
`lenso inspect --resolved` exposes the generated detail with a stable
explainability chain:

```text
Plugin Release
  -> Plugin Instance
  -> Slot Entry -> Slot
  -> generated Module Instance or data mount
  -> Capability binding
  -> Placement and Execution Adapter
  -> Plan Snapshot digest -> applied Transition or Generation digest
```

Stable problem classes: authoring (invalid construct, unknown Slot, Schema
mismatch), admission (untrusted Release, digest mismatch), resolution (missing
owner, ambiguous `one`, conflict, unsupported target), permission (approval
required, denied, unenforceable), state (migration required, incompatible
overlap, unavailable rollback), and application (stale proposal, staging
failure, Transition or Ready failure — running snapshot preserved).

## Development experience

Hard rules:

- one authoring construct per language is the ordinary entrypoint regardless
  of product; product sugar lowers to it, never around it;
- no hand-written Descriptor, Schema file, factory, registration, Composition,
  or Plan in any persona's ordinary path;
- diagnostics speak Plugin and Slot vocabulary first, generated detail on
  demand; and
- development and production evaluate the same derived artifacts, resolution
  rules, permission closure, and readiness gates.

The supported loop:

```text
lenso new <name> --slot <product-slot>
lenso dev --host <product>
lenso test
lenso pack
lenso publish
```

`lenso dev` runs a real product Dev Host. Watch mode rebuilds the package,
resolves a candidate snapshot, and applies it as a hot Transition when the
delta qualifies — the common Tool-editing loop keeps the App warm — falling
back to a bounded development Generation swap otherwise, always preserving
last-known-good on failure. The Agent Harness Dev Host provides a fixture
Model, Turn and Tool invocation inspection, and a resolved authority view.

## Stateful replacement

State identity is Module-owned. Hot Transitions never touch durable state
ownership: a stateful Module replacement is hot only when it is
Interface-identical and its state schema is unchanged. Anything requiring
migration follows the control-plane contract: State Compatibility Receipts,
expand/contract overlap or maintenance replacement, and truthful rollback
availability. Plugin rollback selects an earlier Release into Desired State;
Generation rollback reactivates a retained Generation. Neither reverses
committed business data.

## Migration from the current implementation

Vertical, evidence-first:

1. **Derivation build (ADR 0066).** Implement `#[lenso::module]`,
   `#[lenso::capability]`, `defineModule`, and the product sugar; emit locked
   Descriptors and Schema snapshots; re-author the Agent Harness text-tools
   Module as the first proof — 140 lines plus four registration sites must
   become one source file with zero registrations.
2. **App Definition and resolver.** Replace `composition/fragments/*.json`
   with App Definitions; derive Composition and Plan Snapshot lockfiles;
   delete the code-level plugin profile catalog
   (`plugin_profiles.rs`-style) in favor of the Slot Catalog.
3. **Desired State and proposal.** Add Desired State, Change Proposal, and
   the `install`/`propose`/`apply` operations in front of the existing exact
   `resolve_generation` path, which remains the structural materialization
   seam.
4. **Reconciler and hot Transitions (ADR 0067).** Add snapshot sequence and
   Transition documents to `lenso-app-plan`; implement the Kernel Transition
   mechanism behind the deterministic test Driver with exhaustive conformance
   before any host enables it; wire the Reconciler's hot/structural split.
5. **Authoring loop.** Ship `lenso new`, `dev`, `test`, `pack`, `publish`,
   `inspect` on the same derived artifacts.
6. **Vertical proof.** From a precompiled Agent Harness: build and install an
   out-of-tree Tool package with a hot Transition and no restart; install an
   out-of-tree Agent Loop candidate and replace the built-in Loop through an
   owner decision and Generation swap; run real Turns across both changes;
   disable, update, and roll back — with no Cargo edits, no code-level catalog
   entry, and no hand-authored JSON anywhere.

Existing `resolve_generation`, Resolved Artifact Set, Effective Host Grant
Set, Generation Spec, Transition Spec, Ready Gate, lease, drain, and rollback
mechanisms remain the structural path's authorities. Current schema-version-1
control-plane documents are superseded by explicit version-2 documents
carrying snapshot sequence and Slot fields; no dual interpretation, silent
upgrade, or Kernel compatibility shim.

## Acceptance gates

The design is complete only with executable evidence that:

1. a stateless Tool reaches a real Agent Turn from one source file with no
   hand-authored identity, factory, registration, Schema, binding, or
   Composition artifact, in both TypeScript and Rust;
2. installing that Tool into a running App applies as a hot Plan Transition:
   no restart, active Turns unaffected, new Turns see the Tool;
3. a configuration-only change applies hot through instance replacement;
4. two compatible `many` Tool packages compose deterministically;
5. two valid `one` Model candidates produce a structured decision, never a
   silent winner;
6. a new permission request blocks application until explicitly approved;
7. a data-only Skill package executes no code and disappears cleanly;
8. two named Plugin Instances of one Release receive distinct Module and
   state identities;
9. a Host-plus-Web package activates atomically or leaves the running
   snapshot unchanged;
10. a structural change (Agent Loop replacement) stages a complete Generation,
    switches after readiness, and preserves the old Generation on failure;
11. stateful update truthfully chooses hot, overlap, maintenance, or
    rejection and never invents rollback safety;
12. `inspect --resolved` explains every generated Module, binding, placement,
    grant, snapshot, and Transition from its source Slot Entry;
13. ordered Turn interception proves transformation, short-circuit, cleanup,
    failure classification, and owner-controlled order;
14. a mounted package-owned Module subgraph activates atomically through one
    root Capability and cannot bind another package's private Instances;
15. regenerating any derived artifact reproduces committed bytes, and the
    additive-evolution lint blocks an incompatible Capability type change; and
16. the deterministic Driver conformance suite exhaustively exercises hot
    Transition commit against invocation, cancellation, restart, and shutdown
    interleavings.
