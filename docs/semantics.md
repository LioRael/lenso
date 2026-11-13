# SDK semantics

## Native execution and external interfaces

Native action inputs and outputs retain their language types. TypeScript does not parse an internal value through JSON; Rust's native `Action` has no serialization bounds. The Rust `json` feature adds `Executor::call_json`, durable task definitions, durable event bindings and serializable Console bindings. Native database libraries and HTTP frameworks remain application choices.

TypeScript's `defineAction<Resources>()` fixes the resource type once and infers inputs and outputs from definitions. Any synchronous object exposing `parse(unknown)` can serve as a schema; the core SDK has no Zod dependency. Ordinary external JSON input uses the action's input parser without a separate codec. A custom codec is needed only for a distinct wire representation, such as base64 to native bytes.

Declared TS parsers are applied once to input and output at both entrypoints. Their normalized native values are used, so `trim` cannot mean one thing for HTTP and another for native calls. A parser must accept its declared native type. Type-changing wire conversion belongs in the external codec. Without a parser, native values pass through unchanged; neither path introduces an internal serialization round trip.

TS native `validateInput` and `validateOutput` hooks may return a Promise and are awaited before the next pipeline phase. Input hooks preserve structured ActionErrors; rejected output checks become `internal`. Parsers remain synchronous. Rust invariant hooks remain synchronous and cannot accidentally accept an async implementation.

Every completed executor invocation follows this order:

1. Check cancellation before decoding.
2. Decode external input, if applicable.
3. Validate domain input.
4. Await authorization.
5. Check cancellation again before starting the handler.
6. Await the handler and validate its output.
7. Encode external output, if applicable.
8. Record the final success or failure exactly once through the supplied record sink.

Call records contain the operation id, correlation id, subject, monotonic duration and result classification. They exclude input, output, credentials and private error causes. A sink must be infallible: it must not throw, reject or panic. TS awaits an asynchronous sink before completing the call; Rust invokes a synchronous observer. Neither leaves an unowned asynchronous record write behind. The examples report persistence failures to stderr without converting completed business effects into apparent failures. This callback is an observability hook, not a transactional audit log.

Rust panics, process termination and dropped execution futures are not completed calls and do not guarantee a terminal record. Applications needing durable auditing must persist it with their business transaction or outbox.

`ActionError` carries a stable code, a public message and an optional private source. Codes are `invalid_input`, `unauthenticated`, `forbidden`, `not_found`, `conflict`, `cancelled` and `internal`. Domain input rejection must use `invalid_input`; output invariant failure must use `internal`. TypeScript normalizes unexpected thrown values to a safe `internal` error while retaining the original cause. Rust handlers explicitly return structured failures. Transport adapters choose their own status mappings.

## Identity and authorization

Authentication integrations establish `Principal`. Request bodies cannot establish identities, grant scopes or choose administrative access. `requireScope` / `require_scope` require an exact grant; `*` has no implicit meaning. Resource ownership compares the authenticated subject with an application-supplied owner id. Tenant and organization policy remain application-owned.

`allOf` / `all_of` stop at the first failed check. `anyOf` / `any_of` try an alternative only after `forbidden` or `unauthenticated`; dependency failures, cancellation and programming failures propagate. If every alternative denies, `forbidden` takes precedence over `unauthenticated`. TS policies may be asynchronous; Rust supplies lazy `all_of_async` and `any_of_async` for native asynchronous checks as well as synchronous helpers.

Actions must implement authorization explicitly. A public action explicitly allows anonymous execution. Application entrypoints and native callers use the executor rather than invoking a handler directly. The SDK does not protect an application from its own trusted code constructing a privileged context.

TS native `validateInput` and `validateOutput` hooks may return a Promise and are awaited before the next pipeline phase. Output-check rejection is `internal`; input hooks preserve their structured ActionError. Parsers remain synchronous. Rust invariant hooks remain synchronous and cannot accidentally accept an async implementation.

## Cancellation and effects

Cancellation is cooperative. Executors check it before execution; handlers and dependency adapters observe the signal or token during work. The executor does not race a running handler against an abort promise, claim that I/O has stopped, or roll back committed effects. Cancellation arriving after a successful effect does not turn a successful return into a cancellation error. Resource adapters should translate their cancellation errors intentionally.

Action execution provides no automatic transaction across steps, resources, task submission or event publication. Use native database transactions, an outbox or explicit compensation at the actual consistency interface.

## Resources and modules

Contexts carry application-defined native resource objects. Resource definitions validate configuration before acquisition. TS uses `defineResource(...).configure(config)` and `withResources` to open an explicit dependency map and close it in reverse order. Tests replace a binding directly without a global registry. Rust's optional `Resource` trait describes native configuration, validation, acquisition and asynchronous close. `connect` returns its original native value; `with_resource` scopes asynchronous cleanup. Native constructors and ordinary Rust ownership remain usable directly.

Scopes await cleanup after successful use or returned errors. A failed acquisition cleans up its own partial effects. If use and close both fail, TS retains both in an `AggregateError`, and Rust retains both in `ResourceError::UseAndClose`. Process termination, panic or dropping a Rust future cannot guarantee asynchronous cleanup; native Drop and cooperative shutdown remain necessary where appropriate.

Modules group actual native exports and describe installed actions, tasks, events and required resources. A catalog rejects duplicate identifiers atomically and checks resource availability before startup. It is an introspection catalog, not another execution runtime. Rust keeps typed operation bundles in ordinary structs; TypeScript keeps them in `Module.exports`.

## Durable tasks

Durable input and output explicitly support serialization. A producer submits work from an authorized action; workers do not obtain an identity from a serialized caller-controlled `Principal`. Access to job payloads, status and cancellation is an application responsibility.

The SQLite driver implements these states:

```text
queued -> running -> succeeded
                 -> queued      (retry remains)
                 -> failed      (attempt limit reached)
                 -> cancelled   (cooperative cancellation)
queued -> cancelled
```

Attempts start at one and count expired worker leases. Delay and retry use whole Unix milliseconds; sub-millisecond Rust durations and fractional JS millisecond options are rejected rather than silently truncated. Failures retry with a constant configured delay; the initial attempt counts toward `maxAttempts`. Workers claim only their named task kind. All workers for a kind must agree on its configured concurrency limit, enforced against active database leases.

`attempts(id)` returns each attempt's start, completion, outcome and safe error message, including failures preceding a successful retry. Claims, final status and attempt records commit together; stale completions cannot overwrite history. Durations and Unix milliseconds must fit the nonnegative JavaScript safe-integer range, and attempts/concurrency use the positive `u32` range. Unsafe values fail before a write or claim instead of overflowing Rust integers or losing JS precision. TS supplies `jsonCodec` for schema-validated JSON task payloads and results; custom serialization remains explicit.

A worker renews its lease while awaiting a handler. An expired lease permits another worker to retry; an exhausted expired lease becomes failed. A unique lease token fences renewal and completion. A stale worker cannot overwrite a replacement's state. Scheduling, timers, database availability and clocks can cause lease loss: callers must handle a `conflict` result. Lease recovery happens when a worker next attempts a claim for that task kind. If a renewal fails, both workers signal cancellation and await the handler before reporting the lease error; its result cannot overwrite the replacement. A handler that does not settle can delay this return. Native future dropping or process termination can still interrupt Rust cleanup; the worker does not promise forced termination or crash-safe cleanup.

These mechanisms provide at-least-once attempts, not exactly-once effects. A paused or disconnected old worker can still have performed external effects before losing its lease. Use the stable job id as a downstream idempotency key. Long synchronous work must not block the worker's timer; offload CPU work and use asynchronous I/O appropriately.

Cancelling queued work is immediate. Running cancellation is delivered through the worker heartbeat and requires a cooperating handler. A successful handler remains successful even if a cancellation request arrived late. A failed handler with a pending cancellation becomes cancelled rather than retried. There is no forced termination of noncooperating external work.

Submitting the same id, task and payload returns the existing job without changing its scheduling policy. Reusing an id with a different task or serialized payload is a conflict. Deduplication compares serialized bytes; it does not canonicalize JSON. A single enqueue statement can participate in the application's native SQLite transaction. Rust exposes typed `enqueue_on(db, &task, &input, &options, now_ms)` for this purpose; already encoded payloads use `enqueue_serialized_on`. This replaces the previous string-based `enqueue_on` name in this unreleased baseline; TypeScript accepts the caller's native connection. No cross-database transaction is implied.

The driver owns scheduling operations, not an automatic daemon. Applications decide how to poll, run concurrent workers and stop them. Rust's `with_connection` lends its locked native connection: callbacks use the `_on` transaction helpers rather than reentering owning methods on the same queue. SDK task status is protected application data, not a public management endpoint by default. SQLite is the implemented provider; PostgreSQL, Redis, distributed clock coordination, retention and migration across future schema versions are not implemented.

## Events

`Event<T>` is a process-local notification. It awaits listeners in registration order, stops at the first failure, and does not roll back completed listener effects. TypeScript snapshots listeners for each publication. Rust borrows the event during publication and clones its native payload for each listener. Registration changes affect later publications.

Local subscription lifetimes are independent even when the same function subscribes twice.

`DurableEvent<T>` is a distinct event definition. Subscribers register stable ids and map native event values to their own typed task inputs. Producers publish the event value without naming task handlers or constructing delivery ids. `SqliteQueue.publish` persists one independent job per subscription. `publishSerialized` / `publish_serialized` are lower-level driver interfaces for already prepared deliveries.

Publication identifiers combine the event name, event instance id and subscription id. Repeating the same publication and payload deduplicates each subscription. A conflicting payload rolls back every new delivery in that publication. Duplicate subscription ids are rejected. Subscriber completion is independent, retryable and unordered across subscribers; ordinary tasks implement the durable handlers.

Publication uses a savepoint and one clock snapshot for all newly created deliveries. TS publishing participates in the supplied native connection's current transaction; Rust exposes `publish_on` for the caller's native connection or transaction. A failed publication rolls back its new deliveries while preserving the surrounding transaction for its owner to commit or roll back. With no surrounding transaction, releasing the outer savepoint commits the publication. Calling publication after a separate committed domain write does not make those two operations atomic. A TS unsubscribe closure removes only its own installation; calling it again cannot remove a replacement under the same subscription id. Rust explicitly removes the current named subscription with `unsubscribe(id)`. Subscription definitions are explicitly installed application code; distributed discovery and automatic replay for subsequently installed subscribers are not implemented.

## Console

Console page descriptions reference existing action ids. Manifest construction rejects duplicate pages and unknown operations. Applications must authorize the manifest itself and every management action. A frontend cannot acquire authority by describing a page. The headless client validates manifests and error responses received from the network; action results remain `unknown` until the extension validates its own result contract.

Backend `ConsoleExtension` definitions bind existing Actions for lists, details and custom operations. Both the manifest and management dispatch derive from these bindings. They use the ordinary executor, validation, Access checks and record sink; there is no second switch of business handlers. The TS `ConsolePages<View>` registry installs compiled custom page factories in a host, passing its authenticated client and page metadata. A remote manifest never supplies executable frontend code. The registry returns native view values and does not impose a frontend framework or render DOM itself.

TS manifests are owned snapshots of pages and action lists, matching Rust clone semantics. TS page definitions accept Actions directly; `consoleOperation(action, codec)` is reserved for explicit transport conversion. Reusing the same Action in one extension reuses its binding; distinct handlers with the same id conflict. Rust modules may contribute independent extensions, derive their Module descriptions from those bindings, and combine them with `merge`. Merge preserves declaration order and rejects conflicting page or operation ids without partially installing the contribution.

TS extension constructors accept an optional third array of standalone operations; Rust uses `register(action)`. These reuse the same executor but need no artificial page control. Manifest construction validates their ids against the catalog as well. Team Desk uses this for the current-session query and the explicit notification-worker operation. The client transport preserves the global receiver for both default and injected native fetch implementations.

The client checks JSON shape and shared typed manifest invariants. Native metadata failures keep their conflict/not-found classifications; malformed remote descriptions and bodies are `internal` protocol failures. `callParsed` validates a known result through a caller-owned parser and classifies parser failure as `internal`; server ActionErrors remain unchanged. Input parsing is separate. The raw unknown-result call remains available, and neither path retries a write or treats a bad result as proof of rollback. Base URL queries are retained separately from route pathnames. Cancellation is checked before token acquisition, before dispatch and after response reading; arbitrary provider Promises still own their own lifetime.

The manifest format has its own version (`1`), independent of SDK package versions. Console UI rendering and business action execution are separate concerns. This repository includes a Team Desk browser reference host, but not the complete Official Console product, a CLI integration or Official Registry content.
