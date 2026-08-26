# Lenso authoring tooling

The `lenso-cli` repository owns the executable authoring product and the
`lenso-authoring` library under ADR 0064. The public CLI exposes author intent;
the library owns filesystem edits, package inspection, validation, derivation,
canonical Plan materialization, and development Host mechanics.

Neither installs code into a running Kernel. Package managers acquire packages
and write ordinary lockfiles. Kernel receives only a complete immutable
Resolved App Plan.

## Module author interface

Ordinary Module authors use five top-level commands:

```text
lenso new
lenso dev
lenso check
lenso verify
lenso app
```

`new` creates a Module project. `check` emits fast actionable diagnostics.
`dev` validates, resolves, and runs a fresh development generation. `verify`
proves declared behavior and removal. These commands hide Descriptor lowering,
binding closure, Plan serialization, Adapter assembly, and development Runner
mechanics behind one deep Interface.

The CLI intentionally does not expose the removed top-level `add`, `resolve`,
`run`, `compose`, or `module` command surfaces. They restated implementation
stages or duplicated the ordinary Module workflow.

## Source-derived App Definition

`lenso.app.json` is the only hand-authored App composition input. It selects
Module packages and keyed Instances, supplies configuration and lane choices,
records real binding ambiguities, and may assign request-admission limits to an
exact derived binding. Generated package-owned Descriptors provide
Capabilities, Ports, Operations, execution classes, and lifecycle facts.
Derived App Composition and Plan documents are locked outputs rather than
authoring surfaces.

Binding policies identify the consumer Instance, Capability, and provider
Instance together. They tune queue capacity and maximum concurrency without
selecting a provider or changing package-owned requirements and endpoints:

```json
{
  "binding_policies": [
    {
      "consumer": "agent",
      "capability_id": "lenso.agent.tools@2",
      "provider": "tools",
      "admission": {
        "queue_capacity": 0,
        "max_concurrency": 4
      }
    }
  ]
}
```

A duplicate policy or a policy that does not match a derived binding fails
resolution closed.

```sh
lenso app add greeting-module --definition lenso.app.json --version '^1.0'
lenso app check --definition lenso.app.json
lenso app resolve --definition lenso.app.json \
  --output .lenso/resolved-plan.json
lenso app remove greeter --definition lenso.app.json --uninstall
```

`app add` and `app remove` apply transactional package-manager and App
Definition edits. `app check` and `app resolve` remain explicit advanced
operations for App owners and Hosts that review or exchange canonical Plan
bytes. A product-owned Runner or Host executes those bytes; the authoring CLI
does not provide a generic `run --plan` interface.

## Internal Plan seam

Library Hosts may resolve once, persist or review canonical bytes, reload the
same bytes, and pass the resulting `ResolvedProject` to their Runner:

```rust,ignore
use lenso_authoring::{ProjectAuthoring, ResolvedProject, run_project};

let resolved = project.resolve(root, &options)?;
write_plan(resolved.canonical_bytes())?;
let approved = ResolvedProject::from_canonical_bytes(read_plan()?)?;
let outcome = run_project(&approved, driver, adapters, timeout).await?;
```

Changing the App Definition, package lock, configuration, lane, explicit
decision, or binding policy requires a new resolution before the Host applies
a validated Plan Transition or starts a fresh App Generation. Kernel never
discovers packages, selects providers, rewrites locks, or accepts an authoring
recipe as runtime authority.
