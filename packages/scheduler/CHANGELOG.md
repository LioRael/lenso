# @lenso/scheduler

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
