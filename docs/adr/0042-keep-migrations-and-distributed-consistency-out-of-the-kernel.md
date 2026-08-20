# Keep migrations and distributed consistency out of the Kernel

The Kernel will not discover or execute database migrations and will not provide transactions across Capability Interfaces. A state owner supplies its own explicit setup and upgrade workflow; preparation verifies the selected storage and schema without applying irreversible changes unless the Module deliberately enables an idempotent development policy.

## Consequences

- Transactions remain inside one state owner's implementation.
- Cross-Module consistency uses explicit workflow, idempotency, and compensation semantics.
- Workflow, Outbox, durable event delivery, idempotency, backup, and recovery may be supplied as deep optional Modules or owner-local implementation, but none becomes a Kernel prerequisite.
- Module package tooling, rather than App boot, presents and runs owner-specific upgrade commands.
