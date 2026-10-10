# @lenso/audit

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
