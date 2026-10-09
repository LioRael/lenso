# Notes projection adapter

`notes.ts` is a workspace-runnable example wrapping the unchanged `NotesService`
from `examples/notes/src/notes.ts`. Host adapters should import their own Notes
business types rather than depend on this workspace example.
It keeps the exact `create`, `list`, `read`, `update` and `remove` signatures.
It does not expose a new CLI, MCP or HTTP operation. Its separate `query` accepts
an Auth-produced Notes **list** actor and search input, never an owner or tenant.
Pass the existing `notesAudiences` object as `audiences`.

Construct `createNotesSearchAdapter(host)` with the real Notes service,
`NotesAuthentication`, Search service and these trusted host callbacks:

- `namespace`: fixed application namespace.
- `tenantForOwner(ownerId)` (optional): resolve current membership in host code.
  Notes itself has only ownership, not tenant isolation; this callback does not
  add tenant policy to its business CRUD.
- `readLatest({scope, id})`: one database lookup restricted by namespace,
  tenant (if used), owner **and** ID. Return current note content or `null`.
  Do not implement this by listing Notes or by an unscoped ID lookup.
- `repairTasks.recordRepair(target)` (optional): use the host's **existing Tasks ownership records**
  to durably store the authorized scope/ID and return an
  opaque ticket of 1 to 256 characters. Do not store a note/body snapshot.
- `repairTasks.authorizeRepair(ticket)`: resolve the host's stored ticket and recheck its
  current authority on **every** invocation, including retries. Reject unknown,
  revoked or expired tickets. Resolve current permitted tenant scope here, not
  from task payload or old membership. Keep access to this callback trusted.

CRUD first commits through Notes, then rereads current scoped state to upsert
the projection, or delete it if absent. A failed projection throws
`NotesProjectionPending`, with `businessCommitted: true`, `projectionPending:
true`, the committed `documentId` (or `removed` boolean), and a `repairTicket`. **Do not replay
the business operation**: create would create another note. If ticket recording
also fails, `repairTicket` is undefined and `repairRecordingFailed` is true;
the host must arrange explicit recovery. Raw driver errors and note text are
not retained on the public error.

## Explicit bounded repair or Tasks integration

`adapter.repair(actor, id)` is the default bounded repair: it derives current
trusted read scope, validates the ID, and rereads current state for exactly that
ID. It needs no ticket store. Wrong-owner and revoked actors cannot repair.

When `repairTasks` is configured, `adapter.repairTicket(ticket)` is a single
authorized ID repair, never a batch scan. It reauthorizes, rereads current state
and performs an idempotent scoped upsert/delete.

For durable retries, create `const repairTask = adapter.createRepairTask()`,
register that exact task object in the existing `@lenso/tasks` plugin, and
enqueue it using the Tasks service:

```ts
await tasks.enqueue(
  repairTask,
  { ticket: pending.repairTicket },
  { deduplicationKey: pending.repairTicket },
);
```

Only enqueue after narrowing to `NotesProjectionPending` with a defined ticket.
The strict task schema accepts **only** `{ticket}`. The task uses `defineTask`,
three attempts and bounded backoff (5 seconds initially, at most 60 seconds).
Every handler call reauthorizes the ticket; no actor, owner, tenant, ID or note
text in the payload grants authority. The adapter does not start a worker or
enqueue automatically. The host owns registration, enqueue failures, ticket
retention and bounded worker execution (`maxJobs`, timeout, concurrency).
Use a distinct ticket for each new pending write: Tasks deduplication keys
remain tombstoned even after a job succeeds, so one permanent per-note key
would suppress later repairs.

## Consistency limits

No new ticket DB, outbox, identity or queue is supplied or required by Search:
optional durable Tasks wiring requires host-provided existing durable ownership
records; Tasks core does not supply an owner store.
There is no transaction spanning Notes, Search and these records. A crash
between the business commit and ticket recording can leave an unrecorded
projection gap. The adapter has no version/CAS guarantee: overlapping reads
and writes can install stale projections even though each operation reads
latest state when it starts. A host requiring convergence must serialize
**all writes and repairs per ID** across its workers/processes, or arrange
periodic explicit authorized per-ID repair. Tenant movement also requires
host-controlled cleanup of the old tenant projection; rereading in a newly
authorized tenant does not remove the old scope automatically. No atomicity
or exactly-once claim is made.

Runtime imports are `@lenso/tasks` (`defineTask`) and `zod` (strict Standard
Schema input). Notes/Auth/Search contracts are type imports; the host supplies
the installed Notes service and audience objects. Tests additionally use
`@lenso/auth` and the real Notes service. Build framework dependencies before
running `bun test packages/search/test/notes.test.ts`, since package exports
resolve to `dist`.
