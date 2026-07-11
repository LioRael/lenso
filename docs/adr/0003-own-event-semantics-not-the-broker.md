# Own event semantics, not the broker

Lenso will define transport-independent event contracts, envelopes, compatibility rules, delivery state, idempotent consumption, retry, dead-letter, replay, and operational evidence while treating message brokers as replaceable Transport Adapters. This extends the existing transactional Outbox model across Service boundaries without binding Modules to a broker product or turning Lenso into message infrastructure.

## Consequences

- Local and linked execution may continue using the current Postgres Outbox and in-process relay.
- Autonomous Services publish the same Event Envelopes through an installed Transport Adapter.
- Consumers own an Inbox or equivalent durable deduplication state.
- Broker topics, partitions, offsets, and vendor delivery models do not become Module contract vocabulary.
- Official adapters can be added incrementally without changing Event Contracts.
