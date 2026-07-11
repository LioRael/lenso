# Enforce logical Service data ownership

Each Service exclusively owns its Service Data, schema migrations, and persistence access, and other Services interact with that data only through Service Contracts. Multiple Service Stores may share one physical Postgres cluster during early operation, but cross-Service table access and database transactions are forbidden; Distributed Business Processes use local transactions, Outbox delivery, idempotent consumption, and explicit progress or compensation instead.

## Consequences

- Physical database isolation can evolve independently from business ownership.
- Shared infrastructure must still support separate schemas or databases, credentials, migrations, and backups by Service.
- A Service extraction does not require first untangling direct table access from other Modules or Services.
- Cross-Service consistency requires durable messaging and workflow evidence rather than hidden distributed ACID behavior.
