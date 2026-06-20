# Agent-Ready Module Demo

This demo is the public proof point for Lenso's agent-ready module workflow:

```text
Build a support ticket module for a Lenso app.
```

The runnable example lives in the sibling
[`lenso-examples/examples/support-ticket`](https://github.com/LioRael/lenso-examples/tree/main/examples/support-ticket)
repository path.

## Flow

1. Use `lenso-start` to pick the right public path.
2. Use `lenso-module-authoring` for an in-host Rust module, or
   `lenso-remote-module-authoring` for an out-of-process module.
3. Scaffold the module:

```sh
lenso module create support --with-console
```

4. Add the smallest useful support-ticket slice:
   - ticket list/detail data surface
   - create or update action
   - one runtime workflow or function
   - one Runtime Console surface
   - one smoke check that fails if the module is not wired
5. Run the focused checks for the changed surface.
6. Open `/console` and confirm the module appears with its data, actions, and
   runtime visibility.

## Run The Example

From `lenso-examples`:

```sh
pnpm smoke:support-ticket
pnpm start:support-ticket
```

Install the running module into a local Lenso host:

```sh
lenso module install http://127.0.0.1:4110/lenso/module/v1/manifest
```

## What This Proves

- A business capability can ship as a Lenso module.
- The module declares its backend and console shape through explicit manifests.
- Agents have stable rails: scaffolds, skills, contracts, checks, and Console
  verification.
- Teams can start in one deployable system and split hardened modules across
  process or service boundaries later.

## Keep Out

- Do not add a custom agent runtime for this demo.
- Do not require marketplace trust, deployment orchestration, or service
  discovery.
- Do not build a generic CRUD framework before a real module needs it.
