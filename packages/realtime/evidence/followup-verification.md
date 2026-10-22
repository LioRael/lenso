# Realtime permission and reconnect follow-up — 2026-10-22

## Candidate and scope

This follows and preserves Storage commit `90e220505f77bf7e7b43d451835256bfbd27d526` on `sprint/data-files-20261022` in `/Users/leosouthey/Documents/Codex/2026-10-22/task-7/lenso-data`. Its example, before/after logs and verification document remain unchanged.

Changed runtime code is confined to `packages/realtime/src/index.ts`. Added tests are `packages/realtime/test/delivery-boundary.test.ts`; existing Storage Fetch tests were extended for conditional/Range header permission checks. Realtime README describes the delivery boundary. No Auth, Console, Tasks, framework CLI, root lockfile or dependency configuration was edited.

## Reproduced gap and repair

`delivery-before.txt` records two failures against the original Realtime implementation: a provider delivery resolves a waiting reader with a private update; synchronous subject revocation then runs before the reader continuation. The asynchronous iterator still returns the private marker, and SSE encodes it into a new frame after revocation. This is data still held inside the package, not bytes already emitted to a client.

Waiting readers now receive a wake notification only. The private event stays in the bounded queue, where revocation can purge it before the continuation checks expiry and dequeues. SSE also checks current connection/subscription grants immediately before encoding a frame, covering revocation after dequeue but before its awaiting continuation resumes. A connection retains its single-pending-reader reservation until that continuation completes, so another `next()` cannot steal the queued update.

No per-event authorizer/database query, new reconnect protocol or persistent replay layer was added. Queued events now include an event awaiting its reader continuation; existing event/byte limits apply to that buffered event too. Already returned data or emitted bytes cannot be recalled.

## Successful targeted evidence

`delivery-after.txt`: **52 passed, 0 failed, 252 assertions**, across only:

```sh
bun test packages/realtime/test/realtime.test.ts packages/realtime/test/delivery-boundary.test.ts packages/storage/test/fetch.test.ts
```

The new boundary file has **13 passing cases**:

- Both iterator and SSE drop woken private updates when subject/resource permission is revoked, the subscription is cancelled, or lease/session expiry occurs before continuation.
- SSE rechecks between dequeue and frame encoding.
- A second `next()` during a woken pending read is rejected; the first reader retains its event.
- An isolated simulated business-store flow disconnects, misses a revision, reconnects with the old cursor, receives the existing reconnect gap, snapshots the latest revision, then rejects both snapshot access and a new subscription after persisted policy is revoked. No event replay is claimed.

Existing focused Realtime cases also verify bounded queues, provider failure, deferred revocation, snapshot races, renewal, expiry, gaps and resource/scope isolation. No full workspace test run was performed.

File authorization: GET and HEAD were exercised with no condition, `If-None-Match`, `If-Match`, `If-Modified-Since`, `Range`, and `Range` plus `If-Range`. A metadata-visible user denied read receives 403 in every combination, without ETag, Content-Length or Content-Range. There is no divergent HTTP cache/partial-read branch to repair: the current raw Fetch helper does not implement 304/206 handling. Authorized requests retain full 200 responses, private/no-store headers and full content length; HEAD has no body. Low-level object adapters' existing range support was not changed.

Additional successful checks:

- Realtime browser/Bun builds and declarations passed.
- Realtime and Storage package typechecks passed.
- Targeted oxlint with `--deny-warnings`, oxfmt check and `git diff --check` passed.
- Core's already built output was reused. Log/Otel/Web/Workers prerequisite builds passed for adapter validation; their tracked sources were not edited.
- `hosts-after.txt`: **4 adapter checks passed** (oRPC async iterator, portable browser bundle, in-process Workers Fetch adapter, existing Notes bridge), with one real HTTP startup failure described below.

## Exact blocked checks and limits

No denied network action was retried or moved to another port/environment.

1. Action: `bun test packages/realtime/test/hosts.test.ts`, raw Web/real Bun Fetch case. Target: an owned loopback HTTP server, port 0. Startup failed at `Bun.serve` with `EADDRINUSE` before a server was obtained. Four independent cases completed; real HTTP/SSE remains unverified in this environment. The precise underlying reason for the port-0 error was not established.
2. Action: selected `packages/realtime/integration/redis.test.ts` cases (`independent Realtime|metadata expires|server termination`). Target: owned `127.0.0.1:0` listeners used to allocate isolated Redis ports. All three attempts were denied with `EPERM` at `Bun.listen`; no Redis subprocess was launched and no Redis validation passed. Logs are in `redis-selected.txt`. Three empty temporary directories created before the failed binds were identified by their creation times and removed by exact path with `rmdir`.

Those optional checks need an execution environment that permits owned loopback listeners. Their outcomes are failed/denied, not pending or an uncertain deployment. There was no deployment, external service access, credential change, permission change, push, paid action or GUI browser.

Memory/controlled-provider tests and the business-store reconnect exercise are explicitly local simulations. The Workers adapter check is an in-process adapter test. None of these results establish live D1, PostgreSQL, S3, R2, deployed Workers or production Redis behavior. Actual runtime remains Bun 1.4.3 (c6da4a4d3), with Node 26.10.0 for the installed TypeScript tool.
