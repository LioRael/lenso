# @lenso/workers

## 0.2.1

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0

## 0.2.0

### Minor Changes

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

- 9985217: Add typed, instance-bound plugin configuration with ordered value, environment and JSON-file sources. Resolve and validate all configuration before business setup, preserve schema output and provide safe source attribution without exposing values or original errors.

  Expose static configuration descriptions through Engine and CLI without reading sources. Keep the filesystem adapter separate from runtime-neutral entrypoints and forward Workers request cancellation to startup preflight.

- Updated dependencies [9985217]
- Updated dependencies
  - @lenso/core@0.2.0
