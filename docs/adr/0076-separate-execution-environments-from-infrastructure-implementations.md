---
status: accepted
---

# Separate execution environments from infrastructure implementations

This decision extends ADRs 0034, 0041, 0053, 0070, and 0071. It clarifies
composition and evidence boundaries; it does not replace their ownership or
resolution rules.

## Context

Lenso Plugins can run through different host mechanics while using different
backing resources. Treating those choices as one axis makes a successful
combination look more general than its evidence permits: Cloudflare Workers
does not imply D1, native execution does not imply PostgreSQL, and a local
workerd test does not establish a deployed target or production result.

The existing model already separates the portable Kernel, Host-owned Runtime
Drivers and Execution Adapters, Plugin Capabilities, and Plugin-owned stateful
behavior. It needs one precise way to describe the host context in which a
Plugin runs and the concrete infrastructure that backs a Plugin's private
implementation without creating a second resolver or a global dependency
lookup service.

## Decision

### Keep two independent axes

An **Execution Environment** is a Host-selected runtime context. The current
environment identities are Native, Cloudflare Workers, and Simulated. A Host
Environment Profile selects or admits the Runtime Driver, Execution Adapters,
lifecycle mechanics, resource-injection mechanics, and target restrictions
that can execute an already resolved Plugin Instance.

An **Infrastructure Implementation** is a concrete backing implementation
available to a Plugin through a Host-authorized resource. It can be a direct
PostgreSQL connection, D1 binding, Hyperdrive transport to PostgreSQL,
deterministic simulated store, queue client, secret provider, or another
target-specific facility. Availability of a resource is Host policy. The
Plugin owns the persistence semantics and its private adapter choice within
that authorization.

The two axes compose but neither implies the other:

| Environment | Infrastructure example | Meaning |
| --- | --- | --- |
| Native | direct PostgreSQL driver | A native Adapter runs a Plugin whose private persistence adapter uses a Host-authorized PostgreSQL resource. |
| Cloudflare Workers | D1 | A Workers Adapter runs a Plugin using a Host-authorized D1 binding. |
| Cloudflare Workers | Hyperdrive plus PostgreSQL | A Workers Adapter runs a Plugin using Hyperdrive as a PostgreSQL transport. Hyperdrive is not a persistence type. |
| Simulated | deterministic store | A simulator runs the same observable Plugin behavior with a Host-authorized deterministic backing implementation. |

An Environment Profile must not silently choose a Plugin's storage semantics.
Likewise, exposing a PostgreSQL resource must not imply a Host-wide database
service or bind another Plugin to that adapter.

### Preserve the existing composition path

The composition path remains:

    Host Catalog + Plugin Root
                |
             resolver
                |
        App Composition
                |
        Resolved App Plan
                |
    Driver and Execution Adapter admission

The Host Environment Profile and Host-authorized Infrastructure Selection are
Host assembly inputs around that path. They constrain admission and resource
injection before an Adapter executes the resulting Plan. They do not introduce
a HostComposition type, change App Composition into authoring input, create a
second resolver, or add a new Plan schema field in this decision.

Kernel continues to receive only its immutable Plan and portable handles.
Drivers and Execution Adapters remain outside Kernel ownership under ADR 0053
and ADR 0064.

### Keep Capabilities semantic and private infrastructure private

An implementation becomes a public Capability only when it is a versioned
cross-Plugin product role with stable types, operations, errors, lifecycle
meaning, and explicit Plan-visible bindings. A shared physical connection pool,
database client, secret binding, or transport does not by itself meet that
test.

A Plugin may own a private target-specific persistence adapter when no other
Plugin consumes it as a product role. The Host may authorize and inject the
underlying resource, but does not offer it through a global locator. If
multiple Plugins need the same semantic behavior, they declare and bind a
Capability through the existing resolver authority in ADR 0034.

### Record qualification per exact composition

Qualification evidence is recorded in
[the canonical qualification ledger](../qualification/README.md). A
qualification names the exact Execution Environment, Infrastructure
Implementation set, source revision, evidence, and known limitations. Design,
implementation, release, and qualification are independent evidence facets:
a released artifact does not elevate a local result to target or production
qualification.

No document, renderer, or release note may emit a bare claim that a Capability
is supported. It must either link to the matching qualified combination or
state that the combination is not assessed.

The [execution target capability matrix](../architecture/execution-target-capability-matrix.md)
separately records whether an exact Driver and Adapter target can admit an
interaction. Its admission cells do not select Infrastructure or establish
qualification.

## Consequences

- A test Host can model production-relevant behavior without claiming a
  production infrastructure result.
- A Workers plus D1 result says nothing about Workers plus Hyperdrive plus
  PostgreSQL until that combination has its own evidence.
- Plugin authors keep persistence semantics close to their behavior; Host
  owners keep resource authorization and execution mechanics at the Host
  boundary.
- Cross-Plugin business behavior remains typed Capability work rather than
  accidental sharing through a platform service locator.
- Existing Plans, App Composition, Runtime Driver, and Execution Adapter
  schemas remain unchanged by this decision. Owner repositories may make
  compatible target-specific changes through their normal adoption paths.

## Rejected alternatives

**One environment enum that implies storage.** This collapses execution and
infrastructure facts, makes evidence non-transferable, and hides portability
limits.

**A global Platform service locator or a second dependency resolver.** This
would bypass explicit Capability bindings and duplicate the App resolver's
authority.

**A universal database Capability for every pool or binding.** A transport or
physical client is not necessarily a product role. Such a Capability is added
only when its semantic contract is genuinely shared across Plugins.
