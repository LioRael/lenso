# @lenso/cli

## 0.22.0

### Minor Changes

- 8d9f00a: Project declared output schemas and business error metadata through existing Operations, CLI and MCP. Add opt-in same-user dev connections with generation fences and bounded payload-free recent call metadata, borrowing the host's running app.
- e3fe4a8: Add explicit, opt-in application initialization with trusted host authorization,
  existing configuration preflight, a durable first-owner/config transaction contract,
  resumable activation and a startup readiness fence. The matching native CLI source
  adds setup status/apply/activate; its release remains separate. Include a real
  private SQLite first-run example, without enabling a bootstrap HTTP route or
  installing platform secrets implicitly.

### Patch Changes

- 96f021f: Ship installed CLI guidance, exclusive development output ownership, archive-aware source scanning, and distinct unavailable-versus-invalid MCP credential verification errors.
- Updated dependencies [8d9f00a]
- Updated dependencies [e3fe4a8]
- Updated dependencies [96f021f]
- Updated dependencies [bfd8133]
  - @lenso/engine@0.8.0
  - @lenso/core@0.4.0

## 0.21.0

### Minor Changes

- b21dc1f: Add explicit development workspaces for native TypeScript applications and
  separately built services. Start dependencies after readiness, report candidate
  and retained-instance state, rebuild affected services, and close dependents
  before their dependencies. Expose the workspace through `lenso dev --workspace`
  without changing single-application development or business operation allowlists.
- f38c336: Remove the standalone TypeScript lenso executable. Keep operation APIs and expose an internal adapter module for the sole Rust lenso command; Engine generation and development lifecycles remain shared.

### Patch Changes

- f0cab26: Align the two shipped Engine extension examples with the Engine 0.6 and 0.7 peer contracts and verify compatibility in the packed-consumer test.
- Updated dependencies [b21dc1f]
  - @lenso/engine@0.7.0

## 0.20.1

### Patch Changes

- c310eac: Make bundle resolution inherit custom launch conditions with explicit target overrides. Track development inputs through the local source graph and declared watches, preserving imported JSON, dependency repair, symlink redirects, and ready-hook watch ownership without restarting for ordinary business data.

  Classify Engine-observed cancellation with safe diagnostics and retain actual execution lifetime. Prepare immutable Manage selections once with revocable handles and request-local authorization. MCP discovery and dispatch use the same declaration snapshots while preserving existing tool names; policies should use exact plugin instances and declared methods rather than original Operation object identity.

- Updated dependencies [c310eac]
  - @lenso/engine@0.6.0

## 0.20.0

### Minor Changes

- 615b1a4: Support explicit application roots and configuration entries across Engine and CLI, with inert workspace candidate diagnostics and shared build/dev entry selection. Add opt-in `lenso-source` exports for TypeScript-capable tools while preserving default JavaScript and declaration exports, and forward source conditions to owned development processes.

### Patch Changes

- Updated dependencies [615b1a4]
  - @lenso/engine@0.5.0
  - @lenso/core@0.3.1

## 0.19.2

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0
  - @lenso/engine@0.4.0

## 0.19.1

### Patch Changes

- 82c9809: Add explicit operation-level domain error projection and consistent safe Manage status mappings while preserving internal causes, cleanup aggregation and native oRPC envelopes. Bound public diagnostic fields and validation paths without exposing dynamic input keys or raw exception text.

  Provide optional Web OpenAPI endpoints with RFC 9457 Problem Details, shared procedure selection and schema generation, safe late stream errors, and bounded OpenAPILink error decoding. OpenAPI remains an optional peer and adds no routes by default.

  Omit raw structured error details in default logs and owned OpenTelemetry exports. Preserve existing SDK ownership, service results, cancellation and resource-drain behavior.

- Updated dependencies [82c9809]
  - @lenso/core@0.2.1
  - @lenso/engine@0.3.0

## 0.19.0

### Minor Changes

- 6713422: Add optional instance-bound management declarations, shared finite invocation boundaries, trusted contextual bindings, and independent entry allowlists. Provide SDK-independent agent tools and an explicitly mounted oRPC adapter while preserving ordinary service methods and application-owned authorization and lifecycle.
- 9985217: Add typed, instance-bound plugin configuration with ordered value, environment and JSON-file sources. Resolve and validate all configuration before business setup, preserve schema output and provide safe source attribution without exposing values or original errors.

  Expose static configuration descriptions through Engine and CLI without reading sources. Keep the filesystem adapter separate from runtime-neutral entrypoints and forward Workers request cancellation to startup preflight.

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

- Updated dependencies [6713422]
- Updated dependencies [9985217]
- Updated dependencies
  - @lenso/engine@0.2.0
  - @lenso/core@0.2.0
