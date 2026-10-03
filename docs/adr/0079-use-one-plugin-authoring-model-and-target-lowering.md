# ADR 0079: Use one Plugin authoring model and target lowering

- Status: accepted
- Date: 2026-10-03
- Related: ADRs 0030, 0070, 0071, 0073, 0075, 0076 and 0078

## Context

Source-first discovery, Plugin Root resolution, typed dependency bindings,
Workers Stream transport and the JavaScript Host already exist. The remaining
friction is authoring and execution packaging: a local author should not need
to construct internal Descriptors, codecs, factories or artifact identities to
compose a runnable App. Nor should a target implementation or distribution
source become another kind of application behavior.

## Decision

Plugin remains the sole application-behavior abstraction. Its Instances carry
independent configuration, bindings, resources, state and permissions. One
implementation may supply multiple Instances. Ordinary private language modules
remain ordinary modules inside a Plugin; they do not need separate Plugin
identities. Do not introduce Contribution, Managed Module, or another parallel
behavior hierarchy.

This decision has ten points: one Plugin behavior unit; progressive complexity;
implementation origins; independent Instances; common composition with target
validation/lowering; specialized target/storage support; first-class Rust/TS;
lightweight behavior authoring with generated internals; risk-based verification;
and independent product delivery with explicit non-goals.

The normal authoring surface is Plugin, Instance, Config, Dependency and Target.
A Capability is introduced when a stable, typed cross-Plugin product role is
needed. Existing authoring and generators derive the internal Descriptor,
codec, generation and artifact evidence. Routes, cron/event handlers and
Console integration are optional Plugin behavior surfaces using those seams;
they do not require a hand-written application Host or Descriptor.

Source, Dependency and Portable describe where an implementation comes from.
They do not change Plugin or Instance semantics. Local source and instance
configuration may coexist in `plugins/<id>/plugin.rs` or `plugin.ts` and adjacent
instance files. Normal Cargo and JavaScript package identities select source
dependencies. A declared, locked local JavaScript source dependency retains its
real source identity and fingerprint; it must not impersonate an older registry
artifact. Local iteration does not require first packing, freezing or installing
a Plugin. Immutable artifacts and bundles remain the distribution and portable
consumption boundary.

One App Composition and resolver select the logical graph. Each target then
validates support, authorizes and binds its actual resources, and lowers the
selected graph into its execution packaging. Resolve-once does not omit target
support or resource validation. Targets may select different providers through
explicit target policy and retain the resulting mapping evidence; they need not
produce identical executable bytes. Instance/config/descriptor/dependency
semantics remain the common authority. Target entrypoints, Driver wiring and
platform facility bindings belong to generated execution packaging, not
separate Native and Workers business Hosts.

A specialized Plugin implements only the targets and storage combinations it
needs. Unsupported combinations must fail admission; verify the exact selected
combination before expensive compilation wherever its evidence is available.
Do not invent a universal storage abstraction or require every Plugin to support
all targets. Extract shared behavior when a real reuse requirement exists.
Rust static linking still requires relinking after implementation edits and does
not imply dynamic code replacement.

Rust and TypeScript are first-class authoring paths. Each target pays for the
Adapters and facilities it actually selects. Cross-language calls use genuine,
coarse Capability boundaries; same-language direct implementation and existing
optimization opportunities remain available. Kernel, Driver, Adapter and
Generation retirement/cancellation/cleanup responsibilities are unchanged.

Verification follows risk: frontend checks and relevant interactions; affected
business tests and real requests; contract compatibility in affected languages;
target runtime conformance; and real storage, concurrency, idempotence and
recovery for Auth, migrations or monetary behavior. Run a final exact candidate
gate without repeating the entire matrix after every local edit. Record local
feedback measurements as local measurements, not extrapolated CI savings.

## Current implementation and evidence boundary

`examples/onboarding/source-first-instances` exercises independent Rust
Instances and exact source linkage in Native and Workers. The mixed example
adds two TypeScript Instances and a typed dependency through the same resolved
Kernel graph; Workers performs explicit Bun-to-JS execution lowering. The
current Core JS Workers Adapter admits Request only and rejects Stream/Event,
even though the separately qualified SDK can package Stream. This is an
implementation boundary, not a second authoring model.

`examples/console-auth-source` composes official Console, original PostgreSQL
Auth, an authored protected HTTP Plugin, fixture Secrets and built-in Ingress
through the ordinary CLI. It is a Native/local-storage qualification and does
not qualify Workers, browser rendering or deployment. Source revisions, exact
commands and results accompany the task's evidence; these examples do not
publish dependencies or authorize production resources.

Configuration providers, precise support-combination checks, Console instance
mounting, and CLI/MCP/skill conveniences use their existing owner seams. They
must not add a competing resolver or require ordinary authors to build internal
packaging evidence. Relay production delivery is independent of this work.

## Consequences

Authors can start with a small Plugin and add configuration, dependencies,
lifecycle, target facilities and public Capabilities as their actual behavior
requires. Existing custom Hosts, portable artifact consumers and lifecycle
contracts retain their authority. New source conveniences are additive; no
published version is claimed to contain unpublished authoring APIs.

Three-D1 consolidation, universal portable conversion, libbun embedding and
Marketplace changes are outside this decision's implementation scope.

The current per-section [coverage ledger](../qualification/source-assembly-20261003.md) records remaining work separately from accepted constraints.
