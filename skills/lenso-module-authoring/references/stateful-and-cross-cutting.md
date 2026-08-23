# Stateful and cross-cutting Module recipe

Read the section that matches the product responsibility. Both branches begin
with the native Rust or Bun execution recipe and add ownership constraints;
they are not separate runtime types.

## Stateful Module

A stateful Module owns the meaning and lifecycle of its data. Use this recipe:

1. Name the authoritative facts, invariants, transaction boundary, recovery
   behavior, retention, and failure policy.
2. Keep the persistence client, pool, queries, and tables behind a private
   Adapter unless another Module genuinely needs a replaceable semantic
   persistence Capability. Sharing a physical database never grants table
   access.
3. Keep schema and migration artifacts in the Module package. Run migrations
   through an explicit authoring/deployment command; ordinary `prepare` may
   verify schema compatibility and reserve reversible resources but does not
   apply an irreversible migration by default.
4. Put non-secret connection/configuration data or secret references in App
   Composition. Resolve dynamic secret values through an explicitly bound
   Secrets Capability.
5. Reject startup when required durable state cannot prepare. Report later
   storage loss as a Runtime Failure from the affected Operation; never fall
   back silently to ephemeral memory.
6. Test transaction semantics and recovery at the persistence boundary, then
   exercise the public Capability. A repository test that touches private
   tables is not a consumer contract.

There is no universal State Module or process-wide database pool in Kernel.
Use ADR 0041 and the selected persistence library as authority; do not copy
v0.3.x Service manifests into a vNext package.

## Cross-cutting product behavior

Auth, Secrets, Audit, OpenTelemetry, Story, Workflow, health, and similar
concerns are ordinary optional Modules when selecting the concern creates
product behavior, policy, tasks, state, or operational surface.

- Expose a deep Capability only when another Module needs the semantic role;
  keep owner-local helpers private.
- Keep final business authorization in the target Module. An Auth Module turns
  credential evidence into a scoped assertion; it does not grant ambient
  authority.
- Keep telemetry best-effort and non-authoritative. Audit/Story own durable
  evidence when the product requires it.
- Keep durable delivery and Outbox state with each stateful owner that needs
  atomic persistence; do not turn them into Kernel event semantics.
- Give every background worker generation-owned cancellation, task tracking,
  and bounded queues.

Current vNext examples include `LioRael/lenso-secrets-module` for a generated
Capability plus configuration/preparation, and `LioRael/lenso-otel-module` for
diagnostic subscription, generated application telemetry, managed export
tasks, trace propagation, and removal behavior.

## Deletion proof

Remove the package selection, Module Instance, bindings, configuration, and
Module-owned storage/tasks from the test App. The remaining Composition must
resolve when no requirement still needs the Module, and Kernel must retain no
feature flag, policy branch, global registry entry, or mandatory storage for
the removed concern.
