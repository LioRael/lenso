# Agent-Ready Support Desk Acceptance

The Support Desk acceptance is the highest public product seam for Lenso's
agent-ready application workflow:

```text
Build and operate a support ticket application with Lenso.
```

The runnable application lives in
[`LioRael/lenso-examples`](https://github.com/LioRael/lenso-examples). Lower-level
framework, CLI, Console, generated-client, browser, transport, and container
tests support this scenario; they do not replace it.

## Public lifecycle

1. **Compose.** Materialize and validate the exact `lenso.app.json`, including
   its revision, immutable Module release digests, dependency selections, and
   Linked or Service implementation bindings.
2. **Run locally.** Start the System through `lenso system dev` and the typed
   Local Control Adapter. Runtime commands and credentials remain outside the
   App Composition.
3. **Connect.** Start the separate Console Service and connect the exact System
   topology and Management Binding through the authenticated Console Service
   API. Console does not create, adopt, release, or deploy Workloads.
4. **Status.** Use a real browser to inspect System, Service, Module, Surface,
   and Workload states. Each object is `connected`, `unavailable`,
   `incompatible`, or `unmanaged` with a direct reason.

## Business scenario

The browser loads the receipt-bound Support Ticket and Story `console_ui_esm`
Surfaces without manual enablement. The Support Ticket Surface uses its
generated client and Surface Gateway to list, create, update, and close tickets
through the real Business API.

The acceptance verifies representative actor, tenant, deadline, idempotency,
Surface Grant, Console actor, and target Module authorization behavior. Browser
code never receives a managed-Service credential and never reads a database
directly.

## Local Workload control

The same scenario completes one supported Suspend/Resume or Stop/Start round
trip through the Local Control Adapter. It observes the asynchronous Operation
Record and final operational state through Console.

When the Adapter is unavailable, observation reports unknown state and mutation
is rejected without queueing, replay, or fallback. Console and the active
Adapter remain protected targets.

## Authoring route

Use `lenso-start` when the workflow owner is not yet clear. The usual Support
Desk path composes the App, implements the Support Ticket Module and its
Business API, authors its Console Surface, then runs the integrated acceptance.
The exact package manifests, generated contracts, current CLI help, and
repository checks remain authoritative.

## Keep out

- Do not add a custom agent runtime for this scenario.
- Do not route business behavior through Console or the System Plane.
- Do not introduce a catch-all records surface.
- Do not require production deployment orchestration or release mutation.
- Do not give browser code direct Service credentials or database access.
