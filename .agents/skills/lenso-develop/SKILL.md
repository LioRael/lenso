---
name: lenso-develop
description: Add or modify consumer Lenso plugins and services, Web/Auth integration, explicit CLI/MCP exposure, or application Engine extensions. Not for merely invoking existing operations, generic TypeScript/Bun work, delivery, or framework core maintenance.
---

# Develop a Lenso application

## Classify and read narrowly

1. Classify the change: runtime service/plugin, transport or authentication boundary, explicit operation exposure, or build-time Engine extension. A task can cross these boundaries; keep their responsibilities separate.
2. Read the target application's instructions and manifest: scripts, dependency versions and exports when present. Resolve installed versions from its lockfile/package metadata, not from an example's pins.
3. Read its actual assembly entry and one nearby real plugin. Follow only the imports needed to identify the installed instances, shared input schema, service and changed entry. Locate `lenso.config.ts` for CLI exposure and `lenso.engine.ts` only for Engine work; a full repository scan is unnecessary.
4. Select references by what changes, not every package already installed. Read the linked section/example needed for that decision, not entire manuals. Public docs/examples pin baseline `6e239c71a38279885facce133ceb847bbfe12f2f`; match actual installed exports/declarations, not just versions. Merged APIs may be unpublished. External applications need no framework checkout.

## Preserve the application contracts

- Bind `requires`, `context.get`, `app.get` and Operation declarations to the **exact installed Plugin object**. A new object with the same ID or type is not that dependency. Give distinct instances distinct IDs.
- Keep business services ordinary async functions. Plugin setup wires dependencies and lifetime; entry adapters reuse shared schemas/services rather than introduce another business implementation.
- Acquire resources during setup and register owned cleanup immediately, before the next fallible step. Borrowed clients and platform bindings stay with their owner. Config module top-level code declares assembly without acquiring resources.
- Authenticate at each trusted entry and enforce the same audience/object/owner/tenant policies in the shared service. Business JSON is never a trusted actor, owner assignment or permission grant.
- Use exported package entries only. Source/config is application-owned; `.lenso` and `dist` are reproducible output. Edit source and regenerate rather than patch generated files.
- In a framework workspace, build affected framework packages and dependencies before consumers: exports resolve to `dist`. Avoid consumer typecheck while a build clears that output. External apps use installed artifacts and their own focused scripts; local checks grant no publication, deployment or production authority.

## Route by the changed boundary

- Plugin dependencies, setup/cleanup or typed instance config: [Lifecycle and configuration](references/lifecycle.md).
- CLI operations, MCP tools, requested Manage exposure or generation/build/dev extensions: [Engine and explicit exposure](references/engine-cli.md).
- Fetch/oRPC routes, Bun listener, sessions or authorization: [Web and Auth](references/web-auth.md).
- Database provider/schema, file-state/policy or task contracts, not a projection over an unchanged service: [DB, Files and Tasks](references/data.md).
- Workers, browser/external packages or logging/telemetry bootstrap: [Platforms and external consumers](references/platform.md).
