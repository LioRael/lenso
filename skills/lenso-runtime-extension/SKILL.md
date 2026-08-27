---
name: lenso-runtime-extension
description: Implement or change Lenso host mechanics through a Runtime Driver, Execution Adapter, Runner, App Generation controller, execution class, process/wire or endpoint preparation, or lane integration. Use when the concern exists to execute or switch Plugin graphs, not when deleting a product feature should remove it.
---

# Lenso Runtime Extension

Extend how Plugins run without turning host machinery into product Plugins or
moving host/product policy into the portable Kernel.

## Workflow

1. **Classify the seam.** Apply
   [runtime seams](references/runtime-seams.md). Name the host facility being
   adapted, the portable Interface it implements, and why deleting a product
   feature would not remove the facility. Route removable product behavior to
   `lenso-plugin-authoring`. Finish when Driver, Adapter, Runner, authoring, or
   Kernel owns the change unambiguously.
2. **Resolve the live contract.** Find repository instructions, selected core
   package versions, relevant ADRs, owning runtime/Adapter repository,
   conformance package, supported targets, CI gates, and existing production
   implementation. Cross-repository dependencies use released packages or an
   explicitly approved immutable bootstrap reference. Finish when every trait,
   error, and execution-class identity comes from current source.
3. **Read one implementation branch.** Use
   [Runtime Driver](references/runtime-driver.md) for scheduling/time/task-lane
   work; [Execution Adapter](references/execution-adapter.md) for Plugin
   generation, endpoints, process/wire, or isolation; and
   [Runner and conformance](references/runner-and-conformance.md) for assembly,
   host shutdown, lanes, terminal outcomes, or cross-implementation proof. Use
   [App Generation control](references/app-generation-control.md) for durable
   stage/switch/drain/rollback, routing Leases, structural recovery, or Plugin
   control-plane mechanics.
   Read multiple branches only when the request crosses their real boundary.
4. **Preserve inward dependencies.** Serializable Plan data and Kernel
   Interfaces stay portable. Drivers/Adapters implement them; Runners assemble
   concrete implementations. Core does not depend on a host runtime, protocol,
   Plugin, CLI, or example. Finish when the dependency graph points inward and
   every host API remains outside portable core.
5. **Implement one narrow translation.** Translate the selected host facility
   into the exact portable Interface and map every host outcome to a truthful
   Kernel result. Keep policy/configuration at the Runner or Adapter boundary
   that owns it. Finish when no second graph, hidden binding, ambient registry,
   or product fallback exists.
6. **Fail closed before readiness.** Reject unavailable/duplicate execution
   classes, missing/duplicate factories, invalid entrypoints, endpoint or
   Descriptor mismatches, unsupported interaction kinds, protocol handshake
   mismatches, and incomplete bindings before activation. Bound frames, queues,
   tasks, and cancellation state. Finish when malformed host state cannot reach
   Plugin business code.
7. **Prove portable and real behavior.** Run product-neutral conformance, the
   owning repository's locked gates, target compile checks, and the branch's
   real host smoke. Exercise startup failure, cancellation/deadline, shutdown,
   recreation/supervision, and terminal outcome where affected. Finish when a
   fake-only test cannot mask a broken process, browser, WASIp2, wire, or host
   boundary.

Return the chosen seam and owner, host facility, core Interface version,
dependency direction, failure boundary, conformance and host-smoke evidence,
and any product behavior routed back to a Plugin.
