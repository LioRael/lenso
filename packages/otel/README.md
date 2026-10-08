# @lenso/otel

The root export is the official OpenTelemetry API only. Core, Engine and Tasks
record telemetry without initializing an SDK; without an entry bootstrap it is
no-op. Logger is a separate structural interface from `@lenso/core`.

Use `@lenso/otel/bun` explicitly from a Bun entry or preload:

```ts
import { bootstrapTelemetry } from "@lenso/otel/bun";
import { createORPCInstrumentation } from "@lenso/otel/orpc";

const telemetry = await bootstrapTelemetry({
  serviceName: "my-app",
  instrumentations: [createORPCInstrumentation()],
});
const { main } = await import("./server.ts"); // main resolves only after server/workers drain
try {
  await main();
} finally {
  await telemetry.shutdown();
}
```

Keep the entry alive until its server and durable workers drain, then shut down.
Never bootstrap from config, plugin imports or individual requests. No automatic
Node instrumentation or process exception hooks are installed. `traceExporter`,
`metricExporter`, `sampler`, `contextManager`, `propagator` and `instrumentations`
accept official SDK/API interfaces.
Supplied exporters/context managers are borrowed by default; set
`takeOwnership: true` only when transferring their lifetime to this bootstrap.
Owned defaults are disposed once, even when final export or instrumentation
cleanup fails. In `mode: "external"`, providers/global context remain untouched;
only explicitly supplied instrumentation registrations are enabled/disabled.
The external SDK owner must perform its own final flush/shutdown.
Default OTLP HTTP export has a 2048-span queue, 512-span batches and bounded
timeouts. Flush/shutdown reject on timeout/export failure; a timed-out exporter
may still complete in the background, so ownership remains reserved until actual
shutdown completes. External mode neither registers, flushes nor shuts down
another owner's providers/context. A deadline cannot cancel an arbitrary
caller-supplied exporter: a stuck cleanup keeps ownership reserved.

For a finite CLI command, preload a module that calls
`bootstrapTelemetry({serviceName:"my-cli", flushOnCliExit:true})`, then run
`bun --preload ./telemetry.ts ./node_modules/@lenso/cli/dist/bin.js help --json`.
The CLI ends its command span and awaits bounded flush/shutdown without changing
the command exit code. Do not use the finite-command preload for `dev` child
process instrumentation.

oRPC instrumentation is optional and shares this SDK. Its propagation defaults
to true. If Web's `telemetry.requestLifetime` or an HTTP/Fetch instrumentation
owns propagation, pass `createORPCInstrumentation({propagationEnabled:false})`.
Use only one HTTP propagation owner. The helper rejects simultaneous oRPC OTel
and Workers tracer ownership.
oRPC owns its HTTP/procedure spans. Web's optional INTERNAL lifetime span measures
body/work/cleanup ownership, not a second HTTP server boundary.

## Error export policy

Owned Bun bootstrap defaults to `errorDetails: "omit"`. Its existing batch
processor exports through `createSafeSpanExporter`, which passes an independent
readable span snapshot to the destination. It removes `exception` events,
`exception.*` and `error.*` attributes on spans/events/links, and every
`status.message`, while retaining status codes, timing, topology, correlation
IDs and application-approved labels such as `lenso.error.code` and
`lenso.phase`. It never infers a public code from an arbitrary thrown object's
`.code`. Without application classification, omit raw details entirely.

The installed `@orpc/opentelemetry@2.0.0-beta.42` configuration exposes only
`propagationEnabled` plus standard instrumentation settings. Its official
implementation records exceptions and sets error status messages from
`exception.message`; it has no exception-filter callback. The
[official oRPC integration docs](https://orpc.dev/docs/integrations/opentelemetry)
document propagation configuration, not an error filtering hook. This package
therefore uses the supported
[OTel exporter pipeline](https://opentelemetry.io/docs/specs/otel/trace/sdk/#span-exporter),
without replacing oRPC's tracer or patching dependencies.

`createORPCInstrumentation()` alone does **not** apply this policy. In
`mode: "external"`, the existing SDK owner must wrap **every** exporting pipeline:

```ts
import { createSafeSpanExporter } from "@lenso/otel/bun";
// In the owner's existing provider setup, not a second SDK:
const processor = new BatchSpanProcessor(createSafeSpanExporter(ownerExporter));
```

The helper delegates exporter lifecycle methods and does not transfer ownership.
Raw sibling exporters/processors still see raw spans. Owned bootstrap may opt
into `errorDetails: "raw"` only as an explicit SDK-owner decision.

This is omission at export, not a sandbox: official instrumentation still records
raw exception details in SDK memory, and earlier processors can observe them.
Names, resources, unrelated attributes, free-form/custom events and payloads
are not secret-scanned. Applications must use public classification labels and
avoid sensitive text in those fields. Metrics and native Workers platform export
are outside this trace exporter policy; the Workers owner must configure its
platform policy separately. Export/snapshot failures report a generic failed
export to the SDK, not an exception thrown back into the business operation.
Explicit flush/shutdown may still reject as documented.

Task metadata retains only bounded `traceparent`/`tracestate`, separate from
payload and identity. Each durable attempt is a fresh root with a link to its
original producer, not a child of whichever request happens to drive a worker.
Trace context confers no authentication or authorization.

## Workers

`@lenso/otel/workers` is a separate browser-safe entry. Install the optional peer
`@orpc/cloudflare@2.0.0-beta.42`, enable Wrangler `observability.traces.enabled`,
then explicitly call `bootstrapWorkerTracing()` once in the Worker entry.
It reuses official Workers Traces and oRPC's `CloudflareTracer`, with no Node SDK,
OTLP client, files or process hooks. Never enable `ORPCInstrumentation` alongside
it. The platform owns request spans and export; no provider is shut down per request.
The existing Workers greeting example demonstrates this route.

This route does not bridge native Workers spans to the OTel API: Lenso's
API-only lifecycle/task spans and trace-ID log enrichment require an SDK/context
owner and are not supplied by this native entry. Use platform logs or an
application-supplied structural logger in Workers, not the Bun Pino entry.
Native stream spans currently have no yielded/enqueued events, and oRPC
propagation is not configurable on this route. Custom OTLP export from Workers
is not implemented. Workers' platform export lifecycle applies, rather than
Bun's explicit `forceFlush`/`shutdown`.
