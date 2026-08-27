# Distributed Plugin Runtime

This document records a possible direction, not a committed architecture or current product contract.

## Motivation

An App should be able to evolve from one local runtime into a distributed system without rewriting its Plugin Interfaces. This may eventually let Lenso support microservice deployments, AI agent clusters, multiplayer game servers, and application-specific communication protocols through Plugins and Adapters rather than Kernel features.

## Possible Scenarios

- Run a Plugin outside the local runtime process.
- Bind one Capability to one or more remote Plugin Instances.
- Discover, supervise, and replace remote providers.
- Select communication Adapters such as HTTP, gRPC, WebSocket, QUIC, TCP, UDP, or event brokers.
- Integrate with external runtimes and orchestrators such as Docker, Kubernetes, or Nomad.
- Add optional placement, replica, rollout, and cluster-coordination Plugins when real applications require them.

## Constraints Preserved Now

- Consumers depend on Capability handles rather than native implementation pointers.
- Plugin identity is independent of process and network identity.
- Capability Interfaces make cancellation, deadlines, availability, delivery, ordering, and idempotency semantics explicit when relevant.
- The Kernel owns no durable application state.
- Transport, discovery, deployment, and telemetry remain replaceable implementations outside the Kernel.

These constraints preserve an extension seam. They do not justify speculative Control Plane, scheduler, discovery, or placement Interfaces before multiple real implementations exist.

## Deferred Work

- Remote Plugin execution
- Dynamic App Composition
- Service discovery
- Placement and replica management
- Leader election and distributed leases
- Durable Plugin catalogs
- Rolling upgrades
- Global desired-state and observed-state reconciliation
- A Lenso-owned Control Plane

Lenso may never need to own all of these responsibilities. External infrastructure can satisfy some or all of them through future Adapters.

## Revisit Triggers

Revisit this direction only when a real application requires at least one of the following:

- more than one runtime process;
- runtime discovery rather than static configuration;
- multiple interchangeable Plugin Instances;
- zero-downtime replacement;
- runtime placement changes; or
- coordination that an external runtime cannot adequately provide.
