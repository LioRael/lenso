# @lenso/mcp

## 0.7.0

### Minor Changes

- 29b4e93: Observe the actual operation span through an optional trusted invocation callback, forward it explicitly through Manage, and expose bounded redacted MCP execution metadata without changing business output contracts or authorization.

### Patch Changes

- 1779f5e: Expose bounded redacted failure diagnostics in result metadata, with structured content when no successful output schema is declared. Preserve official SDK output validation and bound dedicated stdio scalar result wrappers.
- Updated dependencies [29b4e93]
  - @lenso/engine@0.9.0
  - @lenso/manage@0.6.0
  - @lenso/cli@0.22.1

## 0.6.0

### Minor Changes

- 8d9f00a: Project declared output schemas and business error metadata through existing Operations, CLI and MCP. Add opt-in same-user dev connections with generation fences and bounded payload-free recent call metadata, borrowing the host's running app.

### Patch Changes

- 96f021f: Ship installed CLI guidance, exclusive development output ownership, archive-aware source scanning, and distinct unavailable-versus-invalid MCP credential verification errors.
- Updated dependencies [8d9f00a]
- Updated dependencies [e3fe4a8]
- Updated dependencies [96f021f]
- Updated dependencies [bfd8133]
  - @lenso/engine@0.8.0
  - @lenso/cli@0.22.0
  - @lenso/manage@0.5.4
  - @lenso/core@0.4.0

## 0.5.0

### Minor Changes

- fed11ff: Replace borrowed and HTTP MCP entry-local `operation_N` tool names with stable
  `operation_` names derived from the exact plugin ID and method tuple. This is a
  pre-1.0 protocol naming change: clients must rediscover tools rather than reuse
  cached positional names.

  Add permission-bound, single-use cursor pagination with fresh visibility checks,
  bounded redacted `structuredContent` alongside text results, and an explicit
  personal-token HTTP mode that verifies current credentials on every protected
  request without advertising an OAuth authorization server.

## 0.4.0

### Minor Changes

- c310eac: Make bundle resolution inherit custom launch conditions with explicit target overrides. Track development inputs through the local source graph and declared watches, preserving imported JSON, dependency repair, symlink redirects, and ready-hook watch ownership without restarting for ordinary business data.

  Classify Engine-observed cancellation with safe diagnostics and retain actual execution lifetime. Prepare immutable Manage selections once with revocable handles and request-local authorization. MCP discovery and dispatch use the same declaration snapshots while preserving existing tool names; policies should use exact plugin instances and declared methods rather than original Operation object identity.

### Patch Changes

- Updated dependencies [c310eac]
  - @lenso/engine@0.6.0
  - @lenso/manage@0.5.0
  - @lenso/cli@0.20.1

## 0.3.1

### Patch Changes

- Updated dependencies [615b1a4]
  - @lenso/engine@0.5.0
  - @lenso/cli@0.20.0
  - @lenso/core@0.3.1
  - @lenso/manage@0.4.1

## 0.3.0

### Minor Changes

- 780b02e: Add borrowed-runtime MCP tools, dedicated borrowed stdio, and opt-in authenticated
  Fetch Streamable HTTP ingress with bounded admission and request cancellation.
  Keep the existing trusted local stdio entry compatible.

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0
  - @lenso/engine@0.4.0
  - @lenso/manage@0.4.0
  - @lenso/cli@0.19.2

## 0.2.1

### Patch Changes

- 82c9809: Add explicit operation-level domain error projection and consistent safe Manage status mappings while preserving internal causes, cleanup aggregation and native oRPC envelopes. Bound public diagnostic fields and validation paths without exposing dynamic input keys or raw exception text.

  Provide optional Web OpenAPI endpoints with RFC 9457 Problem Details, shared procedure selection and schema generation, safe late stream errors, and bounded OpenAPILink error decoding. OpenAPI remains an optional peer and adds no routes by default.

  Omit raw structured error details in default logs and owned OpenTelemetry exports. Preserve existing SDK ownership, service results, cancellation and resource-drain behavior.

- Updated dependencies [82c9809]
  - @lenso/engine@0.3.0
  - @lenso/cli@0.19.1

## 0.2.0

### Minor Changes

- 6713422: Add optional instance-bound management declarations, shared finite invocation boundaries, trusted contextual bindings, and independent entry allowlists. Provide SDK-independent agent tools and an explicitly mounted oRPC adapter while preserving ordinary service methods and application-owned authorization and lifecycle.

### Patch Changes

- Updated dependencies [6713422]
- Updated dependencies [9985217]
- Updated dependencies
  - @lenso/engine@0.2.0
  - @lenso/cli@0.19.0
