# Keep persistence owned by stateful Modules

The Kernel will define no mandatory or universal State Module. A stateful Module owns its data meaning and either contains a private persistence Adapter or requires a specific semantic Capability whose durability, consistency, ordering, transaction, and recovery behavior are part of its Interface. PostgreSQL is normally an external resource used behind such an Adapter, not a process-wide pool or mandatory remote Module.

## Consequences

- Sharing a physical database cluster does not grant cross-Module table access. Each state owner owns its schema and other Modules use its Capability Interface.
- A database client, connection pool, or table becomes a Module only when it provides a genuine independently replaceable deep Interface; infrastructure does not become a Module merely to satisfy the slogan.
- Durable requirements cannot bind silently to ephemeral test implementations.
- A required state dependency that cannot prepare prevents the owning Module and App from becoming active. Runtime storage loss appears as a Runtime Failure from the affected Capability rather than triggering a Kernel fallback to memory.
