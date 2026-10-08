---
"@lenso/core": minor
"@lenso/cli": minor
"@lenso/engine": minor
"@lenso/auth": minor
"@lenso/tasks": minor
"@lenso/web": minor
"@lenso/workers": minor
"@lenso/log": minor
"@lenso/otel": minor
---

Migrate all oRPC consumers to exactly 2.0.0-beta.42 and its v2 wire format,
client/handler APIs, error status policy and typed cancellable iterators.
Clients and servers must upgrade together; v1 is not supported.

Add independent structured logging and application-owned OpenTelemetry bootstrap.
Correlate plugin lifecycle, CLI operations, durable queue attempts and Web requests
without changing ordinary async business services. Carry bounded W3C trace metadata
between processes, create fresh linked retry spans, preserve external SDK/logger
ownership and bound telemetry flush/shutdown. Provide a separate native Workers
Traces entry without a Node SDK.
