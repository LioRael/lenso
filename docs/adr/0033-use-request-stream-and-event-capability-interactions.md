# Use request, stream, and event Capability interactions

Capability Operations will use three interaction primitives: `request` for one response, `stream` for ordered and cancellable flow, and `event` for one-way publication to zero or more consumers. Command and query remain domain meanings expressed by Capability contracts rather than separate Kernel interactions.

## Consequences

- Kernel invocation context contains only the caller Module key, request identity, deadline, cancellation, and opaque extensions. Actor, tenant, Story, telemetry, and idempotency meaning remain outside the Kernel.
- Runtime failures such as unavailable providers, deadlines, cancellation, resource exhaustion, protocol violations, and internal failures remain distinct from Capability-defined domain errors.
- Provider channels are bounded and report resource exhaustion rather than accumulating unbounded work or requiring Kernel persistence.
- Streams are bidirectional and define backpressure, cancellation, and independent half-close; one-way streams are restricted uses of the same interaction.
- Kernel events are ephemeral and backpressured. Fan-out admission may be partial across independently bounded subscribers, is reported explicitly, and never causes automatic replay. Durable delivery, replay, broker integration, and Outbox behavior belong to explicit Modules.
- A Capability is a deep, cohesive role Interface that may contain several related Operations; it is neither one Capability per function nor one universal Interface per Module.
- Kernel Request IDs correlate invocation, cancellation, and Runtime Diagnostics only. A business idempotency key is an explicit Capability field or sealed extension rather than an inferred use of the Request ID.
