# @lenso/webhooks

## 0.3.0

### Minor Changes

- f01f0e0: Add an optional bounded, signed inbound receiver and host-owned PostgreSQL inbox
  that atomically couples event-ID receipts with business writes. Provide explicit
  plugin assembly without adding a listener, queue, automatic migration or secret source.

### Patch Changes

- d5dc7ab: Retain dependency updates deferred by scoped preparation of @lenso/cli, @lenso/core, @lenso/engine, @lenso/manage, @lenso/mcp.
  Review and publish these dependent packages separately; their versions were not advanced.
- 88fcd54: Retain dependency updates deferred by scoped preparation of @lenso/auth, @lenso/cli, @lenso/engine, @lenso/manage, @lenso/mcp, @lenso/realtime, @lenso/scheduler, @lenso/storage, @lenso/tasks.
  Review and publish these dependent packages separately; their versions were not advanced.
- 7664344: Accept Engine 0.6 and 0.7 in optional integrations after whole-workspace compatibility verification. Keep these packages pending for a separately reviewed release.
- Updated dependencies [d5dc7ab]
- Updated dependencies [88fcd54]
- Updated dependencies [f86cfae]
- Updated dependencies [7664344]
- Updated dependencies [7664344]
  - @lenso/audit@0.3.5
  - @lenso/limits@0.2.2
  - @lenso/tasks@0.5.0
  - @lenso/manage@0.6.1

## 0.2.2

### Patch Changes

- Updated dependencies [c310eac]
  - @lenso/engine@0.6.0
  - @lenso/manage@0.5.0
  - @lenso/audit@0.3.1

## 0.2.1

### Patch Changes

- Updated dependencies [615b1a4]
- Updated dependencies [df127d0]
- Updated dependencies [df127d0]
  - @lenso/engine@0.5.0
  - @lenso/core@0.3.1
  - @lenso/audit@0.3.0
  - @lenso/tasks@0.4.0
  - @lenso/manage@0.4.1

## 0.2.0

### Minor Changes

- 387030a: Add durable outbound Webhooks with PostgreSQL and Tasks execution, pinned HTTPS destinations, raw-byte signatures, scoped status queries, bounded recovery, and audited replay.

### Patch Changes

- Updated dependencies
  - @lenso/core@0.3.0
  - @lenso/engine@0.4.0
  - @lenso/manage@0.4.0
  - @lenso/audit@0.2.1
  - @lenso/limits@0.2.1
  - @lenso/tasks@0.3.1
