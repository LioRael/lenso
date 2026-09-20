# Environment and infrastructure composition

Status: normative companion to [ADR 0076](../adr/0076-separate-execution-environments-from-infrastructure-implementations.md).

This document makes the Host and Plugin boundary operational. It does not add a
new resolver, a global Platform abstraction, or a replacement for App
Composition.

## The two questions

Every target-specific design must answer two independent questions:

1. In which **Execution Environment** will the Host run this Plugin Instance?
2. Which concrete **Infrastructure Implementations** may the Plugin use through
   Host-authorized resources?

The Environment question admits Drivers, Execution Adapters, lifecycle
mechanics, resource injection, and target restrictions. It has three current
identities: Native, Cloudflare Workers, and Simulated.

The Infrastructure question describes a concrete backing implementation. It may
be direct PostgreSQL, D1, Hyperdrive transport to PostgreSQL, a deterministic
simulator store, or another resource. It does not name the Plugin's business
behavior.

    Host Environment Profile
        + Host-authorized resources
                    |
                    v
       existing Host + Plugin Root resolver
                    |
                    v
           App Composition and Plan
                    |
                    v
    Driver / Execution Adapter admits execution
                    |
                    v
      Plugin private adapter and Capabilities

The Host profile and resources are assembly inputs. App Composition remains
resolver output, and the Plan remains the Kernel's immutable execution input.

## Boundary decision table

| Question | Put it in a Capability when | Keep it private infrastructure when |
| --- | --- | --- |
| Who may use it? | Several Plugins need a semantic product role. | One Plugin needs a backing implementation. |
| What is stable? | Operations, types, Domain Errors, lifecycle, and compatibility policy. | Adapter mechanics, SQL client, binding, pool, endpoint, or provider SDK. |
| How is it connected? | Explicit typed requirement and resolver binding in the Plan. | Host authorization and Plugin-owned adapter construction. |
| What does change mean? | Contract evolution and Capability conformance. | Target-specific implementation replacement plus combination qualification. |

A shared pool, transport, or secret is not automatically a Capability. If
another Plugin must rely on a business meaning such as account state,
authorization decision, or durable outbox delivery, that meaning needs a
Capability. The physical infrastructure underneath can remain private to its
provider Plugin.

## Legal compositions

| Exact combination | Private implementation boundary | What it does not imply |
| --- | --- | --- |
| Native plus direct PostgreSQL | A Plugin owns its PostgreSQL adapter and schema/migration semantics; the Host authorizes the connection resource. | Workers, D1, or another Plugin's database access. |
| Cloudflare Workers plus D1 | A Plugin owns its D1 adapter and data semantics; the Host authorizes the binding. | PostgreSQL behavior, deployment quality, or production capacity. |
| Cloudflare Workers plus Hyperdrive plus PostgreSQL | A Plugin owns its PostgreSQL adapter; Hyperdrive is a transport selected by the Host context. | D1 behavior or a generic database abstraction. |
| Simulated plus deterministic store | A simulator provides a deterministic backing implementation for a Plugin-owned adapter seam. | Native, deployed Workers, or production qualification. |

The same Capability contract can have qualified combinations in several rows,
but each row needs its own evidence. A local workerd matrix is useful evidence
for Cloudflare Workers with local workerd mechanics; it is not evidence for a
deployed Worker, a different store, or production traffic.

## Host and Plugin responsibilities

The Host owner:

- selects an Environment Profile;
- authorizes concrete resources and their security boundaries;
- supplies the Driver and Execution Adapters that can admit the Plan;
- rejects unavailable or disallowed target combinations before readiness; and
- records target-level operational limits outside the portable Kernel.

The Plugin owner:

- declares semantic dependencies as Capabilities;
- owns its state, migration, persistence, and target-specific private adapter
  behavior;
- fails clearly when the Host does not authorize a required concrete resource;
  and
- contributes exact source and qualification evidence for each claimed
  combination.

The Kernel does neither. It does not discover a resource, choose an adapter,
contact a network, or infer a business dependency.

## Evidence and status

Use the [qualification ledger](../qualification/README.md) for current
evidence. Each qualification contains this tuple:

    subject + source revision + environment + infrastructure + evidence
    + known limitations

Design, implementation, release, and qualification are separate fields. A
release version is a distribution fact, not proof that every Environment or
Infrastructure Implementation was exercised. An absent record means the
combination has not been assessed; it does not mean the combination is
forbidden.

The [execution target capability matrix](execution-target-capability-matrix.md)
is the separate source for Driver and Adapter admission facts. It cannot turn
an admitted interaction into a qualification or authorize a concrete backing
resource.
