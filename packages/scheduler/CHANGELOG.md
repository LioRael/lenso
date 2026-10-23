# @lenso/scheduler

## 0.4.0

### Minor Changes

- f86cfae: Prevent business execution when worker stop races the first lease check. Expose durable pending-attempt timing and fixed failure codes in bounded task summaries and Scheduler Manage. Preserve authorized occurrence visibility during queue read outages with an explicit job-read state.

### Patch Changes

- d5dc7ab: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/core, @lenso/engine, @lenso/manage, @lenso/mcp.
  Review and publish these dependent packages separately; their versions were not advanced.
- 7664344: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/engine.
  Review and publish these dependent packages separately; their versions were not advanced.
- Updated dependencies [4323bcd]
- Updated dependencies [d5dc7ab]
- Updated dependencies [f86cfae]
- Updated dependencies [7664344]
  - @lenso/auth@0.6.0
  - @lenso/tasks@0.5.0
  - @lenso/manage@0.6.1

## 0.3.4

### Patch Changes

- Abort cooperative sibling task handlers and await cleanup when a worker lane fails. Observe initial Scheduler driver rejections immediately while preserving the original failure for hosts.
- Updated dependencies
- Updated dependencies [c490be2]
- Updated dependencies [29b4e93]
  - @lenso/tasks@0.4.1
  - @lenso/auth@0.5.5
  - @lenso/engine@0.9.0
  - @lenso/manage@0.6.0

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
