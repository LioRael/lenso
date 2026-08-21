# ADR 0061: Export OpenTelemetry from a removable Module

- Status: accepted
- Date: 2026-08-21
- Extends: ADR 0030, ADR 0035, ADR 0036, ADR 0037, ADR 0047, ADR 0053, ADR 0056

## Context

Lenso needs optional OpenTelemetry export for Runtime Diagnostics and explicit
application telemetry. OpenTelemetry is useful for correlation, debugging,
metrics, and operational signals, but it is not the owner of business truth.
Putting exporter hooks, trace state, or an SDK feature flag in the portable
Kernel would make an operational product concern part of every App and would
make removal harder than Module removal.

The existing Kernel already provides the required narrow seams:

- bounded, non-blocking structural Runtime Diagnostics;
- opaque ordinary and sealed Invocation Context extensions with issuer,
  audience, and non-overwrite semantics; and
- ordinary Module lifecycle tasks with cooperative cancellation.

## Decision

OpenTelemetry is implemented as the optional `lenso-otel-module` package. A
native factory subscribes to the App's externally supplied `RuntimeDiagnostics`
port, converts only structural records to OTel Logs, and exports them from
bounded asynchronous Module tasks. The Module also accepts explicitly authored
OTel Span, Metric, and Log signals through a declared Capability or a host
handle. Exporter rejection, panic, backpressure, and shutdown cancellation are
Module-local outcomes; they do not fail or block the App.

The Module owns a registered W3C Trace Context propagator. It stores the
`traceparent` and optional `tracestate` in the sealed
`lenso.otel.trace-context` Invocation Context extension. The extension carries
issuer provenance, exact Capability/Operation audiences, and an HMAC proof.
Adapters preserve those fields, and the receiving Module verifies the issuer,
proof, and target audience before interpreting the value. An existing
extension under the same key cannot be replaced.

Runtime Diagnostics are mapped to structural OTel Logs only. The mapping does
not include request or event payloads, Module configuration, secrets, domain
error bodies, arbitrary extensions, or ActorAssertions. Explicit application
telemetry is opt-in and remains volatile; a bounded queue may drop signals.

OpenTelemetry telemetry is not an audit log and is not a durable Story. An App
that needs business evidence, compliance history, or replayable narrative data
must select an Audit or Story Module with its own persistence and ownership.

The exporter boundary is host- and transport-neutral. An OTLP exporter can be
provided by a Module Adapter without adding a network dependency to Kernel.
Rust and Bun tests cover the sealed extension contract and both supported Bun
request wires.

## Consequences

- Apps that do not select the package have no OTel tasks, queues, exporter, or
  trace interpretation.
- Runtime Diagnostics remain useful without OTel, and OTel remains removable
  without changing Kernel APIs or adding a Kernel feature flag.
- Operational telemetry is best effort. Exported signals are not a source of
  durable business truth and must not be used as a replacement for an Audit or
  Story owner.
- Application authors must declare the telemetry Capability and binding when
  they want Module-to-Module explicit signal submission; there is no ambient
  telemetry client.

## Removal test

Removing the `lenso-otel-module` workspace member, its native factory registry
entry, its optional Capability bindings, TypeScript helper, and documentation
leaves the portable Kernel, Runtime Diagnostics, Invocation Context, and Bun
Adapter contracts unchanged. No Kernel feature flag, exporter hook, or OTel
state remains.
