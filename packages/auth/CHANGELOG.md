# @lenso/auth

## 0.5.6

### Patch Changes

- Accept the verified Core 0.4, Engine 0.9 and Manage 0.6 cohort in optional peer dependencies while retaining older supported ranges. Package implementation payloads are unchanged; this corrects strict npm dependency resolution for Console consumers.

## 0.5.5

### Patch Changes

- c490be2: Reject OAuth exchanges that expire during permission reads or CAS acknowledgements. Cap access-token expiry by the grant deadline and report the remaining lifetime in `expires_in`, without retrying consumed code or refresh writes.

  Preserve RFC7009 success responses for missing, expired or already revoked tokens. Reconcile conflicting or unacknowledged revocations with one authoritative read, without replaying the write or claiming success while a token remains live.

## 0.5.4

### Patch Changes

- Add an optional native-client OAuth authorization server with S256 PKCE, one-use codes, SQL compare-and-swap storage, rotating refresh families, current-permission checks and explicit revocation. Keep human authentication and HTTP admission owned by the host.

## 0.5.3

### Patch Changes

- Expose managed session deadline status for coordinated browser renewal and preserve stable administration cursors on stateless hosts.

## 0.5.2

### Patch Changes

- Add optional request-owned read scopes with bounded lifetime, exact cancellation identity, repeated policy checks, and authoritative final verification before disclosure. Existing authorization APIs retain fresh verification.

## 0.5.1

### Patch Changes

- Add explicit authorization phases that verify current evidence once while retaining policy, subject continuity, expiry and cancellation checks. Existing authorization calls continue to verify independently.

## 0.5.0

### Minor Changes

- e7422c9: Add explicit interactive password/external-entry capabilities, browser-safe data
  descriptors and exact-instance HTTP bindings over the existing managed sessions
  and external session bridge. Keep source verification separate from login and
  validate concrete route/cookie compositions.

## 0.4.0

### Minor Changes

- 61edf6f: Add explicitly authorized session administration with bounded realm-scoped pagination, safe detail DTOs, and revision-checked revocation. Native SQLite, PostgreSQL, and D1 session stores expose the optional administration capability while existing credential-holder session APIs remain compatible. Administrative revocation requires an acknowledged audit intent and reports uncertain effects for reconciliation.

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
