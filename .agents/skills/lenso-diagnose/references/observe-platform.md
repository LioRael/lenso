# Observe and platform boundaries

## Locate existing evidence

Read the [OTel guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/otel/README.md) and [logging guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/log/README.md) when traces/logs are relevant.

- `@lenso/otel` root is API-only and no-op without SDK bootstrap. Check the existing Bun entry/preload for `bootstrapTelemetry` from `@lenso/otel/bun` **before config import**, not plugin setup. Do not add global tooling or a collector merely to diagnose.
- For finite CLI commands, existing `flushOnCliExit: true` enables bounded final flush/shutdown in CLI `finally`. Export failure prints fixed stderr text without changing the business exit code. External SDK mode leaves flush/drain/shutdown with its owner; long-lived hosts drain work before telemetry shutdown.
- `@lenso/log` defaults to stderr JSON, separate from OTLP telemetry and without an OTel log exporter. Locate the actual deployed collector receiver/backend and process log supervisor from trusted deployment configuration, without exposing endpoint credentials. An export endpoint is not necessarily a query backend. No collector/storage means no stored telemetry lookup; use retained stderr if available.
- Correlate app/plugin instance, operation, `jobId`, attempt, `traceId` and `spanId`. A task attempt is a fresh root **linked** to the producer, not necessarily the same trace. IDs provide correlation, not authorization.

For opaque errors, consult only authorized safe logs/backend records. Keep payloads, configs, credentials, arbitrary exception text and stacks out of client diagnostics. Report a missing collector/backend or safe diagnostic channel as a gap rather than guessing an Observe query command.

## MCP request loss or cancellation

Read the [MCP guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/mcp/README.md) for the existing local adapter. It requires explicit declarations/allowlist, a converted **object** input schema representable by the SDK and bounded plain JSON output; `runtime-validation-only` is insufficient. It admits one call; `adapter-busy` does not queue another.

The CLI service call has no signal. The adapter checks SDK cancellation before/after the call and drains admitted work/cleanup even if the SDK drops the response. Request cancellation or connection loss is **not durable task cancellation**, rollback or proof of failure. Query authorized business/task status before deciding on recovery. Do not fabricate an output schema from `outputDescription` or bypass allowlist/schema protection.

## Web and Workers

Use the [Web guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/web/README.md) for response/source lifetime and the [Workers guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/workers/README.md) for request-owned apps. Cooperative signals are not rollback. Consume/cancel task-owned response bodies so cleanup can finish. `waitUntil` does not extend an app resource lifetime past response EOF.

Workers native tracing uses the separate `@lenso/otel/workers` route and platform export lifecycle, not Bun SDK/Pino assumptions or per-request provider shutdown. Native tracing does not bridge Lenso OTel API spans/log enrichment. Use existing platform logs/traces and bindings; isolate termination does not guarantee cleanup.
