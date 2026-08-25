# Plugin distribution

Read this branch only when an ordinary Module package must be installed,
enabled, upgraded, rolled back, or removed independently by a product Host.
A Plugin is the installable distribution role of a Module package, not a peer
runtime type and not a wrapper around Module behavior.

## Confirm the implemented product path

Find the target product's accepted Plugin contract, supported execution
classes, Bundle format, product metadata/Slot schema, admission policy, CLI,
and end-to-end fixture. Generic Lenso architecture does not prove that every
product can install every Module dynamically.

The source-derived authoring pipeline is still being completed. Rust can derive
Module Descriptors, configuration Schema, Capability endpoints, linked factory,
and registration through the public `lenso` facade. The complete `lenso pack`
experience, TypeScript `defineModule` derivation, generated Slot Entries,
generic Desired State resolver, and hot Plan Transition are not yet general
product capabilities. Report those gaps instead of hand-authoring a pretend
finished workflow.

## Build one immutable Release

Use the owning build/package tool when it exists. The resulting Release must
close:

- stable Plugin identity and immutable release version;
- generated Module Descriptor and configuration/Capability Schema digests;
- exact Artifact variants, entrypoints, target constraints, and execution
  classes;
- requested permissions, never self-granted permissions;
- product-owned metadata for supported Slots; and
- detached provenance/signature material required by target admission.

Do not execute Plugin code during discovery or admission. Do not let publisher
metadata bind private product Instances, replace an existing `one` provider,
assign global priority, or widen Host policy.

For a data-only Plugin, emit inert data entries with digests and product schema
identity. It has no entrypoint, execution class, Capability, permission, or
runtime callback; an explicitly selected interpreter Module owns its meaning.

## Prove both Module and Release behavior

First run the ordinary Module behavior, lifecycle, Runtime Failure, and
deletion proof. Then use the target product's real admission and transition
path to prove:

1. the Bundle is verified and stored without becoming active;
2. the App owner approves only the requested Release, Instance choices, and
   narrowed scopes;
3. the candidate resolves to exact Plan/Artifact/grant authority;
4. readiness precedes the atomic switch;
5. admitted work remains pinned to one Generation while the predecessor
   drains; and
6. rollback and removal restore exact prior authority without mutating Kernel.

The Agent Harness currently provides the deepest product-specific executable
Plugin slice. Treat its reviewed Tool/Model/Auth profiles and risk-based local
admission as Harness policy, not a generic permission for other products.

## Completion

Return the Module proof separately from the Plugin proof: generated authorities,
Bundle and Release identity, product Slot/profile, requested versus effective
grants, admission result, selected Generation, switch/drain evidence, rollback
handle, provenance inspection, and every unsupported product-chain step.
