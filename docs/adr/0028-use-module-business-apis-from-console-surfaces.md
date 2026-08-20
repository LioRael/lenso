# Use Module business APIs from Console Surfaces

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Console Surfaces will manage business data through generated clients for the exact Module-owned OpenAPI or HTTP Contract. Console Service may provide a same-origin Console Surface Gateway that routes only operations admitted by the Surface artifact's digest-bound grant, but it does not define a parallel management API and does not carry business requests through the System Plane. The target Module remains authoritative for business authorization and behavior.

## Consequences

- Generic Admin Data entities, generic Admin Actions, and generic Console `query` or `command` operations are removed from the new Console Module Contract major without a compatibility runtime.
- A Surface API Grant selects exact operation identifiers from an existing Module business API Contract rather than declaring another data model.
- Operator-only behavior remains in the Module business API Contract and is limited through operation audience and capability rather than a Console-owned Admin API.
- Generated clients preserve the Contract Version, delegated actor, tenant, deadline, idempotency, and Story context; Surface code does not construct Service URLs or credentials.
- Gateway requests identify the exact Contract digest and operation identifier with typed input; the Gateway resolves the admitted method and path and rejects raw targets or headers.
- Authorization is the intersection of the Surface API Grant, current Console Actor authority, and target Module business authorization.
- Linked Modules and Autonomous Services expose the same business semantics even when the Console Surface Gateway resolves them through different Service bindings.
- Console Surface Gateway is a narrow browser transport and authority-attenuation boundary, not a business API owner, deployment controller, arbitrary proxy, or System Plane provider.
- Optional Surface Contributions reference `contractId`, `contractVersion`, and `operationId` with typed input bindings instead of generic `admin_action` descriptors.
- The Console repository releases its public package major through its repository-local Changesets and Trusted Publisher workflow; this decision creates no cross-repository release coordinator.
