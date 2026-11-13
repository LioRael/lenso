# SDK review after Team Desk — 2026-11-13

Scope: the complete local TS/Rust SDK sources, public package contracts and their consumers, using Team Desk's native, Console and durable execution paths as the starting point. The repository still has no commit baseline; this is a whole-source review. The business-flow checks were already green, but did not cover all asynchronous hook, ownership and replacement lifetimes.

## Findings fixed

### P1 — TS async invariant hooks could bypass validation

Location: `typescript/packages/app/src/action.ts:18`, pipeline at lines 96 and 102.

An async method was assignable to the previous void hook signature. The executor discarded its Promise, allowing authorization/effects to start before input validation finished, or reporting success before an output check rejected. This breaks completion ownership and pipeline ordering (TS04).

Both hooks now explicitly return `void | Promise<void>` and are awaited. Input ActionErrors retain their classification; output-check rejection becomes `internal`. Parsers remain synchronous. The failed-first regression holds input validation pending, proves no effects start early, then checks ordered completion and output failure recording.

### P2 — Rust lease loss abandoned handler cleanup

Location: `rust/crates/lenso-sqlite/src/queue.rs`, heartbeat failure branch in `run_one`.

A failed renewal cancelled the token and immediately returned from the execution block, dropping the handler future before it could observe cancellation or finish asynchronous cleanup. TS already waited for the running handler. This violated the cooperative cancellation/owned completion contract (Rust async ownership; TS04).

The Rust branch now signals cancellation, awaits the handler, then returns the original lease error. Stale results still cannot complete or mutate history. Regressions in both languages hold cleanup pending and prove the worker does not return early. An uncooperative handler can delay this return; dropping the outer Rust future or terminating the process still does not guarantee cleanup.

### P2 — An old TS unsubscribe handle removed a replacement

Location: `typescript/packages/app/src/events.ts:45`.

After removing a durable subscription, reinstalling the same id and calling the original removal closure again deleted the new installation. Removal now checks the identity of the installed callback. The failed-first regression proves a stale handle cannot remove its replacement. Rust's explicit `unsubscribe(id)` continues to mean removal of the current named subscription; the language interfaces intentionally differ.

### P2 — Native TS manifest callers shared internal page state

Location: `typescript/packages/app/src/console.ts:88`.

`manifest()` returned the extension's stored page objects/action arrays. Native code adjusting a returned description could alter later manifests while dispatch still used the original bindings. Rust returns clones. The TS manifest helper now returns an owned page/action snapshot, consistent with its module descriptions and Rust behavior (TS02 ownership).

### P2 — TS fanout took a separate timestamp per subscriber

Location: `typescript/packages/sqlite/src/index.ts`, `publishSerialized` and private `enqueueAt`.

A clock that advanced between prepared deliveries assigned different availability times inside one publication; Rust already takes one snapshot. TS now captures the clock once and passes it through a private insertion helper. Existing idempotent jobs retain their original scheduling policy. Both suites check a clock advancing on every read and expect equal timestamps for newly created deliveries.

### P3 — Rust transaction submission repeated serialization and task lookup

Location: `rust/crates/lenso-sqlite/src/queue.rs:298`, consumer in `rust/examples/workflows/src/main.rs`.

The ordinary typed submission interface stopped at a transaction: callers had to serialize the input and repeat the task id. The workflow example did exactly this. `enqueue_on` now takes the actual Task and native input, matching ordinary `enqueue`; low-level prepared payloads use `enqueue_serialized_on`. The example, raw driver test and documentation were migrated. A native transaction test covers typed payload encoding, idempotent submission and rollback.

This changes an **unreleased public signature**: replace old string/payload `enqueue_on` calls with `enqueue_serialized_on`, or pass the Task and native input to the new `enqueue_on`. No compatibility alias was added. Owning submission still serializes before acquiring the connection mutex; no user serializer was moved under that lock.

### P2 — Team Desk imposed ordering on scopes

Locations: TS/Rust Team Desk `store` implementations.

Both examples treated the first scope as the team grant. Adding an unrelated permission before that grant broke otherwise valid native calls, despite SDK scopes being explicit grants without ordering semantics. Team lookup now uses membership. The existing native-call tests prepend an unrelated scope and continue to succeed. This is an application correction, not a new SDK tenant feature.

### P2 — Team Desk normalized copied text differently

Locations: Rust Team Desk `store::trim_text`, project/item creation; common `examples/team-desk/text-fixtures.json`.

JS and Rust native trim behavior differed for BOM and NEL. For example, a BOM-only project name was rejected by TS but accepted by Rust. The Rust application now uses the same whitespace set as the TS parser. Both actual HTTP implementations run common fixtures for project names and item titles, including invalid empty-after-trimming input. This remains application-owned normalization.

## Abstractions retained deliberately

No supported finding required removing an entire SDK layer. Catalog/Module describe native exports without replacing their interfaces; Console bindings reuse the same executor; resource scopes preserve acquisition/use/cleanup ownership; typed and low-level queue paths serve different callers. A universal Application builder, Action/Task supertype, async Observer hierarchy or provider interface with only one implementation would add more concepts than this review's fixes remove.

Schema parsing and invariant hooks are optional alternatives or complements, not mandatory duplicated validation. The normal TS JSON path still uses one parser definition. Rust's synchronous invariant hooks cannot silently accept async implementations; asynchronous authorization remains available. The Rust Observer contract was clarified as synchronous and application-owned in latency, matching the native persistence examples; it is not a transactional audit mechanism.

The HTTP hosts still own identity/body/response handling. Sharing some HTTP glue across examples would not prove a useful transport-neutral SDK interface. Likewise, multiple permission checks and version checks express business rules; hiding them behind a generic CRUD framework would obscure their semantics.

## Evidence and limits

The four initial TS regressions and the Rust cleanup regression failed against the prior implementation. The typed transaction example failed to compile against the old raw-only interface. Native scope-order and Unicode HTTP checks also failed before their fixes. Final executed counts and environments are in [verification](verification.md).

This review strengthens confidence in specific contracts; it does not certify every concurrency interleaving or provider. Browser rendering, deployment operations, additional providers, Rust MSRV and throughput remain unverified. See the [updated five-dimension assessment](team-desk-assessment-2026-11-13.md#revised-judgment).
