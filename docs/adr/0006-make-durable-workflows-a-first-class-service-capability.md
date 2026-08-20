# Make durable workflows a first-class Service capability

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Lenso will support Durable Workflows and Sagas as first-class, Service-owned capabilities for Distributed Business Processes that need explicit completion, timeouts, retries, compensation, versioning, or operator intervention. Simple reactions remain Event Choreography, while workflow coordination stays inside an owning business Service rather than becoming a System Plane runtime responsibility.

## Consequences

- Workflow instances have durable identity, step state, version, and idempotency semantics.
- Commands, events, timeouts, retries, and compensations are visible as one cross-Service business story.
- Operators can inspect and safely pause, resume, retry, cancel, or intervene in an execution.
- Lenso does not require every asynchronous interaction to use a workflow.
- The existing embedded runtime can evolve toward these semantics without making one global workflow engine a Data Plane dependency.
