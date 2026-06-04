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

OpenTelemetry data is an enrichment layer only. Backend APIs map telemetry spans
into business-friendly Technical Operations before the frontend sees them.
Remote HTTP proxy call records are also mapped into Technical Operations, but
they are not OpenTelemetry spans; they are persisted host-side runtime records
with `source = "remote_proxy"`.

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

Remote proxy Technical Operations are correlated by `correlation_id` first.
When possible, they attach to runtime nodes by matching the proxy call's
`span_id` to a telemetry span id and reading the span's
`lenso.function_run_id` or `lenso.outbox_event_id`. If that exact span match is
not available, the backend falls back to matching the proxy call `trace_id`
against safe trace attributes such as `otel.trace_id`, `trace_id`,
`lenso.trace_id`, or `trace.trace_id`. Calls that still cannot be matched remain
story-level operations.

## Remote Proxy Views

Runtime Console intentionally exposes remote proxy calls in two complementary
ways:

- The Remote Calls page is the horizontal operations view. It supports filtering
  by dimensions such as `module_name`, `success`, `remote_status`,
  `error_code`, and `correlation_id`. When a call or correlation filter is
  selected, it can open the matching Runtime Story.
- Runtime Story graph and timeline show remote calls as ordinary
  `remote_proxy_call` nodes under the story's `correlation_id`. This keeps the
  business flow readable without duplicating the same calls in a separate story
  section.
- Runtime Story Technical Operations includes those same calls as
  `source = "remote_proxy"` operations. This places remote module calls beside
  OTEL-derived database, HTTP, worker, and external operations for the selected
  story or execution node.

These are not replacements for each other: Story views explain one business
chain through nodes, while the Remote Calls page supports cross-story
operational diagnosis. Story and Remote Calls navigation is a convenience link
across those views; it does not change the backend matching rules. Remote Calls
list/detail filtering uses exact `correlation_id` matches. Technical Operations
first scope by `correlation_id`, then uses span or trace data only to place a
proxy call on a more specific execution node when those telemetry attributes are
available.

## Provider Boundary

`TelemetrySpanProvider` is the backend abstraction for querying telemetry data. Current local/test support includes:

- no-op provider for normal operation without telemetry storage
- in-memory provider for integration tests

A future queryable backend should implement this provider without changing Runtime Console story semantics.

## Compatibility

`/runtime/traces` currently remains as a temporary redirect alias to `/runtime/stories` for old links. It should not become a product surface or navigation label.
