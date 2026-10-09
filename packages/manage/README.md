# Optional management operations

`@lenso/manage` binds a deliberately chosen set of existing Engine Operations to
one exact plugin instance. It is optional: core plugins need no manage declaration.
Methods can live on the original service or an ordinary sidecar plugin with
explicit `requires`. Setup returns the real service; descriptors never carry handlers.

```ts
import { defineManage, selectManageOperations, describeManage } from "@lenso/manage";

const manage = defineManage({
  plugin,
  operations: [readOperation, removeOperation], // existing defineOperation declarations
  views: [{ key: "notes/detail", title: "Notes", detail: "read", action: "remove" }],
});
export const operations = selectManageOperations(manage, ["read", "remove"]); // CLI
export const mcpOperations = selectManageOperations(manage, ["read"]); // independent MCP choice
const descriptor = describeManage(manage); // schemaVersion: 1, plain JSON, no setup
```

Application code supplies the existing `plugin` and Operations above. See the real
[Notes factories](../../examples/notes/src/operations.ts) and
[Tasks factory](../../examples/tasks/src/plugin.ts). Selecting declarations does
not authenticate a caller, grant permission or open an entry automatically.
Named CLI exports and ordinary single-input `defineOperation` calls remain supported.

## Running entries

`createManageAdapter({running, plugins, operations, binding, canList})` borrows an
explicit `OperationRuntime` from `@lenso/engine/operations`, the
`instanceId`/`get`/`logger` subset of `RunningApp`. An existing `PluginContext`
also satisfies this contract and retains its exact declared dependency checks,
without acquiring app lifecycle ownership. The adapter verifies selected exact
instances and never starts or stops a service graph.
`plugins` is the management selection, not a dependency-closed app assembly:
Auth/DB dependencies need not be selected or exposed. Every selected plugin must
be an exact accessible runtime instance, even when it has no selected operations;
invalid declaration metadata and duplicate IDs are refused.
`binding(operation, validatedInput)` is mandatory and runs
anew for each call; it supplies trusted context and optional confirmation/approval
callbacks, never business handlers. `canList(operation)` must check the current
entry identity's operation-level permission. It filters catalog and gates invocation;
the real service still enforces realm/audience, object, owner and tenant policies.

### Selection lifetime and replacement

For retained tools or routers, prepare the selection once and register its close
with the host's existing cleanup:

```ts
import { createManageSelection, createManageAdapter } from "@lenso/manage";
import { createManageRouter } from "@lenso/manage/orpc";

const selection = createManageSelection({ running, plugins, operations });
onCleanup(() => selection.close());
const adapter = createManageAdapter({ selection, binding, canList });
const router = createManageRouter({
  selection,
  evidence: extractRequestEvidence,
  binding: bindRequestOperation,
  canList: listForRequestEvidence,
});
```

Preparation validates the complete selection and exact accessible instances once.
It copies and freezes operation declarations and source metadata, retaining the
exact plugin, schema and function identities. Original array/declaration edits
cannot add operations or retarget existing handles. Schemas, plugins and trusted
callbacks are application code, not deep-copied or globally frozen; changes to
that code require a new selection. Local key and plugin/method indexes serve
invocations; no process-wide selection or credential cache is used.

There is no in-place update or inferred revocation from edits to the original
config arrays. To replace a config, explicitly close the old selection and
prepare a new one from the new declarations. `close()` is synchronous and
idempotent: it revokes future catalog access and dispatch from every attached
adapter, router and retained agent handle, and clears the selection's indexes,
declarations and borrowed runtime reference. Keys have a selection-local nonce,
so an old key cannot address a replacement selection. Closing never stops the
borrowed app; stopping the app is not a substitute for closing a retained
selection. Host policies/callbacks and active requests own any references they
retain themselves.

Each request extracts current trusted evidence and creates only a request-bound
adapter over the prepared selection. Neither evidence, actors, catalog admission
nor execution bindings are cached. Input is validated once and binding runs once
per invocation. Target validity is checked after asynchronous admission, input
validation and binding, then validity and visibility are checked again immediately
before dispatch, after confirmation/approval waits. `canList` is a visibility
gate, not execution authority: the binding and real service still authenticate
and authorize execution, including current object/tenant policies.

Closing during any pre-dispatch wait prevents execution. An already dispatched
service call is not cancelled, retried or rolled back by closing the selection;
its request may retain the runtime until the call settles. Close revokes dispatch,
not effects that have already occurred.

The existing `{running, plugins, operations, ...policies}` options remain
supported: an adapter prepares a private selection, and a router prepares one
selection for its lifetime, not one per request. Use an explicit selection when
the host needs to revoke retained handles or release their runtime references.

Bindings may supply an actual `signal`. Engine checks it before gates, after gate
waits, before dispatch and after service completion, retaining the abort reason
in the in-process cause chain. An aborted gate wait prevents dispatch; cancellation
after dispatch does not roll back mutations or stop the service. The signal is not
injected into business methods: cooperative service cancellation still needs an
explicit host-bound context. Cancellation metadata remains descriptive.

For methods declared with `context: true`, the binding context type is inferred
from the actual second argument. `bindManageOperation(operation, {context})` also
checks individual bindings when an entry selects heterogeneous service contexts.
Prefer evidence or an actor produced by current Auth, not an identity copied from
input. Never use a shared mutable current actor/credential or global Context.

- `catalog()` returns safe versioned entries, including schema availability and
  a selection-scoped opaque `key`. Display identifiers may be redacted; dispatch
  uses `invokeEntry(key, input)`, not redacted display strings. Keys are local to
  the configured selection, not durable IDs or receipts across deployments.
- `invoke(pluginId, method, input)` remains available for trusted in-process callers.
- `createAgentTools(adapter)` (also `@lenso/manage/agent`) returns SDK-independent
  descriptions and `invoke` functions only for the caller-filtered catalog.
  It requires a convertible object input schema and rechecks admission at invocation.
- `@lenso/manage/orpc` exports `createManageRouter`. Supply the running app,
  selected Operations, `evidence(context)` using current Auth Fetch extractors,
  `binding(operation, input, evidence)` and `canList(operation, evidence)`.
  Its `catalog` and `invoke` procedures are not mounted automatically.
  Invoke accepts `{key, input}` or `{pluginId, method, input}`; neither accepts an
  actor/approval envelope field. Evidence is obtained from each current request.
  The optional peer is exactly oRPC `2.0.0-beta.42`.

The common Engine path validates raw Standard Schema input once, preserves the
own service method's `this`, retains existing operation telemetry, and handles
opaque unknown errors plus finite JSON output. Default success/catalog output
budget is 1 MiB (`maxOutputBytes`); oRPC errors use fixed bounded messages and
typed safe data declared through synchronous `.errors` schemas. Invalid input
is `BAD_REQUEST`/400; missing authentication or reauthentication is
`UNAUTHORIZED`/401; domain permission denial is `FORBIDDEN`/403. Unknown plugin,
unknown operation and caller-hidden operation all return the same `NOT_FOUND`/404
message/data, omitting requested identifiers. Known mapped resource-not-found,
conflict and size failures use 404, 409 and 413 respectively. Unexpected,
unknown, aggregate and cleanup failures are opaque `MANAGE_FAILED`/500.
Explicitly classified Auth/queue unavailability is `SERVICE_UNAVAILABLE`/503,
Storage provider failure is `BAD_GATEWAY`/502, unsupported operations are
`NOT_IMPLEMENTED`/501, and request cancellation is `CLIENT_CLOSED_REQUEST`/499.
Required confirmation/approval refusal is `FORBIDDEN`/403. Cancellation does not
prove that effects were rolled back; none of these mappings grants safe retries.
Original causes remain internal. Applications declare `Operation.mapError`
using package `instanceof` projectors for service/binding domain errors; only the
optional `./orpc` entry imports Auth. Byte budgets do not impose service memory/CPU quotas.
Missing JSON Schema converters remain runtime-only; agent/MCP tools reject them.
No output schema is guessed from input.

## Boundaries

`confirmation: "required"` needs a trusted confirmation flow, and
`approval: "required"` needs an explicit approval-owner callback. Missing/false
callbacks fail closed. Input flags and destructive metadata cannot satisfy these
gates or replace business authorization. A callback returning true without real
verification is an application bug, not an approval implementation.

This is finite query/command invocation, not a control platform: no retry,
scheduling, rollback, durable journal, receipt recovery or persistent audit.
`retry: "safe"` is descriptive; business keys/transactions own idempotency.
Tasks submit/query/cancel/retry remain ordinary, separately authorized operations.
Submitting returns a job ID; request cancellation is not durable-job cancellation.
Streams, including plain AsyncIterables, are rejected as JSON results.

View hints only describe presentation (`key`, title/group/order, columns, detail/action
references). A future Console adapter registers components for stable view keys;
there is no React, component function, module URL or Devframe runtime here.
Namespaced extensions must be plain JSON and cannot affect dispatch or Auth.
Schema defaults/examples and sensitive metadata/results use existing redaction;
redaction is best effort, not a sandbox or protection for secrets under arbitrary keys.
There is no implicit configuration read-all/write-all, hot update or restart.
