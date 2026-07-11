# Federate Runtime Stories across Services

Lenso will preserve Runtime Story as durable business-operation evidence across Autonomous Services rather than reducing observability to technical distributed traces. Each Service records Story Segments under a propagated Story Context, and the System Plane or Runtime Console assembles a Federated Runtime Story enriched by correlated OpenTelemetry traces, metrics, and logs without becoming responsible for Service-local evidence capture.

## Consequences

- Story identity survives asynchronous delivery, retries, compensations, and changing trace identifiers.
- Business-critical progress remains queryable even when technical telemetry is sampled or unavailable.
- Services retain local Story Segments while aggregation is delayed or offline.
- Runtime Console can answer both business-progress and technical-diagnostic questions through linked evidence.
- Cross-Service context envelopes must propagate Story Context consistently through HTTP, gRPC, events, and workflows.
