# Expose non-blocking Runtime Diagnostics

The Kernel will be instrumentable without owning an observability product. An explicitly bound observer may receive authoritative structural runtime facts through an opt-in, read-only Runtime Diagnostics Interface backed by a bounded asynchronous receiver; OpenTelemetry, Story, Console, logging, persistence, aggregation, sampling, and export remain Module-owned concerns.

## Consequences

- With no observer, invocation hot paths perform a fast interest check and avoid diagnostic record construction and dynamic allocation.
- Kernel hot paths never await observers or execute observer-owned callbacks. They only attempt a non-blocking enqueue into independently bounded observer queues.
- Diagnostic overload drops records instead of affecting application execution. Sequence gaps or dropped-record counters make loss visible.
- Observer failure, shutdown, and export failure cannot alter invocation results, Capability bindings, lifecycle transitions, or supervision decisions.
- Runtime facts contain structural identities, timing, lifecycle, saturation, restart, and Runtime Failure information. They exclude business payloads, configuration, secrets, domain-error bodies, opaque extension values, and Actor assertions by default.
- Diagnostic delivery and export are excluded from their own observation path to prevent recursion.
- Runtime Diagnostics are local, ephemeral, and best-effort. They are not an audit log, durable Story, or correctness mechanism.
- Trace-context creation and propagation remain explicit instrumentation concerns carried through opaque Invocation Context extensions; the read-only Diagnostics Interface cannot mutate calls.
- Story Modules derive durable business timelines from explicit business or audit events and may enrich them with diagnostics. OpenTelemetry Modules establish and propagate trace extensions and may consume diagnostics; neither capability is required by the Kernel or every App.
