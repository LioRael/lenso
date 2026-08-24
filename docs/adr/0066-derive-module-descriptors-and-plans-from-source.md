# ADR 0066: Derive Module Descriptors, contracts, and Plans from source

- Status: proposed
- Date: 2026-08-25
- Extends: ADR 0031, ADR 0034, ADR 0045, ADR 0050, ADR 0055, ADR 0056,
  ADR 0057
- Contracts:
  [`../architecture/plugin-authoring-and-resolution.md`](../architecture/plugin-authoring-and-resolution.md)

## Context

The Module runtime contract is deep, but its authoring Interface is shallow:
authors restate facts the compiler and type system already know. Implementing
a ten-line stateless Agent Tool today requires hand-written package identity
constants, a `NativeModuleFactory` implementation with manual entrypoint and
configuration validation, a JSON Schema embedded as a Rust string literal,
`LocalBoxFuture` and nested `Result<Result<T, E>, RuntimeFailure>` plumbing,
plus edits in up to four other places: a factory registration macro in the
host, a code-level plugin profile catalog, a Capability crate with
hand-written `capability.json` and schema files, and an App Composition JSON
fragment repeating `capability_id`, `descriptor_version`, cardinality,
admission limits, and `execution_class` values.

Every one of those declarations duplicates information that exists in Module
source or in the Capability contract. Hand-maintained duplication is both the
developer-experience complaint and a standing drift risk that current codegen
direction (JSON to code) only partially checks.

## Decision

**Module source is the single source of truth. Descriptors, Schemas,
factories, registrations, manifests, Compositions, and Plans are generated,
locked artifacts — like lockfiles, they are committed, diffed, verified, and
never hand-edited.**

### One authoring entrypoint per language

Each supported language provides one Module authoring construct — a Rust
attribute macro (`#[lenso::module]` with product sugar such as
`#[agent::tool]`) and a TypeScript function (`defineModule` with product sugar
such as `agent.tool`). From that construct the build derives:

- package and Module identity and version from the package manifest;
- the configuration Schema from the declared configuration type;
- required Capabilities and cardinality from typed Port fields;
- provided Capabilities and Operations from the implemented contract traits;
- the optional durable state identity and Schema from the state declaration;
  and
- the Execution Adapter factory, endpoint glue, and host registration.

Authors write plain async functions returning domain `Result`s; generated glue
owns boxing, dispatch, and `RuntimeFailure`. No hand-written factory,
identity constant, registration call, or code-level catalog entry remains an
authoring surface.

### Capability contracts as types

A Capability contract is authored as an annotated trait or interface in its
owning language. The build emits the canonical JSON Schema and Descriptor as a
locked snapshot committed beside the source. Cross-language bindings generate
from the locked snapshot, so the existing portable value profile, additive
minor-evolution lint, and drift gates all remain — the generation direction
reverses, the verification surface does not. CI fails when regenerated
snapshots differ from committed ones, exactly as generated bindings fail
today.

### Composition intent, derived Composition

The App Definition is the only hand-authored composition input: which Modules
an App uses, Slot choices, configuration values, and Execution Lane
assignments. The resolver closes bindings from generated Module Descriptors —
Ports and provided Capabilities make most bindings unambiguous; real
ambiguity becomes a structured decision, never silent selection. The derived
App Composition and the Resolved App Plan are generated lockfile-style
outputs: inspectable, diffable, signable, and authoritative for execution,
but not an authoring surface for any persona.

### What this does not change

The Kernel runtime contract — Descriptor semantics, explicit bindings,
lifecycle, supervision, admission, lanes — is unchanged; this decision changes
who writes its inputs. Generated artifacts remain the exact verification and
admission authorities that ADR 0065 and the control-plane contract require.
Authoring-time derivation grants no trust: an installed third-party Rust
Module still requires an admitted Wasm or Process variant, and derivation
never selects an Execution Class or broadens a grant.

## Consequences

- The ordinary stateless-extension path shrinks to one source file with no
  Lenso JSON, identity constants, factory, registration, or Composition
  knowledge; the deep-Module path uses the same construct with Ports and
  contract traits.
- Capability crates keep their generated code but lose hand-written
  `capability.json` and schema files; hand-written App Composition fragments
  and code-level plugin profile catalogs are deleted rather than migrated.
- Proc-macro and codegen become load-bearing developer infrastructure:
  expansion must stay inspectable (`cargo expand`-level golden tests),
  compile-time cost must be watched, and generated output must stay
  deterministic across toolchains.
- Schema evolution discipline moves into type review: the additive lint and
  locked-snapshot diff are the guardrails that make "types as source"
  cross-language safe.
- Migration is mechanical but broad: every existing Module and Capability
  crate in dependent repositories re-authors onto the derivation macros, with
  the Agent Harness text-tools Module as the first vertical proof.
