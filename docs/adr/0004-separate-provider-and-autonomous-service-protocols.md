# Separate Provider and Autonomous Service protocols

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Host-to-Provider interactions use the Host-owned Provider Protocol, while Autonomous Services communicate directly through Service-owned HTTP, gRPC, and event contracts. Lenso generates and verifies contracts and clients and supplies context propagation, resilience, idempotency, standard errors, and operational evidence, but it does not make Autonomous Services impersonate Providers or proxy their business traffic through a Host.

## Consequences

- The Provider Protocol is a Host transport contract and never a Module source.
- Autonomous Services own and version their Service Contracts.
- Service-to-Service calls remain direct Data Plane traffic.
- Lenso tooling may present both interaction types in one Runtime Console while preserving their different ownership semantics.
