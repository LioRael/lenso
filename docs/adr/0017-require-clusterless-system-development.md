# Require clusterless System development

Lenso will make multi-Service development and distributed-behavior verification possible in a local System Sandbox without Kubernetes or an external message broker. The sandbox supplies local Transport, Endpoint Resolver, Workload Identity, isolated Service Stores, controlled time, Failure Scenarios, contract checks, and Story replay, while separate Environment Verification proves the same behavior against real production-class infrastructure.

## Consequences

- `lenso system dev` becomes the local multi-Workload entrypoint.
- Timeouts, retries, duplicate or reordered events, slow dependencies, and partial failure are repeatable tests rather than manual accidents.
- Linked and extracted implementations can be compared with the same business inputs and Story evidence.
- Local success never claims production equivalence; real transport, network, identity, store, and deployment checks remain explicit release evidence.
- Kubernetes and external brokers are supported integrations, not prerequisites for ordinary development.
