# Service Capability Tiers

> **Legacy v0.3.x architecture:** This page describes the maintained
> Service-oriented implementation and is not normative for vNext. Read
> [lenso-vnext.md](lenso-vnext.md) for vNext decisions.

Lenso has two explicit Service capability tiers. The protocol names describe
different ownership boundaries; they are not interchangeable version upgrades.

## Provider

Provider Services use `lenso.service.v1`. They can be authored in Rust and TypeScript.
A Provider exports one or more Modules to a Host while the Host
retains authentication, queues, retries, Outbox claims, Runtime Story records,
and technical operation ownership.

The Rust Provider surface and `@lenso/service-kit` implement this tier. The
Provider protocol can expose declared business HTTP routes, runtime functions,
and Event handlers, but it does not make the Provider independently
authoritative for those runtime responsibilities.

## Autonomous Service

Autonomous Services use `lenso.service.v2`. This tier is Rust only. An
Autonomous Service owns its Workloads, Service-owned storage through its
Service Store, migrations, runtime queues, Inbox and Outbox, health, shutdown,
and local Story Segments.

Current Rust framework capabilities include:

- direct HTTP from versioned OpenAPI contracts;
- direct gRPC from versioned Protobuf contracts;
- transport-independent Event Contracts with Inbox and Outbox delivery;
- versioned Durable Workflows with persisted progress, retries, timeouts, and
  compensation;
- Workload Identity for Service Principals;
- bounded Delegated Actor Context and optional Tenant Context; and
- Service-owned storage, migrations, and operational state.

These Data Plane capabilities run without Console or System Plane availability.
Console may observe and request supported management operations through an
enrolled System Plane connection, but it does not proxy business traffic,
publish releases, or deploy the Service.

`@lenso/service-kit` does not provide Autonomous Service parity. TypeScript
projects that use the package remain on the Provider tier until a separately
reviewed `lenso.service.v2` runtime surface exists.
