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
redacted by default, along with common `body` and `payload` fields. Default `err`
and `error` serializers omit Error messages and stacks. Explicit custom
serializers/redaction override this policy. This is not comprehensive secret
discovery: free-form strings, deeply nested/custom keys, and external logger
configuration require application-level policy. Avoid logging full payloads.
