---
"@lenso/tasks": minor
"@lenso/scheduler": minor
---

Add optional bounded Tasks queries with explicit task filters, immutable job-ID pagination, and metadata-only summaries for D1 and PostgreSQL providers. Legacy providers report unsupported queries without changing their existing contracts.

Add an opt-in Scheduler Manage companion that reuses scheduler authorization and revision operations, accepts trusted actor and AbortSignal context, validates registered task input, and omits schedule input and job results from management reads. Hosts retain ownership of workers and tick drivers.
