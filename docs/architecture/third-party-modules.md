# Third-Party Modules and Services

This document defines the supported third-party boundary. Read it together with
`service-module-boundary.md`, `module-console-surfaces.md`, and
`provider-runtime.md`.

## Vocabulary

- A **Module** is a logical business capability selected by an immutable Module
  Release. The current executable source is `Linked`.
- A **Service** is an independently deployed process. It can provide one or more
  Module exports through a Provider Runtime Plan, but the process itself is not
  a Module.
- A **Provider** is a Host-subordinate transport binding to a Service export.
  The Host retains authorization, queues, retries, runtime records, outbox
  publication, and story ownership.
- `Remote Module` is retired vocabulary. Future Wasm support will be a new
  Module source and will not restore the old remote-process model.

## Immutable Releases

A Module Release binds the normalized `ModuleManifest`, compatibility metadata,
and optional isolated Console UI artifact to content digests. A Service Release
separately binds deployment and Provider protocol evidence. Installation selects
exact releases in `lenso.modules.lock.json` and the environment service
installation ledger; mutable manifest URLs are discovery inputs, not authority.

Every change is previewed as a Module Change Plan. Provider delivery adds an
exact Service Installation Plan. The durable management operation applies only
the approved effects and records receipts. Catalog entries and manifests cannot
write arbitrary environment values, run commands, launch processes, or mutate
Console files directly.

## Service Contract

A Service exposes the versioned Provider protocol declared by its selected
Service Release. The Host verifies the live descriptor against the locked
descriptor before activating the Provider. HTTP routes, runtime functions,
event handlers, admin data, and admin actions are usable only when declared by
the selected Module Release and allowed by Host policy.

Provider responses may request typed Host effects. The Host validates those
effects against the Module declarations, commits outbox/runtime requests in one
database transaction, and acknowledges only after commit. An invocation ID and
canonical effect digest make an exact replay idempotent and reject conflicting
replays. A Service never writes Host queues or runtime tables directly.

## Console UI Artifacts

Optional third-party UI is an immutable artifact referenced by the Module
Release, not an npm package installed into the hosted Console. Runtime Console
loads the artifact in a sandboxed cross-origin iframe with `allow-scripts` only.
The artifact receives no Host token, cookies, stores, or internal imports.

The artifact and Console Service communicate through `lenso.console-bridge.v1`. The
handshake binds the Module identity, surface name, Module Release digest, UI
artifact digest, granted capabilities, nonce, and expiry. The Console exposes only
typed operations authorized by the exact grant; the backend independently
checks the operator scope and the Module declaration for every operation.

The retired mechanisms must not return:

- `@lenso/runtime-console-api` or a host-internals alias;
- `@lenso/remote-module-kit`;
- same-origin dynamic bundles or `/console/extensions/*` routes;
- copied bundle ledgers or a Console extension registry;
- Module install or uninstall code that mutates Console assets.

## Host Responsibilities

The Host owns:

- release, compatibility, signature, and policy verification;
- capability enforcement and request/response limits;
- Provider endpoint and credential resolution;
- runtime queues, retries, outbox publication, and Runtime Story records;
- admin authorization and generic data/action dispatch;
- durable Module and Service management operations;

The Console Service owns Console composition grants and the Console Bridge
backend. A managed Host does not expose that route unless it is explicitly
composed as the Console Service with a Console-owned authority adapter.

Services own their business execution and stable effect identities. They must
not receive caller bearer tokens, impersonate Host-owned records, supervise the
Host, or depend on Console implementation details.

## CLI and Console Flow

Discovery may add a catalog entry, but installation always resolves an
immutable Module Release before preview and apply:

```sh
lenso module catalog add https://example.com/releases/billing/index.json
lenso module install billing
lenso module uninstall billing
```

Runtime Console uses the same `/admin/modules/*` management workflow. Neither
path copies frontend bundles. Console Composition resolves the selected UI
artifact digest and creates the bounded bridge grant when the surface opens.

## Current and Deferred Sources

Current Module execution is Linked, with Provider bindings available for
Service-provided behavior under Host control. Wasm is deferred until it has an
immutable artifact contract, resource limits, capability imports, and the same
Host-owned effect semantics. No generic independently running `Remote` Module
source is supported.
