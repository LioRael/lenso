# Standardize optional Tenant Context

Lenso will standardize Tenant Context propagation and Tenant Isolation while allowing each Service to declare a Tenancy Mode of none, optional, or required. Tenant scope is derived from verified actor or Service context and must be explicit in requests, events, background work, and workflows; tenant lifecycle remains business Module behavior, and applications without multi-tenancy do not inherit unnecessary runtime complexity.

## Consequences

- Service Contract checks can reject missing or incompatible Tenant Context requirements.
- Background work cannot silently fall back to a default tenant.
- Services may use row, schema, or database isolation while preserving the same Tenant Isolation contract.
- Organization or account Modules own tenant creation, membership, and lifecycle rather than the platform core.
- Lenso remains suitable for internal and single-tenant systems as well as multi-tenant products.
