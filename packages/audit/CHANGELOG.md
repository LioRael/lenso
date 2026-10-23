# @lenso/audit

## 0.3.5

### Patch Changes

- d5dc7ab: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/core, @lenso/engine, @lenso/manage, @lenso/mcp.
  Review and publish these dependent packages separately; their versions were not advanced.
- 88fcd54: Retain dependency updates deferred by scoped preparation of @lenso/auth, @lenso/cli, @lenso/engine, @lenso/manage, @lenso/mcp, @lenso/realtime, @lenso/scheduler, @lenso/storage, @lenso/tasks.
  Review and publish these dependent packages separately; their versions were not advanced.
- 7664344: Accept Engine 0.6 and 0.7 in optional integrations after whole-workspace compatibility verification. Keep these packages pending for a separately reviewed release.
- Updated dependencies [4323bcd]
- Updated dependencies [d5dc7ab]
- Updated dependencies [f86cfae]
- Updated dependencies [7664344]
  - @lenso/auth@0.6.0
  - @lenso/tasks@0.5.0
  - @lenso/manage@0.6.1

## 0.3.4

### Patch Changes

- Accept the verified Core 0.4, Engine 0.9 and Manage 0.6 cohort in optional peer dependencies while retaining older supported ranges. Package implementation payloads are unchanged; this corrects strict npm dependency resolution for Console consumers.
- Updated dependencies
  - @lenso/auth@0.5.6

## 0.3.3

### Patch Changes

- Updated dependencies [e7422c9]
  - @lenso/auth@0.5.0
  - @lenso/manage@0.5.2

## 0.3.2

### Patch Changes

- Updated dependencies [61edf6f]
  - @lenso/auth@0.4.0
  - @lenso/manage@0.5.1

## 0.3.1

### Patch Changes

- Updated dependencies [c310eac]
  - @lenso/engine@0.6.0
  - @lenso/manage@0.5.0

## 0.3.0

### Minor Changes

- df127d0: Add an optional scoped Audit Manage companion with query and authorized detail reads. Trusted entries bind the scope and principal outside business input, while the Audit service rechecks authority for every read.

### Patch Changes

- Updated dependencies [615b1a4]
- Updated dependencies [df127d0]
  - @lenso/engine@0.5.0
  - @lenso/core@0.3.1
  - @lenso/tasks@0.4.0
  - @lenso/manage@0.4.1

## 0.2.1

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0
  - @lenso/engine@0.4.0
  - @lenso/manage@0.4.0
  - @lenso/auth@0.3.1
  - @lenso/tasks@0.3.1

## 0.2.0

### Minor Changes

- 71ab23f: Add an optional audit service with trusted actors, exact-scope queries,
  append-only corrections, Drizzle persistence, strict intent admission,
  safe best-effort diagnostics, and explicit Manage and Tasks integrations.

### Patch Changes

- Updated dependencies [82c9809]
- Updated dependencies [3e0a680]
  - @lenso/core@0.2.1
  - @lenso/engine@0.3.0
  - @lenso/manage@0.3.0
  - @lenso/auth@0.3.0
  - @lenso/tasks@0.3.0
