---
name: lenso-start
description: Route a Lenso task through the Core, Web, or Agent path to one primary owner workflow.
---

# Lenso Start

Start from the result, then choose the product path and owner. This Skill is the
human-invoked index; it coordinates the work without becoming another owner.

## Route

1. **State the result.** Describe what a person or another Plugin can observe,
   the authority already supplied, constraints, and the requested delivery
   level. Finish when the result can be understood without framework nouns.
2. **Choose one task map.** Read only the matching reference:
   - [Core framework](references/core.md) for Plugins, Capabilities, App
     configuration, composition, runtime, or framework semantics;
   - [Web product](references/web.md) for HTTP endpoints, Auth, upstream calls,
     Ingress, testing, or deployment; or
   - [Agent product](references/agent.md) for first Turns, Profiles, Tools,
     Sessions, Memory, child Agents, MCP, or Agent surfaces.
3. **Name one primary owner.** Select exactly one of
   `lenso-business-planning`, `lenso-capability-authoring`,
   `lenso-plugin-authoring`, `lenso-app-configuration`, or
   `lenso-runtime-extension`. The chosen task map may define later handoffs;
   they do not become parallel owners of the first change.
4. **Ground the route.** Inspect repository instructions, current source,
   selected package versions, installed `--help`, and the nearest real test.
   Finish when the proposed artifacts and commands exist in current authority.
5. **Continue through the primary Skill.** Add a secondary Skill only after an
   artifact crosses a real ownership seam. Stop at a missing prerequisite only
   when inventing it would change the requested result or authority.

Portable Kernel graph, lifecycle, invocation, admission, readiness,
supervision, and diagnostic semantics are not a sixth owner workflow. Route
those changes through the core repository's `CONTEXT.md`, relevant ADR, and
product-neutral conformance.

Return the selected task map, primary Skill, owner repository, first artifact,
observable completion state, later handoffs, and any missing prerequisite.
