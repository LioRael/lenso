# Authorized task status and recovery

Use the [Tasks guide](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/tasks/README.md) for durable state/attempt/cancellation semantics. The [Tasks operation declarations](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/tasks/lenso.config.ts) and [authorized service](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/tasks/src/authorized-service.ts) demonstrate actual status/recovery exposure, not universal operation names.

## Query first

Find and inspect the app's existing authorized status entry, then query it before considering recovery. Confirm job state/attempt budget/cancellation flag, durable owner, queue identity and current worker/attempt evidence. A job ID alone is not authority. The example query returns state, attempt, maxAttempts and cancelRequested, **not worker health**; use existing authorized worker/supervisor logs for that evidence. Report unavailable fields rather than inventing a status API or querying raw tables.

Identify the business idempotency key, already-written business result and external side effects through authorized entries. Queue delivery is at-least-once; a worker can write then die before acknowledgement. Deduplication and fenced acknowledgement do not guarantee exactly-once business effects.

## Decide from observed state

- Pending/running: inspect scheduling, worker availability, attempt/claim and safe logs. `cancelRequested: true` does not mean stopped.
- Final failed: retry is eligible only through the existing service's rules and with explicit authorization. Confirm idempotency and effects first. A manual retry adds an attempt without resetting the counter; automatic retry policy is separate. Do not retry successful, running, pending or cancelled jobs, or perform bulk retry.
- Cancelled/terminal: reconcile business effects; terminal status cannot prove an abandoned process or lease-losing attempt has physically stopped.
- Missing/null: retention pruning can remove status/results. It is not proof of never running; dedup mappings may survive. Do not resubmit with a new key to evade uncertainty.

For an explicitly authorized cancellation, use the ownership-enforcing entry and query afterward: `cancelled` prevents a pending claim; `requested` asks a running handler to stop cooperatively; `terminal` and `missing` do not undo effects. Timeout, claim loss and worker abort are not durable user cancellation, and an abort-ignoring handler can retain its slot/delay shutdown.

For authorized retry, make one eligible recovery attempt, query afterward and check the business result. On connection loss, ambiguous acknowledgement or continued failure, query/report and stop, not blind replay. Worker/supervisor restart needs its own authorization; missing/mismatched DB state is a blocker to report, not permission to provision or migrate.
