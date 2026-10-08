# @lenso/web

## 0.2.0

### Minor Changes

- c361fdf: Add an optional `@lenso/web/bun` listener plugin that binds an explicitly selected Web instance, applies application-owned ingress policy, reports the actual listening address, and registers shutdown cleanup immediately after startup.
- Migrate all oRPC consumers to exactly 2.0.0-beta.42 and its v2 wire format,
  client/handler APIs, error status policy and typed cancellable iterators.
  Clients and servers must upgrade together; v1 is not supported.

  Add independent structured logging and application-owned OpenTelemetry bootstrap.
  Correlate plugin lifecycle, CLI operations, durable queue attempts and Web requests
  without changing ordinary async business services. Carry bounded W3C trace metadata
  between processes, create fresh linked retry spans, preserve external SDK/logger
  ownership and bound telemetry flush/shutdown. Provide a separate native Workers
  Traces entry without a Node SDK.

### Patch Changes

- Updated dependencies [9985217]
- Updated dependencies
  - @lenso/core@0.2.0
