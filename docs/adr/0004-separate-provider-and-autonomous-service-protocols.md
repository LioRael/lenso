# Separate Provider and Autonomous Service protocols

Host-to-Provider interactions will retain the Host-owned Remote Module Protocol, while Autonomous Services communicate directly through Service-owned HTTP, gRPC, and event contracts. Lenso will generate and verify contracts and clients and supply context propagation, resilience, idempotency, standard errors, and operational evidence, but it will not make Autonomous Services impersonate Providers or proxy their business traffic through a Host.

## Consequences

- The Remote Module Protocol remains a valid compatibility and gradual-extraction path.
- Autonomous Services own and version their Service Contracts.
- Service-to-Service calls remain direct Data Plane traffic.
- Lenso tooling may present both interaction types in one Runtime Console while preserving their different ownership semantics.
