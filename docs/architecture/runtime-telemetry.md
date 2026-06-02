# Runtime Telemetry Architecture

Runtime Console uses the business runtime model as the source of truth.

## Source Of Truth

Runtime stories come from runtime tables and execution records:

- `platform.outbox`
- `runtime.function_runs`
- `correlation_id`
- causation and runtime metadata carried through headers/input payloads

The Runtime Console product model remains:

- Story
- Execution
- Step
- Event
- Failure
- Retry
- Pressure Point
- Runtime Graph

## Telemetry Enrichment

OpenTelemetry data is an enrichment layer only. Backend APIs map telemetry spans into business-friendly Technical Operations before the frontend sees them.

The frontend does not query collectors, Tempo, or any telemetry backend directly. It calls:

- `GET /admin/runtime/stories/{correlation_id}/technical-operations`
- `GET /admin/runtime/executions/{node_id}/technical-operations`

Technical Operations attach to runtime nodes using safe runtime attributes:

- `lenso.correlation_id`
- `lenso.story_id`
- `lenso.function_run_id`
- `lenso.outbox_event_id`
- `lenso.execution.kind`
- `lenso.execution.name`

If an operation cannot be matched to an execution node, it remains story-level enrichment.

## Provider Boundary

`TelemetrySpanProvider` is the backend abstraction for querying telemetry data. Current local/test support includes:

- no-op provider for normal operation without telemetry storage
- in-memory provider for integration tests

A future queryable backend should implement this provider without changing Runtime Console story semantics.

## Compatibility

`/runtime/traces` currently remains as a temporary redirect alias to `/runtime/stories` for old links. It should not become a product surface or navigation label.
