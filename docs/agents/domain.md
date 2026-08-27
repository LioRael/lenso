# Domain docs

Lenso `main` is the vNext context. Before exploring or changing the code, read:

- [`CONTEXT.md`](../../CONTEXT.md)
- [`docs/adr/README.md`](../adr/README.md)
- [`docs/architecture/lenso-vnext.md`](../architecture/lenso-vnext.md)
- the relevant ADR 0030–0070

Use the canonical terms Host, Plugin Root, App, Plugin, Plugin Instance,
Capability, Port, Slot, App Composition, Plan Snapshot, Plan Transition,
Reconciler, App Generation, Kernel, Runtime Driver, and Execution Adapter. Do
not introduce App Definition, Module, Service, Provider, System Plane, Console,
Story, Auth, or PostgreSQL as public vNext runtime types.

If a proposal conflicts with an ADR, surface the conflict explicitly and
identify the smallest new seam that resolves it.
