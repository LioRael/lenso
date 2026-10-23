# @lenso/limits

## 0.2.2

### Patch Changes

- d5dc7ab: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/core, @lenso/engine, @lenso/manage, @lenso/mcp.
  Review and publish these dependent packages separately; their versions were not advanced.

## 0.2.1

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0

## 0.2.0

### Minor Changes

- 6dc75f7: Add fixed-window request limits, periodic numeric quotas, and weighted concurrency leases with explicit backend failure policies. Provide process-local memory and atomic SQLite, PostgreSQL, and D1 stores, with optional SQL entry points and explicit migrations.

### Patch Changes

- Updated dependencies [82c9809]
  - @lenso/core@0.2.1
