---
name: lenso-start
description: Choose one Lenso vNext development workflow.
---

# Lenso Start

Route the request by its owner. This is the human-invoked index; the selected
skill owns the work.

## Route

1. State the requested outcome without framework nouns.
2. Choose exactly one primary workflow:
   - unclear product behavior, ownership, or boundaries ->
     `lenso-business-planning`
   - Capability identity, Operations, Schemas, compatibility, or generated
     consumer/provider bindings -> `lenso-capability-authoring`
   - product behavior implemented as a Rust, Bun, Web, stateful, Auth, Story,
     Audit, OpenTelemetry, Secrets, or other Module; or packaging that Module
     as an installable Plugin Release ->
     `lenso-module-authoring`
   - package or Plugin selection, keyed Module/Plugin Instances,
     configuration, Slot choices, bindings, placement, Web profiles, Desired
     State, or Resolved App Plan ->
     `lenso-app-composition`
   - scheduling, clocks, Module generation, Adapter-level endpoint
     preparation, language-process/wire integration, execution classes,
     Plugin admission/Store mechanics, Reconciliation, App Generation
     stage/switch/drain/rollback, or Runner orchestration ->
     `lenso-runtime-extension`
3. Treat portable graph, lifecycle, invocation, admission, supervision,
   readiness, and diagnostic semantics as Kernel work. Read the core
   repository's `CONTEXT.md` and relevant ADR rather than routing it through a
   product skill.
4. Name a secondary workflow only when the request crosses a real ownership
   boundary. Continue with the primary skill when it is available; otherwise
   report the missing catalog entry.

Examples:

- "Design a support-ticket product" -> planning first.
- "Add `assign_ticket` to the Ticket contract" -> Capability authoring.
- "Implement the Rust or Bun Ticket provider" -> Module authoring.
- "Package the Ticket Module as an installable Plugin" -> Module authoring.
- "Select two providers and bind one consumer" -> App Composition.
- "Enable a reviewed Plugin and choose its Slot" -> App Composition.
- "Add a Python process execution class" -> Runtime extension.
- "Implement durable App Generation rollback" -> Runtime extension.

Routing is complete when one owner and one observable completion state are
unambiguous. Ask one boundary question only when two owners still fit.
