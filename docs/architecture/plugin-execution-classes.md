# Plugin execution classes

Status: proposed companion contract for
[ADR 0065](../adr/0065-govern-dynamic-plugins-above-the-kernel.md).

This document defines how one Plugin governance model selects different Module
execution mechanics. It does not make those mechanics uniformly implemented.
The [Dynamic Plugin control-plane contract](dynamic-plugins.md) owns identity,
admission, locks, grants, App Generations, routing, and the Process Protocol.
The [Module authoring, Slots, and dynamic resolution contract](plugin-authoring-and-resolution.md)
owns the ordinary author Interface and the Slot lowering that generates these
internal execution declarations.

## One lifecycle, orthogonal execution

`Plugin` is the installation, release, permission, and lifecycle identity.
`Module Instance` is the product behavior selected into the Resolved App Plan.
`Execution Class` selects the Adapter that makes that Instance executable.

One Plugin Release may carry several target variants for one logical Module
entry. Resolution chooses one variant for each keyed Instance and locks
it before staging. The variants expose the same configuration and Capability
Interface; they cannot change product meaning merely because their topology is
different.

```text
Plugin Release
  Module entry: example.search-provider
    implementation: native-builtin  -> exact product-linked factory
    implementation: process         -> admitted executable Artifact
    implementation: wasm-component  -> admitted Component Artifact
    implementation: quickjs         -> admitted bundled ESM Artifact
  Data entry: example.search-prompts
    Artifact + content Schema -> explicit Prompt Catalog Module input

Host Execution Policy + target + Host Build Manifest
  -> exactly one implementation per keyed Module Instance
  -> Resolved Artifact Set + Resolved App Plan
  -> configured Adapter catalog -> Kernel
```

Runtime does not negotiate, benchmark, or fall back. A failed selected variant
fails staging or its Module generation. Choosing another variant is a new
resolution and App Generation Transition.

## Support matrix

Support is reported per execution branch and interaction Profile, not inferred
from the fact that a Plugin can be installed.

| Entry or branch | Dynamic install | Process isolation | In-process | Initial interaction claim | Dynamic Plugin status |
| --- | --- | --- | --- | --- | --- |
| Native built-in, `lenso.native-rust@1` | No new machine code | No | Yes | Existing native Request, Stream, and Event conformance | Stable execution; Plugin governance proposed |
| Process, current `lenso.bun-process@1` | Yes | Yes | No | `provide-request-v1` only | Protocol and Adapter spike; product acceptance incomplete |
| Wasm Component, `lenso.wasm-component@1` | Yes | No | Yes | Preview: provide Request; stable: provide and consume Request | Preview Adapter implemented with a bounded runtime envelope; generated typed WIT integration and two-language proof remain open |
| Embedded JavaScript, `lenso.quickjs@1` | Yes | No | Yes | Preview: provide Request; stable: provide and consume Request | QuickJS-NG preview Adapter implemented; stable consume and product proof remain open |
| Native dynamic library, `lenso.native-dylib@1` | Yes | No | Yes | Provide Request only | Experimental trusted C-ABI Adapter implemented; fuzzing and platform review remain open |
| Data entry | Yes | Not applicable | Interpreted by an existing Module | No interaction Profile of its own | Designed; interpreter vertical proof deferred |

`Stream` and `Event` remain disabled for Process, Wasm, QuickJS, and dylib until
each branch passes the shared bidirectional conformance surface for admission,
ordering, backpressure, half-close or volatility, cancellation, late outcomes,
shutdown, restart, and Generation drain. Unsupported kinds fail before
readiness.

## Manifest variants and deterministic resolution

One generated `module_entries[]` entry references the immutable Module
Descriptor that owns the logical package ID, configuration Schema, provided
Capabilities, and required Capabilities. Its `implementations[]` entries own
only execution facts. This is canonical publication output, not the ordinary
Plugin author's Interface:

```json
{
  "id": "search-provider",
  "module_descriptor_digest": "sha256:...",
  "implementations": [
    {
      "id": "wasm-aarch64",
      "artifact": "search-wasm",
      "entrypoint": "search-provider",
      "execution_class": "lenso.wasm-component@1",
      "targets": ["aarch64-apple-darwin"],
      "profiles": ["provide-request-v1"],
      "support_channel": "preview"
    }
  ]
}
```

An implementation selects exactly one execution input: an admitted Artifact or
an exact built-in factory reference from the Host Build Manifest. Only
`lenso.native-rust@1` may use a built-in factory input; selecting it changes App
Composition but installs no machine code. Implementation IDs are Release-local
and immutable. `targets` use exact normalized host triples and declared host
features. A wildcard OS, architecture, ABI, engine, or Protocol Profile is
invalid.

Resolution applies this order:

1. Expand the selected Plugin Features.
2. Reject variants whose Artifact or built-in factory, target, support channel,
   trust level, required Profiles, or grants are not admitted by the Host Build
   and Host Execution Policy.
3. If the App-local lock pins an implementation ID, require that exact variant.
4. Otherwise use the first applicable policy preference rank containing one
   valid variant.
5. Reject zero matches or more than one valid variant at the selected rank.
6. Record the policy digest, variant, Artifact, target, Execution Class,
   Profiles, support channel, and reason in the Resolved Artifact Set.

Publishers may offer variants but cannot rank them. Products may prefer, for
example, stable Wasm over a stable Process implementation for constrained
third-party code, but that order is an explicit reviewable policy rather than
an implicit `fastest` rule.

## Data entries

Data is an entry kind, not an Execution Class. A Prompt, Skill, template,
rule set, locale, or static model table has no entrypoint, lifecycle callbacks,
Capabilities, or ambient authority merely because it arrived in a Plugin
Bundle.

A data entry declares one admitted Artifact, media type, exact content
Schema identity and digest, and Product Metadata reference. The App-local lock
mounts it into one named input slot of one explicitly selected interpreter
Module Instance. The product resolver validates that the interpreter supports
that schema and materializes the logical digest and slot into its reviewed
configuration. The Generation Supervisor supplies only a read-only
digest-verified Artifact handle through that interpreter's reviewed Adapter
seam; machine-local paths do not become authority.

The first proof must demonstrate that removing the Data Plugin removes only the
mounted data and leaves the interpreter Module and remaining Composition valid.
Executable files in a Data Artifact are inert bytes. A product that evaluates
them has selected an executable Module interpretation and must classify the
result under an execution branch instead.

## Wasm Component branch

The proposed execution class is `lenso.wasm-component@1`, hosted by Wasmtime's
Component Model support. Wasmtime is fixed by exact build identity in the Host
Build Manifest; the Artifact is an admitted WebAssembly Component, not a core
module accepted by heuristic adaptation.

### Capability bridge and WIT ownership

The Capability Descriptor IR remains the source of truth. `lenso-protocols`
owns deterministic WIT projection and drift checks beside Rust and TypeScript
projection. WIT is generated only for Descriptor shapes with one exact mapping;
an unsupported shape fails generation rather than falling back to generic
strings or untyped invocation.

One generated world exports the Instance's provided Capability operations and
imports only its explicitly bound required Capability operations. Host context,
terminal authority, and opaque Capability handles live in a versioned
`lenso:runtime@1` WIT package. WIT resources are generation-scoped,
non-forgeable handles; ownership and borrowing follow Component Model resource
semantics. Handles never cross App Generations or survive Module recreation.

WASI is absent by default. Filesystem, sockets, clocks, randomness, environment,
and subprocess functions exist only when an Effective Host Grant names an exact
enforcer and the selected world imports the corresponding reviewed host
Interface. Preopened ambient directories or inherited host environment are
forbidden.

### Limits and failure mapping

Each Module generation has one isolated Wasmtime Store with bounded linear and
table memory, instance count, host resources, async tasks, result bytes, and
compile cache input. Fuel provides a deterministic instruction budget where
required; epoch interruption supplies lower-overhead host deadline and
cancellation preemption. Memory remains bounded because a long bulk-memory
instruction is not required to observe every epoch tick.

The host terminal arbiter remains authoritative. A host cancellation or
deadline interruption maps to Kernel `Cancelled` or `DeadlineExceeded` once.
Guest-returned declared Domain Errors remain Domain Errors. A trap, invalid
canonical ABI value, fuel exhaustion not caused by the committed host terminal
outcome, memory-limit breach, forbidden import, or resource-table violation
retires the Module generation and exposes bounded Adapter diagnostics plus
`ModuleFailure`; uncertain guest state is not reused. No invocation is replayed.

Stable support requires real components from at least two guest languages,
provide-and-consume Request conformance, strict WIT drift detection, resource
cleanup, cancellation/deadline races, trap recovery, restart, and Generation
drain evidence.

## Embedded JavaScript branch

The selected engine is QuickJS-NG under execution class `lenso.quickjs@1`.
This choice optimizes for a small embeddable engine and a narrow host Interface,
not Node compatibility. The exact engine build and feature set are Host Build
inputs.

One Module generation receives one fresh runtime/context and an immutable,
digest-closed ES module graph. Admission accepts source or a product-defined
portable bundle format; engine bytecode is a target-specific cache Artifact,
never the only source of authority. Runtime npm resolution, package downloads,
CommonJS probing, `node_modules` traversal, native addons, dynamic native module
loading, `eval`-supplied composition, and ambient Node/Bun APIs are unsupported.
Pure JavaScript npm dependencies may be bundled before admission only when the
resulting closed Artifact and licenses are reviewed and locked.

Generated bindings expose only selected Capability operations and bounded
value conversion. Filesystem, network, environment, clocks, randomness, and
subprocess access require explicit host Interfaces and Effective Host Grants.
The Adapter bounds heap, stack, module bytes, pending Promise jobs, host handles,
result bytes, and per-turn execution. Its engine interrupt callback observes
host cancellation and deadlines; only the host terminal arbiter commits their
outcomes.

JavaScript exceptions that match a declared Domain Error are validated and
returned. An uncaught exception, interrupt unrelated to a committed host
terminal outcome, memory exhaustion, invalid value, rejected module load, or
job-queue invariant violation retires the Module generation as
`ModuleFailure`. QuickJS embedding reduces the exposed host surface but is not
presented as a complete hostile-code security boundary until the implementation
and native engine supply chain receive their own security review.

The first release has no npm-runtime compatibility promise. If a product later
requires Node semantics, that is a new Execution Class and threat model rather
than silently widening `lenso.quickjs@1`.

## Native dynamic-library branch

The proposed `lenso.native-dylib@1` class is experimental and accepts only
explicitly trusted, signed, target-exact Artifacts. It provides performance and
native ecosystem access, not confinement.

The ABI is a versioned C layout with one visible entry symbol,
`lenso_module_v1`. The returned root table begins with `abi_version` and
`struct_size`, followed by function pointers and reserved zero fields. Calls
exchange bounded canonical byte buffers and opaque integer handles rather than
Rust traits, C++ objects, or language-owned strings. The host supplies allocate,
reallocate, and free callbacks; memory allocated by one side is released only
through that side's callback. Every function returns a status code and writes
outputs through validated out-parameters.

Unwinding across the ABI is forbidden. Rust libraries catch panics inside the
library and return one fatal status; a panic or foreign exception that crosses
the ABI may abort the Host and is part of the explicit trust decision. The
Adapter validates table sizes, reserved fields, symbol visibility, Capability
tables, buffer ownership, limits, and all returned bytes before readiness.

Artifacts declare exact OS, architecture, ABI, minimum OS, code-signing
identity, and product policy compatibility. They load from unique
content-addressed paths so generations can coexist. A loaded library is never
unloaded from a live Host: destructors, thread-local state, callbacks, and
foreign runtimes make safe unloading unprovable. Disable prevents future
selection; update loads a side-by-side digest; reclaiming code mappings requires
a complete Host restart. UI labels that restart requirement before enablement.

Stable status is intentionally unavailable in V1. Experimental acceptance
requires request conformance, allocator and buffer fuzzing, panic/exception
fixtures, descendant-thread cleanup policy, code-signature rejection,
side-by-side generations, Host-restart reclamation, and a platform security
review for every supported target family.

## Product experience

Products present one Plugin lifecycle: `installed`, `enabled`, `disabled`,
`update available`, `staged`, `active`, `draining`, `rollback standby`, and
`removal blocked`. Each entry row also shows execution facts rather than
hiding them behind a generic Plugin badge:

- selected implementation and why the Host Execution Policy chose it;
- Execution Class, target, Artifact digest, and support channel;
- in-process or out-of-process topology and truthful isolation claim;
- requested, approved, and effective grants with named enforcers;
- interaction Profiles and unsupported behavior;
- whether enable, disable, update, or reclamation applies as a hot Plan
  Transition or requires an App Generation or complete Host restart; and
- measured boundary cost only from a named benchmark and comparison scope.

One enable action applies one complete validated change — a hot Plan
Transition or a staged candidate Generation. UI never offers per-entry partial
activation, runtime fallback, or a security claim based only on a signature. A
failed candidate preserves the running snapshot and shows the exact failed
stage.

Agent Harness V1 adopts Generations per Turn. One Turn and all nested Model,
Tool, Stream, and durable commit work retain one Generation Lease; the next
Turn may adopt the new Generation even in the same Session. Session-pinned
adoption is deferred because a long-lived Session can retain old executable
code and grants indefinitely. Adding it later requires an explicit maximum
lease, operator-visible retention, revocation behavior, Session fencing, and
garbage-collection proof.

## Delivery branches and gates

Each branch is a separate vertical milestone after the shared control plane:

1. **Process vertical:** finish the public SDK and Adapter, then the Agent Tool
   Plugin acceptance sequence in the control-plane contract.
2. **Data vertical:** admit one Prompt or Skill Artifact, bind it to one native
   catalog Module, switch Generations, prove removal, and execute no Plugin code.
3. **Wasm preview:** provide Request from two guest languages with strict WIT,
   resource, limit, trap, cancellation, restart, and drain evidence.
4. **Wasm stable:** add consume Request and a real constrained product flow;
   complete the confinement and supply-chain review.
5. **QuickJS preview:** run one bundled ESM provider with generated bindings,
   bounded jobs and memory, cancellation, restart, and no ambient host APIs.
6. **QuickJS stable:** add consume Request and a real product requirement;
   preserve the no-Node contract.
7. **Native dylib experimental:** complete the C ABI, fuzzing, signing,
   side-by-side, crash, and Host-restart gates. It does not graduate
   automatically with the other branches.

No milestone changes Kernel Plugin semantics. A branch adds one Execution
Adapter, host policy entry, Artifact kind, conformance surface, and truthful UI
support state. Repository creation remains subject to ADR 0064.

The implementation packages currently live with the extracted host runtimes in
`lenso-runtime-rust`: `lenso-plugin-control-plane`,
`lenso-wasm-component-adapter`, `lenso-quickjs-adapter`, and
`lenso-dylib-adapter`. `lenso-protocols` owns the deterministic WIT and generated
Rust runtime-codec projections. Package existence is not stable support: the
remaining gates above still control every support claim.

## Primary implementation references

- [WebAssembly Component Model WIT specification](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md)
- [Wasmtime Component Model runtime](https://docs.wasmtime.dev/)
- [Wasmtime interruption and resource limits](https://docs.wasmtime.dev/api/wasmtime/struct.Config.html)
- [QuickJS-NG](https://github.com/quickjs-ng/quickjs)
