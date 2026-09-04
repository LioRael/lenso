# From product SDK declarations to an executable Plugin

Status: **Design approved on 2026-09-04; implementation and exact formats pending.**
Date: 2026-09-04.

Read the [authoring review](2026-09-04-plugin-usage-walkthrough.md) and
[Rust/TS examples](2026-09-04-multilingual-plugin-authoring.md) first. This
companion defines how product helpers enter generic authoring without putting
Agent or Web semantics into the generic SDK, resolver, or Kernel.

## Decision

Product SDKs lower their conveniences through the existing Capability machinery.
One authoring build derives both standard declarations and executable binding
code from the same source. Installation reads generated artifacts and never
imports source modules or launches a Plugin to discover its declarations.

The worked path is a TS document-sync Plugin with named `source` and
`destination` Store requirements, a private asynchronous transform engine, and
an Agent-owned `tools(...)` declaration. Both Store instances may be Rust
implementations. An Agent consumer reaches it through a Host-resolved
ToolProvider Capability, not an implicit global tool registry.

```text
Plugin source + locked contract/SDK dependencies
                |
       language authoring build
       + product SDK lowering
                |
       standard declarations + generated binding code
                |
       compile, check, and package exact implementation
                |
       verify bundle and resolve Host + Plugin Root
                |
       prepare -> construct -> bind -> App ready
                |
       Agent -> ToolProvider -> sync -> bound Store clients
```

## 1. Separate product facts from Capability facts

`tools([...])` is an Agent SDK helper producing one ToolProvider declaration.
Its tool list is product-owned data used to implement `catalog` and `execute`.
`sync_document` is a tool name inside that contract, not a new Capability
operation or an entry in a Kernel tool table. Tool parameter schemas, output
formatting, execution classification, and error translation belong to Agent.

The generic authoring layer accepts the standard Capability identity, exact
Descriptor projection, supported interaction kinds, and an instance binding
recipe. A direct implementation and a product-helper-generated implementation
use that same entrypoint. Config, named requirements, implementation selection,
and lifecycle stay with their existing owners. Grouping tools does not create
another Plugin instance, and a product helper cannot add hidden dependencies,
broaden grants, choose accounts, or register another lifecycle authority.

Tool catalog data can be compiled into the generated Agent binding or carried
as its verified package resource. It does not require an Agent-specific field
in the generic Plugin Descriptor. Tool validation belongs to the product build
integration; generic packaging validates the standard projections and artifact
integrity. Neither needs a second handwritten manifest.

## 2. A bounded build interface, not arbitrary source evaluation

The language authoring frontend recognizes the generic definition shape and
resolves imported declarations through exact package dependencies. Product SDKs
that add authoring syntax supply a versioned build entrypoint through normal
package metadata. The frontend resolves it from the imported export and pinned
package identity, not a global registry or the spelling `tools`.

That entrypoint has a narrow job:

- Receive declaration values, generated contract references, source locations,
  and opaque handler references from the language frontend.
- Return standard Capability declarations, any generated product data, and
  binding code linked to those handler references.
- Report unsupported declarations and contract errors against the author's
  source. It does not resolve App instances, launch Plugins, or install packages.

Handler references are build references to source code, not serialized closures
or persistent runtime IDs. Normal typechecking/linking resolves them in the
executable. The generic frontend gathers typed config and named requirements;
product lowering cannot silently append undeclared requirements.

The first TS declaration subset supports literal structures, supported schema
builders, statically resolvable constants/contract references, and helper calls
with a declared lowering entrypoint. Declaration values may be composed within
that subset. Business handler and constructor bodies remain ordinary TS and
are compiled without being invoked or evaluated for metadata discovery.
Do not attempt to infer whether arbitrary JS is pure. Computed imports,
network/database reads, environment-dependent tool lists, and unsupported
function calls in declarations fail with source diagnostics. Do not fall back
to importing `plugin.ts` to find out what it exports.

Build entrypoints are executable build dependencies, comparable in trust to
compiler plugins and Rust procedural macros. This is not a build sandbox or a
guarantee that a malicious SDK cannot perform I/O. The supported build interface
receives explicit inputs, must generate deterministic results, and is never
loaded from an installed bundle during inspection or admission. Source builds
use the existing trusted build workflow; inspecting a prebuilt Plugin does not
implicitly opt into a source build.

The exact package export name, structural build types, and supported expression
grammar belong in the implementation specification. This design chooses a
normal SDK build dependency, not a separately installed build-plugin marketplace,
general compiler-hook framework, or a promise to understand arbitrary TS.
An SDK needing no syntax sugar may simply export standard generated bindings
and needs no custom build entrypoint.

## 3. Derive both outputs together

One build produces standard Plugin/Capability metadata and the binding code
which implements it. The Agent lowering derives its catalog and dispatch from
one tool declaration, so an author never separately edits a name list and a
dispatch table. The binding recipe is stateless until attached to an instance;
it must not capture a live engine, account connection, or mutable singleton.

Validate duplicate tool names in Agent lowering and duplicate Capability
declarations/requirement IDs in generic assembly. Check configuration schema,
contract versions, handler types, supported interaction kinds, and referenced
generated artifacts before publishing the bundle. Keep ADR 0066's generated,
locked snapshots and drift checks; generated files are reviewable outputs,
not a second authoring authority.

Compile the Plugin and generated binding together. Package exact artifacts and
their standard declarations under the existing verified bundle mechanism;
never combine a fresh declaration with an old executable from another build.
Reuse the existing integrity checks and allocate profile/format changes only
against the implementation baseline. Hash agreement proves which bytes were
admitted, not that arbitrary implementation code obeys the contract. Product
conformance and runtime boundary validation remain necessary.

## 4. Install and resolve without executing the Plugin

The installer verifies the bundle, contract/profile compatibility, and Host
admission rules. No TS source import, helper build execution, constructor, or
Plugin process launch is needed to inspect or install the prebuilt bundle.
An unsupported profile is rejected or excluded by explicit Host selection
before activation; there is no runtime implementation fallback.

Resolution independently selects exact providers for `source` and `destination`
under accepted ADR 0073 on explicit adoption. The resulting Plan contains ordinary
Capability bindings and the selected execution implementations. The resolver
does not care that a TS tool calls a Rust store. Generated Store clients retain
the requirement identity through the actual supported Adapter path.

An Agent consumer must also be selected and bound to ToolProvider. Neither
installation nor the tool catalog grants invocation authority. Installing this
Plugin into a Host without that consumer does not create an Agent automatically.
If the Host's required Slot/contract policy cannot admit it, report that policy
failure rather than silently inventing a consumer or attachment.

## 5. Bind one constructed instance, then open admission

During preparation, validate standard contract and executable endpoint/profile
agreement using the actual Adapter path. A transport handshake, when required,
is part of startup execution, not offline metadata discovery. Normal runtime
module loading may execute library top-level code; this design does not turn
that code into lifecycle-managed or sandboxed work.

After required Store providers are callable, inject the resolved config and
named clients and invoke the TS factory. It loads its private engine and returns
one complete instance. The generated binder attaches the ordinary ToolProvider
implementation to that instance, without invoking business handlers. The same
object is passed to each tool call; generated binding does not clone state or
invoke the constructor for each registration.

Construction success is admitted only while that startup attempt remains open.
A late return after cancellation transfers directly to cleanup, not the binder.
The [cancellation review](2026-09-04-plugin-cancellation-and-cleanup.md) defines
late-completion ownership and safe cleanup under the remaining deadline.

Activate the provider with its locked declaration. Activating downstream
consumers may call it in dependency order under ADR 0046, while external work
remains behind the App Ready Gate. On failure, follow existing bounded startup
cleanup; successful construction transfers lifecycle cleanup responsibility as
specified by the authoring review. A recreated instance receives a fresh binding
to its fresh object while retaining the same resolved logical dependencies.

The call path is an ordinary ToolProvider `execute` request. Agent-generated
dispatch validates the tool input and invokes the selected method, which uses
the bound Store clients. Deadline, cancellation, domain/runtime errors, and
supported wire validation cross the TS/Rust boundary through existing contracts.
Neither the generic SDK nor Kernel knows the meaning of `sync_document`.

## Current evidence and remaining implementation work

The inspected Rust Agent SDK already generates ordinary `lenso::provides`
implementations; this ownership is accepted in Agent Harness ADR 0024. In the
inspected CLI, `describe_bun_plugin` runs `src/lenso.describe.generated.ts`, and
the scaffold's describe script imports `plugin.ts`. Therefore non-evaluating TS
declaration extraction is an actual build-path change, not a current guarantee.
The existing Bun plugin definition also rejects Stream/Event declarations;
this Request walkthrough does not establish support for those interaction kinds.

Language SDK owners own extraction and instance binding; Agent owns tool
lowering; protocol tooling owns standard contracts and projections; CLI/runtime
owners own packaging and executable profiles; Host owns selection and admission.
Core owns portable Plan validation and execution and gains no product or
language dependencies.

Review cases before implementation specifications:

| Case | Required outcome |
| --- | --- |
| `tools` is imported under an alias | Resolve the SDK export identity; no name-based Agent special case. |
| Tool list depends on a live account query | Declaration error; move dynamic account behavior into the supported runtime contract. |
| Declaration generation encounters constructor side effects | The constructor is not invoked; required metadata must be extractable separately. |
| SDK emits a hidden dependency or conflicting declaration | Reject the output against the declared input contract. |
| Catalog and dispatch are stale relative to source | Generation drift check fails; do not publish mixed outputs. |
| Read-only inspection of a prebuilt bundle | Verify data/artifacts without loading SDK build entries or Plugin code. |
| Two identical Store types bind to different Rust instances | Preserve each requirement identity through both generated clients. |
| Runtime endpoint/profile disagrees with admitted metadata | Fail startup before business dispatch; no automatic adaptation or fallback. |

These cases are design acceptance statements. No source extraction, compilation,
mixed-language runtime proof, or new SDK release is claimed here.
