# @lenso/realtime

## 0.2.2

### Patch Changes

- d5dc7ab: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/core, @lenso/engine, @lenso/manage, @lenso/mcp.
  Review and publish these dependent packages separately; their versions were not advanced.

## 0.2.1

### Patch Changes

- Apply download authorization to HEAD and release its unread stream. Recheck Realtime grants across iterator and SSE delivery boundaries before disclosing buffered events.

## 0.2.0

### Minor Changes

- 93c5a37: Add resource-authorized SSE subscriptions with bounded authorization leases, buffers and lifecycle cleanup. Provide independent memory and standalone Redis Pub/Sub providers, snapshot/gap consistency semantics, and thin Notes and Tasks adapters.

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0
