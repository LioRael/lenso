# Lenso Service Operations Plane V8 Design

## Goal

V8 makes service-provided behavior visible, checkable, and operable as
host-owned service operations. V6/V7 made services installable, upgradeable,
observable, and deployable. V8 should make every HTTP route, runtime function,
event handler, and admin action answer the same operator questions:

- what service and module provide this operation;
- what capability and policy protect it;
- whether it is safe to probe;
- how it was invoked;
- where its Remote Calls, Runtime Story, and Technical Operations evidence live;
- what the next fix is when it fails.

This is still not a service mesh, API gateway, distributed transaction layer,
schema registry, or orchestration platform.

## Baseline

The current product model is:

```text
service = remote provider process + service manifest + one or more modules
module = business capability inside a service or linked into the host
host = control plane for auth, capabilities, runtime queues, outbox, story,
       technical operations, install state, service process state, and Console
```

V7 already added service contract checks, service create for Rust and TypeScript,
`service check --serve-command`, service diff/upgrade/rollback, deployment
exports, Console provider detail, and Rust/TypeScript service proofs.

V8 builds on that without adding a second runtime.

## Non-Goals

V8 must not add:

- service discovery;
- API gateway routing beyond existing host-owned module HTTP proxy paths;
- service mesh concepts;
- distributed transactions;
- a global schema registry;
- automatic trust, signing, or marketplace policy;
- Kubernetes operators;
- long-lived streaming, websockets, or SSE;
- browser bearer-token forwarding to services.

## Core Idea

Introduce a host-owned service operation view:

```text
service manifest
  -> modules
    -> HTTP routes
    -> runtime functions
    -> event handlers
    -> admin actions
  -> Service Operation Catalog
  -> CLI checks
  -> Console operation detail
  -> Remote Calls / Runtime Story / Technical Operations evidence
```

The operation catalog is an index and diagnostic surface. It does not own
execution. Execution stays in the existing host paths:

- HTTP routes through the host remote HTTP proxy;
- runtime functions through host-owned `runtime.function_runs`;
- event handlers through host-owned outbox dispatch;
- admin actions through host-owned admin-data action invocation.

## Operation Model

Each operation is a normalized view over an existing manifest declaration.

```text
operation id = <module>/<kind>/<name-or-method-path>
provider = service name
module = module name
kind = http_route | runtime_function | event_handler | admin_action
capability = declared capability when present
safe probe = explicit manifest metadata only
evidence = host-owned call/story/technical operation links
```

The operation catalog should be derivable from loaded service/module metadata
and install receipts. It should not require a new database table in the first
slice. If query cost becomes a problem, cache the computed view behind the
existing admin-data API.

## Operation Contract Metadata

V8 extends manifest declarations with optional operation metadata. Existing
manifests remain valid.

Supported optional fields:

- `operationId`: stable human-readable operation id when the default is too
  verbose.
- `inputSchema`: manifest-local JSON schema for sample input validation.
- `outputSchema`: manifest-local JSON schema for expected response/output.
- `safeProbe`: explicit probe declaration for checks.
- `timeoutMs`: operation-specific timeout hint, clamped by host policy.
- `idempotency`: `none`, `idempotent`, or `requires_key`.
- `summary`: short Console label.

`safeProbe` is deliberately explicit. The CLI must not infer safe POST/action/
runtime/event calls from naming. A safe probe may include:

```json
{
  "method": "POST",
  "path": "/tickets",
  "input": { "title": "Probe ticket", "dry_run": true },
  "expect": { "status": 200 }
}
```

For runtime functions, admin actions, and event handlers, probes run only when
the declaration says they are safe. Otherwise `lenso service check` reports the
operation as declared but not probed.

## Host Invocation Context

Host-to-service calls should use one consistent context envelope where the
transport supports it.

Forwarded context:

- request id;
- correlation id;
- causation id;
- traceparent;
- provider name;
- module name;
- operation id;
- operation kind;
- actor kind after host validation.

Never forward:

- browser `Authorization`;
- cookies;
- `set-cookie`;
- hop-by-hop headers;
- raw host runtime table identifiers beyond the operation-specific request id.

TypeScript and Rust SDK helpers should expose a small reader for this context.
They should not introduce a framework abstraction over Express/Axum/Node HTTP.

## CLI UX

V8 strengthens `lenso service check` around operations:

```sh
lenso service check <manifest-or-url>
lenso service check <manifest-or-url> --operation support-ticket/http/GET:/tickets
lenso service check <manifest-or-url> --sample-input probe.json
lenso service check <manifest-or-url> --json
```

Output should group by operation:

```text
Service manifest ok: support-suite-provider 0.1.0
Operations:
- ok      support-ticket http_route GET /tickets
- skipped support-ticket runtime_function support-ticket.escalate-ticket.v1 safe probe not declared
- ok      support-ticket admin_action assign_ticket
Evidence:
- Remote Calls: /operations/remote-calls?module=support-ticket
- Runtime: /operations/functions?module=support-ticket
- Story: /?q=support-suite-provider
```

Keep the check local and deterministic. It may start a service with
`--serve-command`, fetch the manifest, run declared safe probes, and stop the
child process. It must not install into a host unless the user runs install.

## Console UX

Console `/services` should grow from provider detail into operation detail:

- provider summary remains the first view;
- an Operations section lists operation kind, module, capability, state, last
  success, last failure, and links;
- selecting an operation opens a detail panel;
- the detail panel links to Remote Calls, Runtime Story, Technical Operations,
  function run details, and retry/dead-letter views when applicable;
- degraded operations show one next action.

The Console should use host-owned state only. It should not call services
directly from the browser.

## Managed Service Logs

V8 adds the smallest useful log story for host-started services:

```text
.lenso/service-logs/<provider>/<service>.log
```

Rules:

- only CLI/host-started local services write these logs;
- external deployments are shown as externally managed;
- logs are plain text append-only files with a bounded tail shown in Console;
- no log search, indexing, retention policy, or distributed log backend.

This gives operators enough local evidence without pretending Lenso is a log
platform.

## SDK Changes

TypeScript service kit:

- operation metadata helpers;
- safe probe helpers;
- host invocation context reader;
- tests that serialize V8 metadata without breaking V6/V7 manifests.

Rust service path:

- expose the same metadata shape through plain serde-compatible structs or
  helper constructors;
- keep Axum in the template/example;
- add context extraction helper for request headers.

Do not add a new server framework wrapper.

## Examples

V8 proof examples:

- `support-suite-provider` remains the broad TypeScript proof and should expose
  HTTP route, admin action, runtime function, and event handler operation
  metadata.
- `rust-audit-service` should add one runtime function or event handler so Rust
  proves more than HTTP route serving.
- Both examples should work with `lenso service check --serve-command`.

## Phases

### Phase 1: Operation Catalog

- normalize service operations from loaded service/module metadata;
- add admin-data response shape for provider operations;
- expose operation links to Remote Calls, Runtime Story, Runtime Functions, and
  Technical Operations;
- keep the catalog computed from existing state.

### Phase 2: Contract Metadata

- add optional operation metadata fields to TypeScript and Rust contract helpers;
- keep old manifests accepted;
- add contract tests for HTTP route, runtime function, event handler, and admin
  action metadata.

### Phase 3: Operation-Aware Check

- extend `lenso service check` to print operation groups;
- support `--operation`;
- run only explicit safe probes;
- report skipped unsafe operations clearly.

### Phase 4: Console Operation Detail

- add operation list and detail panel under `/services`;
- show last success/failure and evidence links;
- add one next action per degraded operation state.

### Phase 5: Local Logs

- capture stdout/stderr for host-started service processes;
- add `lenso service logs <provider> <service>`;
- show bounded log tail in Console for local managed services.

### Phase 6: Proof Upgrade

- upgrade `support-suite-provider` with operation metadata and safe probes;
- upgrade `rust-audit-service` with one runtime/event operation;
- document the V8 proof path in `lenso-examples`.

## Success Criteria

V8 is done when:

- a user can open `/services`, select a provider, and see every provided
  operation with evidence links;
- `lenso service check --serve-command` explains which operations were checked,
  skipped, or failed;
- safe probes never run unless declared;
- Remote Calls, Runtime Story, and Technical Operations remain the evidence
  chain;
- linked modules and existing service manifests still work;
- no service receives browser bearer tokens;
- no service writes host runtime, outbox, story, or technical operation tables.
