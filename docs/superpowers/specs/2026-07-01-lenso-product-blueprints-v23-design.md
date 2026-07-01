# Lenso Product Blueprints V23 Design

## Goal

Turn V22 Launchpad from a one-shot first-run path into a small product-growth system: users can discover built-in product blueprints, create a real app, add curated business addons, diagnose local readiness, and hand richer context to coding agents.

## Positioning

V23 is **Product Blueprints & Addons**.

The product story becomes:

```sh
lenso app list
lenso app create acme-support --blueprint support-desk
cd acme-support
lenso app add support-sla
lenso dev doctor
lenso dev up
lenso agent task "add overdue ticket escalation"
```

This keeps Lenso practical: it helps a real business app grow from a generated support desk into a modular service system without introducing a new platform layer.

## Boundaries

V23 does not add:

- external blueprint marketplace
- user-authored blueprint DSL
- background process supervisor
- AI provider integration
- workflow engine
- Kubernetes requirement
- service mesh or service discovery

Blueprints and addons are built-in CLI recipes. They generate or update existing Lenso control-plane files:

- `lenso.system.json`
- `lenso.workspace.json`
- `.lenso/launchpad.json`
- `.lenso/dev-doctor.json`

Module contracts and service manifests remain the authoritative runtime contracts. Launchpad state stays generated control-plane state.

## Product Concepts

### Blueprint

A blueprint is a built-in app recipe. It declares:

- name
- label
- summary
- generated host name
- services to scaffold
- modules owned by those services
- system graph entries
- Launchpad checklist entries
- supported addons

V23 ships three blueprints:

- `support-desk`
- `backoffice-crm`
- `ops-console`

`support-desk` remains the recommended default.

### Addon

An addon is a built-in extension recipe for an existing blueprint app. It declares:

- addon name
- supported blueprint names
- service scaffold to create or update
- modules added to the system graph
- capabilities and dependencies
- Launchpad checklist entries
- agent context hints

V23 ships three addons:

- `support-sla`
- `customer-profile`
- `notifications`

The first implementation can keep each addon as one generated service with one module. No addon may require editing a service internals by string replacement.

### Dev Doctor

`lenso dev doctor` checks the generated app's local readiness. It is static by default and only performs HTTP checks when explicitly requested.

Default checks:

- `.env` exists
- `.lenso/launchpad.json` exists and parses
- `lenso.system.json` exists and parses
- `lenso.workspace.json` exists and parses
- each workspace service `cwd` exists
- each workspace service manifest file exists
- required local command binary is present for `pnpm start` or `cargo run`

Optional live checks:

```sh
lenso dev doctor --live
```

Live checks probe service `readyUrl` values and report stopped services as actionable issues.

`--write-state` writes `.lenso/dev-doctor.json` so Runtime Console can show doctor state without running shell commands.

## CLI

New and extended commands:

```sh
lenso app list
lenso app inspect <blueprint>
lenso app create <dir> --blueprint <blueprint>
lenso app add <addon>
lenso dev doctor
lenso dev doctor --live
lenso dev doctor --write-state
lenso agent context
lenso agent task "..."
```

`app list` and `app inspect` are read-only.

`app add` updates an existing Launchpad app. It must:

- fail with a clear message when not run in a Launchpad app root
- fail when the addon does not support the current blueprint
- fail when the addon has already been applied
- scaffold only missing service directories
- update `lenso.system.json`
- update `lenso.workspace.json`
- append addon metadata to `.lenso/launchpad.json`

`agent context` includes:

- blueprint summary
- applied addons
- service/module ownership
- dev doctor summary when `.lenso/dev-doctor.json` exists
- existing host-owned runtime boundaries

`agent task` appends the requested task plus the same context.

## Generated State

`.lenso/launchpad.json` remains protocol `lenso.launchpad.v1` and receives additive fields:

```json
{
  "addons": [
    {
      "name": "support-sla",
      "label": "Support SLA",
      "status": "configured",
      "services": ["support-sla"],
      "modules": ["support-sla"]
    }
  ],
  "supportedAddons": ["support-sla", "customer-profile", "notifications"]
}
```

`.lenso/dev-doctor.json` uses protocol `lenso.dev-doctor.v1`:

```json
{
  "protocol": "lenso.dev-doctor.v1",
  "status": "needs_attention",
  "checkedAtUnixMs": 1782900000000,
  "live": false,
  "checks": [
    {
      "id": "env-file",
      "label": ".env file",
      "status": "passed",
      "message": ".env exists",
      "command": null
    }
  ]
}
```

Statuses are:

- `passed`
- `needs_attention`
- `failed`
- `skipped`

Overall doctor status is:

- `ready`
- `needs_attention`
- `failed`

## Host API

Add one small endpoint:

```text
GET /admin/data/launchpad/doctor
```

It reads `.lenso/dev-doctor.json`.

Missing state returns `empty` with:

```text
lenso dev doctor --write-state
```

This endpoint is read-only. It never runs doctor checks.

## Runtime Console

Launchpad 2.0 extends `/launchpad` with:

- blueprint name and summary
- applied addon list
- supported addon suggestions
- dev doctor status
- failing doctor checks and next commands
- existing service/module table

The page stays compact. It remains an entry screen, not a replacement for Services, Modules, Operations, Runtime Story, or Remote Calls.

## Examples And Site

`lenso-examples` adds fixtures for:

- blueprint catalog output
- `support-desk` with `support-sla`
- dev doctor state
- agent task context

`lenso-site` updates:

- Host Quickstart
- CLI Reference
- a new Product Blueprints page

Docs should say that Kubernetes remains optional. Blueprints are a local-product acceleration layer, not a deployment requirement.

## Success Criteria

- `lenso app list` shows the three built-in blueprints.
- `lenso app inspect support-desk` explains services, modules, and supported addons.
- `lenso app create acme-support --blueprint support-desk` still creates the V22 support desk path.
- `lenso app add support-sla` updates an existing support-desk app once and refuses duplicate application.
- `lenso dev doctor` reports static local readiness without requiring services to be running.
- `lenso dev doctor --write-state` writes `.lenso/dev-doctor.json`.
- `GET /admin/data/launchpad/doctor` returns the written doctor state or an empty next command.
- Runtime Console `/launchpad` shows addons and doctor state.
- `lenso agent task "..."` includes blueprint, addons, ownership, boundaries, and doctor summary.
- Existing linked modules, service install, service system, and V22 Launchpad flows keep working.
