# Remote Module Runtime

This note defines the first runtime-execution boundary for out-of-process
modules. It is a design checkpoint, not an implementation record.

Remote modules already support manifests, schema-admin reads, admin surface
metadata, and host-owned HTTP proxying. Runtime behavior is separate: the host
must continue to own durable queues, retry policy, Runtime Story semantics, and
operator visibility. A remote module may execute work, but it must not become a
parallel runtime.

## Goals

- Allow a configured remote module to provide runtime function implementations.
- Keep `runtime.function_runs` as the durable source of truth.
- Keep the worker as the only component that claims, retries, and completes
  function runs.
- Reuse existing Runtime Console stories, timelines, retries, execution logs,
  payloads, and Technical Operations.
- Preserve the `ModuleManifest` data / `ModuleBinding` behavior split.

## Non-goals

- Remote modules directly polling or claiming `runtime.function_runs`.
- Remote modules directly consuming `platform.outbox`.
- Remote scheduling, queues, flows, or trigger ownership.
- Streaming function output or long-lived bidirectional channels.
- Browser credentials, host bearer-token forwarding, or arbitrary host bridges.
- Marketplace install trust, signatures, provenance, or compatibility policy.
- Wasm execution or JavaScript bundle execution.

## First Slice

The first slice should support host-invoked remote functions only.

The host remains responsible for:

- loading remote function declarations from the module manifest;
- registering proxy-backed function handlers in `FunctionRegistry`;
- claiming pending rows from `runtime.function_runs`;
- constructing `ExecutionContext`;
- enforcing timeout, retry, and body-size policy;
- mapping remote success/failure to existing function-run statuses;
- writing execution logs and Runtime Story data through the existing runtime
  path.

The remote module is only an executor. It receives one function invocation from
the host and returns one result envelope.

## Manifest Shape

Function declarations are pure manifest data. `ModuleManifest::runtime` uses a
`RuntimeSurface` wrapper to describe functions without embedding handlers:

```json
{
  "runtime": {
    "functions": [
      {
        "name": "remote_crm.sync_contact.v1",
        "version": 1,
        "queue": "remote-crm",
        "input_schema": "remote_crm.sync_contact.v1",
        "retry_policy": {
          "max_attempts": 3,
          "initial_delay_ms": 1000
        }
      }
    ]
  }
}
```

Open questions for implementation:

- whether `input_schema` references committed host contracts, remote-provided
  schema fragments, or both;
- whether queue names are module-local by default and rewritten by the host when
  proxy-backed registration is added.

Do not put executable behavior or transport clients into `ModuleManifest`.

## Protocol Sketch

The first protocol should be request/response JSON over the existing remote
module base URL:

```text
POST /lenso/module/v1/runtime/functions/{function_name}/invoke
```

Request:

```json
{
  "function_run_id": "fnrun_01",
  "function_name": "remote_crm.sync_contact.v1",
  "attempt": 1,
  "correlation_id": "corr_01",
  "causation_id": "httpreq_01",
  "actor": { "kind": "service", "id": "worker", "scopes": [] },
  "trace": {
    "trace_id": "00000000000000000000000000000001",
    "span_id": "0000000000000001"
  },
  "input": {}
}
```

Success response:

```json
{
  "output": {}
}
```

Failure response should use the existing remote error envelope:

```json
{
  "error": {
    "code": "external_dependency_failure",
    "message": "remote CRM was unavailable",
    "retryable": true,
    "details": []
  }
}
```

The host maps retryable failures through the existing runtime retry machinery.
The remote module may suggest retryability, but the host applies the registered
retry policy and maximum attempts.

## Runtime Story Semantics

Remote function execution should not create a new product surface. It should
look like any other runtime function run:

- `runtime.function_runs` row is the story node source.
- Timeline item type remains `function_run`.
- Execution logs are written by the host before/after the remote invocation.
- Technical Operations may include a `source = "remote_runtime"` operation for
  the outbound invocation, but the business Story node remains the function run.
- Retry and dead-letter behavior use the same Runtime Console flows as linked
  functions.

The host should attach compact remote invocation metadata to execution logs or
Technical Operations, not invent a second remote-function history page in the
first slice.

## Auth And Transport

The host must not forward the caller's bearer token. If the remote source is
configured with a host-to-remote token, the host uses that token when invoking
remote functions, matching the HTTP proxy boundary.

Forward only operational context needed for execution:

- request id generated by the worker for this invocation;
- correlation id;
- causation id;
- trace context;
- actor context after host-side validation.

Function invocation request and response bodies must have explicit size limits.
Timeouts should use the remote module source timeout unless a narrower runtime
function timeout is configured.

## Event Handlers

Remote event handlers use the same host-owned outbox dispatch model as linked
handlers. A remote module may declare event subscriptions, but it never claims
or consumes `platform.outbox` rows directly.

Manifest declarations are pure data:

```json
{
  "events": {
    "handlers": [
      {
        "name": "sync_contact_on_user_registered",
        "event_name": "identity.user_registered.v1"
      }
    ]
  }
}
```

The worker loads configured remote modules through `app-bootstrap`, registers
proxy-backed handlers in the shared `EventHandlerRegistry`, then dispatches
claimed outbox rows through the existing relay. Success marks the row
`published`; retryable remote failures use the existing `failed` retry path and
eventually become `dead` after `max_attempts`.

The remote protocol is request/response JSON over the module base URL:

```text
POST /lenso/module/v1/events/handlers/{handler_name}/invoke
```

The request includes the host-owned outbox event id, event name/version,
source module, aggregate identity, correlation/causation ids, actor, trace,
payload, and original event headers. The host may authenticate with the
configured host-to-remote token, but must not forward caller bearer tokens or
cookies.

Success may return a JSON body or `204 No Content`. Empty success performs no
follow-up action. JSON success may include a bounded declarative result action:

```json
{
  "actions": [
    {
      "type": "enqueue_function",
      "function_name": "remote_crm.sync_contact.v1",
      "input": { "contact_id": "usr_1" }
    }
  ]
}
```

The first result-action slice intentionally supports at most one
`enqueue_function` action. The host only accepts functions declared by the same
remote module and already registered in the host `FunctionRegistry`; it uses the
registered retry policy when inserting `runtime.function_runs`. The remote
handler cannot set host retry policy, write runtime tables, emit events, invoke
admin actions, or call arbitrary host bridges.

Failure uses the standard remote error envelope, and retryability is mapped
through the existing outbox retry/dead-letter machinery. Invalid result actions
are non-retryable protocol failures and cause the claimed outbox row to become
dead through the existing relay path.

## Implementation Order

1. Add manifest data types for remote function declarations without registering
   them. Done.
2. Extend the remote module protocol fixture and tests to expose those
   declarations. Done.
3. Add a proxy-backed `RuntimeFunction` implementation in
   `platform-module-remote`. Done.
4. Register remote function handlers into `FunctionRegistry` during module
   loading. Done.
5. Add worker/runtime tests proving success, retryable failure, exhausted
   attempts, timeout, and missing remote function behavior. Done.
6. Add Runtime Console tests only if existing function-run views need additional
   remote invocation metadata.
7. Add manifest event declarations plus proxy-backed remote event handlers that
   dispatch through the host-owned outbox relay. Done.
8. Allow remote event handlers to return one declarative `enqueue_function`
   result action for a runtime function declared by the same remote module.
   Done.

Do not implement event-emitting result actions, admin action bridges, arbitrary
host bridges, streaming, or marketplace trust in the remote event-handler result
slice.
