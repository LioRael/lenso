# Standardize optional Tenant Context

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Lenso will standardize Tenant Context propagation and Tenant Isolation while allowing each Service to declare a Tenancy Mode of none, optional, or required. Tenant scope is derived from verified actor or Service context and must be explicit in requests, events, background work, and workflows; tenant lifecycle remains business Module behavior, and applications without multi-tenancy do not inherit unnecessary runtime complexity.

## Consequences

- Service Contract checks can reject missing or incompatible Tenant Context requirements.
- Background work cannot silently fall back to a default tenant.
- Services may use row, schema, or database isolation while preserving the same Tenant Isolation contract.
- Organization or account Modules own tenant creation, membership, and lifecycle rather than the platform core.
- Lenso remains suitable for internal and single-tenant systems as well as multi-tenant products.
