# Third-Party Modules and Services

> **Legacy v0.3.x architecture:** This page describes the maintained
> Service-oriented implementation and is not normative for vNext. Read
> [lenso-vnext.md](lenso-vnext.md) for vNext decisions.

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
and optional same-realm `console_ui_esm` Console UI artifact to content digests. A Service Release
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

Optional third-party UI is an immutable `console_ui_esm` artifact referenced by
the Module Release, not an npm package installed into the hosted Console. The
artifact contract binds the Module identity, release and artifact digests,
protocol major, independent `hostApi` and `consoleUi` ranges, entries, ordered
style assets, generated surface manifest, permissions, and provenance. The
Console Shell validates that contract before loading the artifact in the same
realm.

The artifact interacts with a Managed Service through the typed
`lenso.system-plane.module-operations.v1` contract. Inventory, data-only action
contribution resolution, descriptor-bound configuration reads, and
descriptor-bound configuration writes are fixed operations. Each request binds
the target Service, environment, calling Module namespace, delegated authority,
capabilities, and target revision where mutation is involved. Sensitive
configuration is write-only and audit evidence contains digests rather than
values. There is no arbitrary endpoint proxy, query language, generic
key/value store, or Console Bridge route.

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

The Console Service owns Console composition and artifact receipts. A managed
Service exposes only its authenticated System Plane capability contracts; it
does not expose a Console Bridge compatibility route or accept Console-owned
state as authority.

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

Console reaches target-owned management only through negotiated System Plane
capabilities; the retired same-host `/admin/modules/*` adapter is not a
production authority boundary. Neither path copies frontend bundles. Console
Composition resolves the selected UI artifact digest and loads the reviewed
same-realm ESM entry after manifest and style-asset verification.

## Current and Deferred Sources

Current Module execution is Linked, with Provider bindings available for
Service-provided behavior under Host control. Wasm is deferred until it has an
immutable artifact contract, resource limits, capability imports, and the same
Host-owned effect semantics. No generic independently running `Remote` Module
source is supported.
