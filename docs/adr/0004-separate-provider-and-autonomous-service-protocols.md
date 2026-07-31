# Separate Provider and Autonomous Service protocols

Host-to-Provider interactions use the Host-owned Provider Protocol, while Autonomous Services communicate directly through Service-owned HTTP, gRPC, and event contracts. Lenso generates and verifies contracts and clients and supplies context propagation, resilience, idempotency, standard errors, and operational evidence, but it does not make Autonomous Services impersonate Providers or proxy their business traffic through a Host.

## Consequences

- The Provider Protocol is a Host transport contract and never a Module source.
- Autonomous Services own and version their Service Contracts.
- Service-to-Service calls remain direct Data Plane traffic.
- Lenso tooling may present both interaction types in one Runtime Console while preserving their different ownership semantics.
