# Domain docs

Lenso `next` is the vNext context. Before exploring or changing the code, read:

- [`CONTEXT.md`](../../CONTEXT.md)
- [`docs/adr/README.md`](../adr/README.md)
- [`docs/architecture/lenso-vnext.md`](../architecture/lenso-vnext.md)
- the relevant ADR 0030–0057

Use the canonical terms App, Module, Module Instance, Capability, Operation,
App Composition, Resolved App Plan, Kernel, Runtime Driver, and Execution
Adapter. Do not introduce Service, Provider, System Plane, Console, Story,
Auth, or PostgreSQL as vNext runtime types.

If a proposal conflicts with an ADR, surface the conflict explicitly and
identify the smallest new seam that resolves it.
