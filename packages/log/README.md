# @lenso/log

Structured Pino logging for Lenso plugins and hosts. `createLogger()` emits JSON
to stderr by default, keeping stdout available for CLI output and MCP protocol
messages. Configure your process supervisor to collect stderr. Do not also
export these logs through an OpenTelemetry log exporter unless duplicate
records are intended; this package only correlates active trace/span IDs and
does not register an SDK or exporter.

```ts
import { createLogger } from "@lenso/log";

const log = createLogger({ level: "info" });
log.child({ instanceId, pluginId, operation }).info({ jobId }, "started");
```

Set `pretty: true` for synchronous readable development output, without a worker
transport. An existing Pino logger can
be supplied with `logger`; the returned child is enriched with trace IDs but
the supplied logger is not closed. Caller-owned logger configuration and
transports remain the caller's responsibility.

Common structured keys such as `authorization`, `cookie`, `password`, `token`,
`secret`, `session`, `credentials`, `signature`, and `connectionString` are
redacted by default, along with common `body` and `payload` fields. For newly
created loggers, default `err`, `error`, and configured `errorKey` serializers
replace all values (including non-Error objects and primitive throws) with
`{ type: "Error" }`. Messages, stacks, causes and arbitrary `.code` properties
are not forwarded. Direct Error writes without a message use
`"Error details omitted"` rather than Pino's inferred error message.

An optional `classifyError(error)` callback may return explicitly public
`{ code, phase }` labels. Only those two string fields are copied; callback
failure falls back to omission. Classify by trusted error identity or application
policy, not by copying arbitrary `.code`, `.message`, or `.cause` properties.
Instance, plugin, operation, request and trace/span correlation fields remain
available in ordinary structured bindings.

Explicit custom serializers (including child serializers), hooks and redaction
are SDK/logger owner extension points and may opt into raw details. An external
`logger` keeps its owner's serializers and transports: wrapping it adds trace
correlation, **not error omission**. `classifyError` applies only to newly
created loggers, not borrowed loggers. Configure an external logger's safe
serializers at its owner boundary before supplying it.

This is not a sandbox or comprehensive secret discovery. Free-form messages,
interpolation arguments, deeply nested/custom keys, and external logger
configuration require application-level policy. Avoid logging full payloads.
