# @lenso/web

## 0.3.2

### Patch Changes

- 615b1a4: Support explicit application roots and configuration entries across Engine and CLI, with inert workspace candidate diagnostics and shared build/dev entry selection. Add opt-in `lenso-source` exports for TypeScript-capable tools while preserving default JavaScript and declaration exports, and forward source conditions to owned development processes.
- Updated dependencies [615b1a4]
  - @lenso/core@0.3.1

## 0.3.1

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0

## 0.3.0

### Minor Changes

- 82c9809: Add explicit operation-level domain error projection and consistent safe Manage status mappings while preserving internal causes, cleanup aggregation and native oRPC envelopes. Bound public diagnostic fields and validation paths without exposing dynamic input keys or raw exception text.

  Provide optional Web OpenAPI endpoints with RFC 9457 Problem Details, shared procedure selection and schema generation, safe late stream errors, and bounded OpenAPILink error decoding. OpenAPI remains an optional peer and adds no routes by default.

  Omit raw structured error details in default logs and owned OpenTelemetry exports. Preserve existing SDK ownership, service results, cancellation and resource-drain behavior.

### Patch Changes

- Updated dependencies [82c9809]
  - @lenso/core@0.2.1

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
