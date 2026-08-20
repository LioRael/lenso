# Runtime Telemetry Architecture

> **Legacy v0.3.x architecture:** This page describes the maintained
> Service-oriented implementation and is not normative for vNext. Read
> [lenso-vnext.md](lenso-vnext.md) for vNext decisions.

Console uses the business runtime model as the source of truth.

## Source Of Truth

Runtime stories come from runtime tables and execution records:

- `platform.outbox`
- `runtime.function_runs`
- `correlation_id`
- causation and runtime metadata carried through headers/input payloads

The Console product model remains:

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
with `source = "provider_http"`. Provider runtime function invocations are mapped
from host-written execution logs with `source = "remote_runtime"`; they enrich
the function run node rather than creating a second remote-function surface.

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

Console intentionally exposes remote proxy calls in two complementary
ways:

- The Provider Calls page is the horizontal operations view. It supports filtering
  by dimensions such as `module_name`, `success`, `provider_status`,
  `error_code`, and `correlation_id`. When a call or correlation filter is
  selected, it can open the matching Runtime Story.
- Runtime Story graph and timeline show provider calls as ordinary
  `provider_http_call` nodes under the story's `correlation_id`. This keeps the
  business flow readable without duplicating the same calls in a separate story
  section.
  The node's `metadata.source_metadata` is the Story UI contract for compact
  timeline summaries and inspector details: module, method, declared path,
  provider path/status, duration, request/trace/span ids, path params, error
  code, retryability, and error details.
- Runtime Story Technical Operations includes those same calls as
  `source = "provider_http"` operations. This places provider service calls beside
  OTEL-derived database, HTTP, worker, and external operations for the selected
  story or execution node.

## Provider Runtime Operations

Provider runtime functions keep the normal Runtime Story shape: the business node
is still the `function_run`. When the worker invokes an out-of-process module
function, it writes a host-owned execution log with compact operation metadata:
module, function name, provider path, request id, trace/span ids, duration,
success, retryability, and error code/details when present. Runtime Admin maps
those logs into `source = "remote_runtime"` Technical Operations for both:

- `GET /admin/runtime/stories/{correlation_id}/technical-operations`
- `GET /admin/runtime/executions/{node_id}/technical-operations`

These operations attach to the function run node through `execution_id`. They do
not replace function run lifecycle logs or create a horizontal Provider Calls
page; the Provider Calls page remains specific to HTTP proxy call history.

## Application Execution Logs

In-process function handlers can add business-relevant logs with the standard
`tracing::{info, warn, error}!` macros. Use the exact
`lenso::execution` target, keep the message static, and put dynamic values in a
JSON `attributes` object:

```rust
use serde_json::json;

tracing::info!(
    target: "lenso::execution",
    attributes = %json!({
        "reservation_id": reservation_id,
        "outcome": "available",
    }),
    "Inventory reservation checked"
);
```

The runtime binds correlation, story, function-run, trace, function, service,
and workload identity from the immutable execution scope. Handler fields whose
names start with `lenso.` are discarded and cannot override that identity.

Attributes are recursively redacted when a key contains a sensitive term such
as `authorization`, `cookie`, `password`, `secret`, `token`, `api_key`,
`access_key`, `credential`, or `email` (case-insensitive). Messages that look
like credentials or email addresses are replaced as a whole; this is why
messages should be static. Bodies, attributes, and the in-flight log queue are
bounded. A full queue, disabled writer, timeout, or storage failure produces an
incomplete capture report and a sanitized Runtime warning, but never changes the
function's business result. This first slice does not durably project that
capture report: local Inspector coverage describes whether its read source was
available, not whether every handler event was captured. Durable capture-status
projection remains a later evidence-contract extension.

This first slice captures events emitted while an in-process function handler
future is being polled. It does not yet establish an execution-log scope for
HTTP handlers, outbox handlers, provider-service runtimes, or detached tasks
created with `tokio::spawn`. The scoped forwarding subscriber preserves normal
span and event callbacks to an installed Host subscriber, but the `tracing`
subscriber API cannot forward dispatch downcasts. Code inside this initial
handler scope should not call dispatch-downcast extensions such as
`tracing_opentelemetry::OpenTelemetrySpanExt`; use the `ExecutionContext` trace
data supplied by the runtime instead.

These are not replacements for each other: Story views explain one business
chain through nodes, while the Provider Calls page supports cross-story
operational diagnosis. Story and Provider Calls navigation is a convenience link
across those views; it does not change the backend matching rules. Provider Calls
list/detail filtering uses exact `correlation_id` matches. Technical Operations
first scope by `correlation_id`, then uses span or trace data only to place a
proxy call on a more specific execution node when those telemetry attributes are
available.

## Provider Boundary

`TelemetrySpanProvider` is the backend abstraction for querying telemetry data. Current local/test support includes:

- no-op provider for normal operation without telemetry storage
- in-memory provider for integration tests

A future queryable backend should implement this provider without changing Console story semantics.

## Console Routes

Console exposes canonical product routes only:

- `/runtime/stories` is the Story workbench, including graph, waterfall,
  flame, and timeline views for a selected story.
- `/operations/remote-calls`, `/operations/functions`,
  `/operations/dead-letters`, and `/operations/queues` are the horizontal
  operations views.

Runtime Story detail is the API surface for story timeline data:
`GET /admin/runtime/stories/{correlation_id}` returns `timeline_items`.
There is no standalone admin runtime timeline endpoint.

Legacy Console aliases such as `/runtime/traces`, `/timeline`,
`/events`, `/functions`, `/dead-letters`, `/remote-proxy-calls`, and `/queues`
are intentionally not preserved. Architecture checks fail if those aliases are
reintroduced in the Console router.
