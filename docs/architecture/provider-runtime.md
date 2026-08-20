# Provider Runtime

> **Legacy v0.3.x architecture:** This page describes the maintained
> Service-oriented implementation and is not normative for vNext. Read
> [lenso-vnext.md](lenso-vnext.md) for vNext decisions.

Provider Runtime is the Host-side transport for Modules delivered by a Service.
It is not a Module source or public delivery kind: Module Ecosystem V1 exposes
only `linked` and `service` delivery.

The implemented `lenso.provider.v1` boundary uses one locked descriptor and one
common invocation/outcome envelope across HTTP, schema reads, admin actions,
runtime functions, and Event handlers. The Host verifies Service Release,
Module Release, Manifest, export, and contract digests before activating an
export; timeout recovery uses the durable invocation identity and outcome
acknowledgement. The Host effect coordinator commits validated Events, Runtime
Function requests, and the invocation receipt in one Host Store transaction
before acknowledgement. Recovered outcomes reuse the committed digest and
cannot rebind an invocation identity to different effects.

Runtime behavior remains Host-owned: the host
must continue to own durable queues, retry policy, Runtime Story semantics, and
operator visibility. A Provider Service export may execute work, but it must not become a
parallel runtime.

## Goals

- Allow a configured Provider Service export to provide runtime function implementations.
- Keep `runtime.function_runs` as the durable source of truth.
- Keep the worker as the only component that claims, retries, and completes
  function runs.
- Reuse existing Console stories, timelines, retries, execution logs,
  payloads, and Technical Operations.
- Preserve the `ModuleManifest` data / `ModuleBinding` behavior split.

## Non-goals

- Provider Service exports directly polling or claiming `runtime.function_runs`.
- Provider Service exports directly consuming `platform.outbox`.
- Provider scheduling, queues, flows, or trigger ownership.
- Streaming function output or long-lived bidirectional channels.
- Browser credentials, host bearer-token forwarding, or arbitrary host bridges.
- Operator trust decisions for explicit module installs and official catalog
  curation.
- Wasm execution or JavaScript bundle execution.

## First Slice

The first slice should support host-invoked provider functions only.

The host remains responsible for:

- loading provider function declarations from the module manifest;
- registering proxy-backed function handlers in `FunctionRegistry`;
- claiming pending rows from `runtime.function_runs`;
- constructing `ExecutionContext`;
- enforcing timeout, retry, and body-size policy;
- mapping provider success/failure to existing function-run statuses;
- writing execution logs and Runtime Story data through the existing runtime
  path.

The Provider Service export is only an executor. It receives one function invocation from
the host and returns one result envelope.

## Manifest Shape

Function declarations are pure manifest data. `ModuleManifest::runtime` uses a
`RuntimeSurface` wrapper to describe functions without embedding handlers:

```json
{
  "runtime": {
    "functions": [
      {
        "name": "provider_crm.sync_contact.v1",
        "version": 1,
        "queue": "provider-crm",
        "input_schema": "provider_crm.sync_contact.v1",
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

- whether `input_schema` references committed host contracts, provider-provided
  schema fragments, or both;
- whether queue names are module-local by default and rewritten by the host when
  proxy-backed registration is added.

Do not put executable behavior or transport clients into `ModuleManifest`.

## Protocol Sketch

The default protocol is request/response JSON over the existing Provider Service export
base URL:

```text
POST /lenso/provider/v1/exports/{export_key}/runtime:invoke
```

Request:

```json
{
  "function_run_id": "fnrun_01",
  "function_name": "provider_crm.sync_contact.v1",
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

Failure response should use the existing provider error envelope:

```json
{
  "error": {
    "code": "external_dependency_failure",
    "message": "provider CRM was unavailable",
    "retryable": true,
    "details": []
  }
}
```

The host maps retryable failures through the existing runtime retry machinery.
The Provider Service export may suggest retryability, but the host applies the registered
retry policy and maximum attempts.

For a Provider-backed runtime function, one stable `function_run_id` represents
one owning-Module business attempt. The outer Provider `invocation_id` is
deterministic per `(function_run_id, attempt)`: it remains stable for POST
timeout recovery and acknowledgement of that exact technical attempt, then
changes when the Host runs the next technical attempt. A known business
observation is a succeeded Provider operation, even when the observed business
result is transient or permanent failure. A retryable failed Provider outcome
means the operation could not establish a business observation and therefore
belongs only to the Host's technical retry rail. The owning Module decides
whether a known business result creates a new business attempt and therefore a
new stable function run.

Provider-backed runs opt into a bounded terminal lifecycle Event when the Host
exhausts technical retries before any Provider business observation can be
committed. The Event is inserted atomically with the `dead` transition, has a
stable identity derived from the function run, and contains only run, function,
owner Module, correlation, and sanitized failure classification.
It never includes function input, rendered content, credentials, or raw
Provider responses. Owning Modules consume this public lifecycle fact instead
of reading `runtime.function_runs`.

Provider outcomes are bounded to 1 MiB by the Host response decoder before
deserialization. This aggregate bound covers effect evidence, Event payloads
and headers, and Runtime Function inputs. Before committing anything, the Host
also caps a batch at 100 effects and rejects duplicate Event or Runtime request
identities.

## Runtime Story Semantics

Provider function execution should not create a new product surface. It should
look like any other runtime function run:

- `runtime.function_runs` row is the story node source.
- Timeline item type remains `function_run`.
- Execution logs are written by the host before/after the provider invocation.
- Technical Operations may include a `source = "provider_runtime"` operation for
  the outbound invocation, but the business Story node remains the function run.
- Retry and dead-letter behavior use the same Console flows as linked
  functions.

The host should attach compact provider invocation metadata to execution logs or
Technical Operations, not invent a second provider-function history page in the
first slice.

## Auth And Transport

The host must not forward the caller's bearer token. If the provider source is
configured with a host-to-provider token, the host uses that token when invoking
provider functions, matching the HTTP proxy boundary.

Forward only operational context needed for execution:

- request id generated by the worker for this invocation;
- correlation id;
- causation id;
- trace context;
- actor context after host-side validation.

Function invocation request and response bodies must have explicit size limits.
Timeouts should use the Provider Service export source timeout unless a narrower runtime
function timeout is configured.

## Event Handlers

Provider event handlers use the same host-owned outbox dispatch model as linked
handlers. A Provider Service export may declare event subscriptions, but it never claims
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

The worker loads locked Provider Service exports through `lenso-bootstrap`, registers
proxy-backed handlers in the shared `EventHandlerRegistry`, then dispatches
claimed outbox rows through the existing relay. Success marks the row
`published`; retryable provider failures use the existing `failed` retry path and
eventually become `dead` after `max_attempts`.

The default provider protocol is request/response JSON over the module base URL:

```text
POST /lenso/provider/v1/exports/{export_key}/events:handle
```

The request includes the host-owned outbox event id, event name/version,
source module, aggregate identity, correlation/causation ids, actor, trace,
payload, and original event headers. The host may authenticate with the
configured host-to-provider token, but must not forward caller bearer tokens or
cookies.

Success may return a JSON body or `204 No Content`. Empty success performs no
follow-up action. JSON success may include a bounded declarative result action:

```json
{
  "actions": [
    {
      "type": "enqueue_function",
      "function_name": "provider_crm.sync_contact.v1",
      "input": { "contact_id": "usr_1" }
    }
  ]
}
```

The first result-action slice intentionally supports at most one
`enqueue_function` action. The host only accepts functions declared by the same
Provider Service export and already registered in the host `FunctionRegistry`; it uses the
registered retry policy when inserting `runtime.function_runs`. The provider
handler cannot set host retry policy, write runtime tables, emit events, invoke
admin actions, or call arbitrary host bridges.

Failure uses the standard provider error envelope, and retryability is mapped
through the existing outbox retry/dead-letter machinery. Invalid result actions
are non-retryable protocol failures and cause the claimed outbox row to become
dead through the existing relay path.

Stable Host Event and Runtime request identities are compare-and-replay
boundaries: identical content is safe across a new technical invocation
identity, while content drift is a conflict. Runtime effects preserve the
invocation actor, tenant, and trace exactly; delegation does not implicitly
authorize a Service to mint broader Host execution authority.

## Implementation Order

1. Add manifest data types for provider function declarations without registering
   them. Done.
2. Extend the Provider Service export protocol fixture and tests to expose those
   declarations. Done.
3. Add a proxy-backed `RuntimeFunction` implementation in
   `platform-module-provider`. Done.
4. Register provider function handlers into `FunctionRegistry` during module
   loading. Done.
5. Add worker/runtime tests proving success, retryable failure, exhausted
   attempts, timeout, and missing provider function behavior. Done.
6. Add Console tests only if existing function-run views need additional
   provider invocation metadata.
7. Add manifest event declarations plus proxy-backed provider event handlers that
   dispatch through the host-owned outbox relay. Done.
8. Allow provider event handlers to return one declarative `enqueue_function`
   result action for a runtime function declared by the same Provider Service export.
   Done.

Do not implement event-emitting result actions, admin action bridges, arbitrary
host bridges, or streaming in the provider event-handler result slice.

Native gRPC transport is a separate lane. See
`docs/architecture/module-provider-grpc.md`.
