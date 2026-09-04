# Construction failure, cancellation, and cleanup across languages

Status: **Proposed authoring/runtime clarification; not implementation approval.**
Date: 2026-09-04.

This companion completes the lifecycle portion of the
[authoring review](2026-09-04-plugin-usage-walkthrough.md) and
[Rust/TS comparison](2026-09-04-multilingual-plugin-authoring.md). It retains
ADRs 0046–0048: strict initial startup, cooperative cancellation, bounded
shutdown, stable bindings, and truthful Execution Adapter isolation.

The worked instance is the same TS sync Plugin calling Rust Store instances.
Its private transform engine needs asynchronous construction and cleanup.
These rules also apply when the sync implementation itself is Rust. No new
Plugin category, lifecycle method pair, or automatic state migration is added.

## Three events that must remain distinct

1. The caller receives a terminal outcome.
2. The underlying work stops accessing instance resources.
3. Those resources are cleaned up or their execution container is terminated.

A deadline may cause the first event before the others. A result, cancellation
signal, transport disconnect, dropped Rust Future, or rejected TS Promise is
not proof that an external write was reversed or that all associated work ended.
The runtime tracks result completion separately from execution termination.
These are internal accounting facts, not extra author-visible Plugin states.

## Construction and ownership

Start construction after required providers are callable, under a finite
Host-selected startup budget. Its optional lifecycle input exposes cancellation
and remaining budget without adding a service locator. Keep external ingress
and recurring work closed until App readiness. In an explicit earlier prepare
phase, existing ADR 0046 restrictions still apply.

Until a factory returns a complete object, it owns partial resource cleanup.
A constructor that acquires two resources must unwind the first if acquiring
the second fails. Resource-library helpers own partial allocations that they
never return. Rust synchronous destructors may release synchronous resources;
async close needs explicit cooperative cleanup. TS needs explicit cleanup as
well; garbage collection is not a reliable release operation.

On successful return, the wrapper takes ownership exactly once. If admission
for this construction attempt is still open, it proceeds to bind and activate
that object. If startup has already been cancelled, the returned object goes
directly to cleanup, never to binding, readiness, or externally callable work.
The factory must not also close an object whose ownership it returned.

On factory failure before return, there is no complete instance on which to
invoke `stop`. Record construction failure as the primary cause, with partial
cleanup errors as additional diagnostics. Do not replace the original failure
with a misleading cleanup-only result.

On constructor timeout, close admission for the attempt and signal cooperative
cancellation. Keep observing its execution under the remaining cleanup budget;
do not discard a TS Promise merely because a timeout Promise won a race. A late
successful return must still be owned and cleaned up if the execution container
is alive and safe cleanup time remains. A late rejection must be observed and
cannot alter the startup outcome. Neither event starts an automatic retry.

If the factory never yields or returns, normal async cleanup is not guaranteed.
At the cleanup boundary, apply the actual Adapter/Host escalation described
below. Do not start a replacement that conflicts with resources the abandoned
attempt may still own. This also applies to two successive constructor attempts,
not only to already-active instances.

## Invocation cancellation

An operation receives an inherited deadline and cancellation scope. A nested
Store call cannot extend that deadline, erase caller cancellation, or create
a new full timeout at every hop. Adapters translate remaining time between
clock domains; they do not treat another process's monotonic timestamp as local.
Explicitly detached durable work needs a domain contract, not a hidden retry or
unscoped task spawned from a cancelled call.

The runtime chooses one terminal result for a request. A result accepted before
the applicable terminal condition is final. Once cancellation/deadline has been
accepted, late success cannot replace it or produce a second response. The
executor checks the local deadline before accepting completion; transport and
scheduling delays do not establish a claim about when a remote side effect ran.
Exact simultaneous-event ordering follows the existing serialized runtime path
and must be covered by the implementation specification, not per-language rules.

Cancellation alone does not mark an otherwise healthy instance failed or cause
it to restart. Its business method may finish promptly, await an interruptible
library operation, or fail to cooperate. Each generated client propagates the
scope and rejects new nested calls from an already cancelled invocation.

A client response completing does not free an executor's real capacity slot
while underlying work still occupies it. Keep such work bounded and accounted
for until it settles or the Adapter actually stops it. Otherwise repeated
timeouts could create unlimited unfinished work behind a nominal concurrency
limit. Ordinary further admission follows the existing bounded queue policy.
No new unbounded cleanup or retired-Promise queue is introduced.

The Agent helper maps the structured ToolProvider result as its contract
requires. It must not reinterpret cancellation as `updated`, or assume a lost
Store response means the write did not occur. Product recovery, idempotency,
conflict resolution, and compensation remain with their existing owners.

## Disablement, failed startup, and replacement cleanup

Use one coordinated cleanup sequence with a shared absolute deadline:

1. Close new external admission and future triggers. Close affected instance
   admission as it enters cleanup; cancel instance-owned background work.
2. Allow admitted invocations to drain only within the Host's bounded drain
   allowance, then signal cancellation. Calls depending on still-active providers
   can finish while those providers remain available in reverse cleanup order.
3. Establish that managed work no longer accesses the instance's resources.
   Merely delivering a cancellation signal is insufficient.
4. If that safety condition holds and budget remains, invoke the complete
   object's optional `stop` hook at most once. Release SDK-owned registrations
   and remaining managed resources in their defined order.
5. At the shared deadline, record unfinished work/cleanup and apply Adapter/Host
   escalation. Do not start a fresh timeout for every task, resource, or hook.

The Host must allocate drain time within the overall cleanup budget; draining
cannot silently reset or extend it. Stop receives the remaining cleanup budget,
not the already-cancelled business call's token. Cleanup cannot open ingress,
launch recurring work, acquire a longer lifetime, or revive an old invocation.
Existing ordinary dependency calls used for cleanup remain bounded and may fail
because that provider is already unavailable; no special bypass is implied.

If work still accesses a resource, the default is not to call a general-purpose
`stop` concurrently and close that resource beneath it. Skip unsafe hook
execution, report the reason, and escalate. A resource/library with an explicit
safe concurrent shutdown contract may be stopped through that contract; do not
infer this property for arbitrary Plugin state.

An instance's stop hook is an at-most-once attempt, not a guarantee of invocation,
successful completion, or eventual durable writes. If a hook errors, record it
and continue independent safe cleanup within the remaining budget. Do not retry
the hook automatically or mark the instance ready again. Required durable work
cannot rely solely on shutdown hooks.

The runtime can account for managed work and Adapter-reported execution only.
Authors using raw threads, timers, detached promises, or library workers remain
responsible for proving those workers no longer access resources they close.
An empty managed scope is not proof that arbitrary untracked work has stopped.

## Language mechanisms and physical limits

| Mechanism | What it does and does not establish |
| --- | --- |
| Rust cancellation token | Signals cooperating code; it cannot interrupt arbitrary synchronous or blocking native work. |
| Dropping a Rust Future | Stops polling that Future and runs owned synchronous destructors. It does not await async close, cancel every spawned task, or reverse external effects. |
| TS cancellation signal | Notifies cooperating handlers/libraries. The signal does not terminate their Promise, and a library must actually use it. |
| TS `finally` | Runs when control leaves its block. An unresolved await or terminated process may prevent it running; `Promise.race` does not force the losing branch to exit. |
| Isolated Process termination | Can stop code in that process after confirmation of termination; it does not undo remote writes or guarantee stopping independently spawned external workers. A shared process affects all instances inside it. |
| Wasm interruption/disposal | Only the selected Adapter's actual interruption and host-call behavior can establish stopped execution; Wasm alone does not guarantee prompt cancellation of host work. |
| Native instance cleanup | Cannot safely promise to kill an arbitrary thread or unload code still executing. When safe isolation/reuse cannot be established, fail the containing runtime and use Host/Runner escalation rather than silently overlap replacements. |

Deadline detection and cleanup scheduling require a responsive executor. A
blocked JS event loop or non-yielding native lane can prevent in-process timers
from firing. A hard process exit bound needs a control path outside that blocked
executor. Report unsupported guarantees honestly; do not claim that adding an
async timeout creates preemption or a real-time shutdown guarantee.

Closing admission prevents new managed calls but is not fencing against native
library access or an external worker. Before starting an exclusive replacement,
confirm release/termination at the real resource owner; distributed resources
may require their own fencing or lease contract. Generation identity alone is
not an external lock. Do not add automatic distributed locking to Kernel.

## Review cases and adoption

| Case | Required outcome |
| --- | --- |
| Second resource fails during construction | Factory cleans up what it owns; initial startup fails; no stop hook on a nonexistent object. |
| Factory returns just after startup cancellation | Never activate it; wrapper takes ownership and attempts safe cleanup within the existing budget. |
| Another selected Plugin fails after this one constructed | No App readiness; this complete object participates in reverse startup cleanup. |
| Store commits but the sync call times out | Caller sees the terminal failure; the write may exist; no automatic replay. |
| TS handler ignores cancellation and remains pending | Response may terminate through an available caller/transport path; executor capacity remains occupied and bounded; resource cleanup cannot race it. |
| Rust Future is dropped during async resource cleanup | Do not report async cleanup as completed; retain uncertainty and use actual Adapter escalation. |
| Stop hook errors or exceeds the remaining deadline | At most one attempt; record failure, continue independent safe cleanup if possible, and do not extend shutdown. |
| Old constructor or writer may still own an exclusive resource | Reject/defer conflicting replacement; never infer release from a cancelled caller response. |
| Native code blocks the only executor | No false prompt-cancellation claim; the outer Host must provide whatever hard termination policy it requires. |

The inspected Bun provider loop awaits `invokeRequest`, then checks cancellation
flags; its context exposes a cancellation getter. That is not evidence that a
Promise was interrupted or that an AbortSignal authoring interface already
exists. Rust already exposes managed task scopes. These are useful foundations,
not proof of the complete ownership and late-completion rules proposed here.

Keep current wire result categories and accepted lifecycle contracts. SDK owners
map language mechanisms; Adapters report execution termination; Driver/Runner
owners enforce available timing/escalation mechanisms; core owns portable
admission/outcome/cleanup accounting. A release must describe exact supported
behavior, including gaps, before claiming this authoring contract. Semantic
changes to accepted shutdown behavior require explicit review rather than an
unannounced SDK-only change. No runtime tests have been run for this design.
