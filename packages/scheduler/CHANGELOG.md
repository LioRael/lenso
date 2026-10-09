# @lenso/scheduler

## 0.3.0

### Minor Changes

- df127d0: Add an authorized, bounded Manage task-schema catalog.
- df127d0: Add optional bounded Tasks queries with explicit task filters, immutable job-ID pagination, and metadata-only summaries for D1 and PostgreSQL providers. Legacy providers report unsupported queries without changing their existing contracts.

  Add an opt-in Scheduler Manage companion that reuses scheduler authorization and revision operations, accepts trusted actor and AbortSignal context, validates registered task input, and omits schedule input and job results from management reads. Hosts retain ownership of workers and tick drivers.

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
  - @lenso/auth@0.3.1
  - @lenso/tasks@0.3.1

## 0.2.0

### Minor Changes

- 3e0a680: Add a persistent Scheduler for PostgreSQL and D1 that hands stable occurrences to the existing Tasks worker, retry and status APIs. Include explicit finite Workers consumption, scoped queue identity binding, read-only acceptance lookup and recovery without creating replacement jobs.

  Add the Tasks D1 provider and persistent queue identity migration. Custom TaskProvider implementations must implement identity and read-only deduplication lookup; PostgreSQL consumers must explicitly install the optional pg and pg-boss peers. Apply the documented migrations before runtime startup.

### Patch Changes

- Updated dependencies [82c9809]
- Updated dependencies [3e0a680]
  - @lenso/core@0.2.1
  - @lenso/auth@0.3.0
  - @lenso/tasks@0.3.0
