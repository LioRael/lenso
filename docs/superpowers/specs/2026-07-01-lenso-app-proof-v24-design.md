# Lenso App Proof V24 Design

## Goal

Turn V23 Product Blueprints from a creation flow into a maintained app flow.
After a user creates a blueprint app and adds addons, Lenso should help them
verify, compare, repair, and explain the generated control-plane state without
touching their business logic.

V24 is **App Proof & Blueprint Lifecycle**.

The product story becomes:

```sh
lenso app create acme-support --blueprint support-desk
cd acme-support
lenso app add support-sla
lenso app verify --write-proof
lenso app diff
lenso app repair --dry-run
lenso agent task "add enterprise SLA escalation"
```

## Positioning

V23 answered "how do I create a product-shaped Lenso app?"

V24 answers "is this generated app still coherent after real work starts?"

This matters because generated apps drift. A developer can remove a workspace
entry, edit `lenso.system.json`, add a service without Launchpad metadata, or
forget to write doctor state. V24 gives operators and coding agents one compact
proof artifact that says what is intact, what drifted, and what command fixes
the safe parts.

## Boundaries

V24 does not add:

- user-authored blueprint DSL
- external blueprint marketplace
- automatic edits to user business code
- service discovery
- process supervisor
- Kubernetes requirement
- service mesh
- distributed version negotiation
- AI provider integration

Blueprint recipes remain built into the CLI. Runtime contracts remain service
manifests and module manifests. App Proof is generated control-plane evidence,
not a new source of truth.

## Concepts

### App Proof

`.lenso/app-proof.json` is the latest app-level verification result.

It records:

- protocol
- app name
- blueprint name
- applied addons
- status
- checked timestamp
- proof checks
- drift count
- next command

Statuses:

- `ready`
- `drifted`
- `needs_attention`
- `failed`
- `empty`

Check statuses:

- `passed`
- `drifted`
- `needs_attention`
- `failed`
- `skipped`

Example:

```json
{
  "protocol": "lenso.app-proof.v1",
  "status": "drifted",
  "checkedAtUnixMs": 1782900000000,
  "projectName": "acme-support",
  "blueprint": "support-desk",
  "addons": ["support-sla"],
  "checks": [
    {
      "id": "workspace-service-support-sla",
      "label": "support-sla workspace entry",
      "status": "passed",
      "message": "support-sla is present in lenso.workspace.json",
      "command": null
    },
    {
      "id": "launchpad-doctor-state",
      "label": "Launchpad doctor state",
      "status": "needs_attention",
      "message": ".lenso/dev-doctor.json is missing",
      "command": "lenso dev doctor --write-state"
    }
  ],
  "drifts": [
    {
      "resource": "dev-doctor",
      "name": ".lenso/dev-doctor.json",
      "message": "App proof has no latest doctor state.",
      "command": "lenso dev doctor --write-state"
    }
  ],
  "nextCommand": "lenso dev doctor --write-state"
}
```

### Blueprint Diff

`lenso app diff` compares the current app to the built-in blueprint and applied
addon recipes.

It checks generated control-plane state only:

- `.lenso/launchpad.json`
- `.lenso/dev-doctor.json`
- `lenso.system.json`
- `lenso.workspace.json`
- generated service directories and service manifest files

It does not diff user route handlers, module internals, database schema, or
application source files beyond the expected generated service manifest path.

### Safe Repair

`lenso app repair` only repairs generated control-plane state and missing
scaffold directories.

Allowed repairs:

- regenerate `.lenso/launchpad.json` from blueprint plus addons
- insert missing `lenso.workspace.json` service entries
- insert missing `lenso.system.json` service/module/dependency entries
- recreate a missing generated service directory when the service directory is
  absent

Forbidden repairs:

- overwrite an existing service directory
- edit source files inside an existing service
- remove unknown user-added services
- remove unknown modules
- run package managers
- start services

If repair sees a conflict, it reports a command and stops.

## CLI

New commands:

```sh
lenso app verify
lenso app verify --write-proof
lenso app diff
lenso app repair --dry-run
lenso app repair
```

`app verify`:

- reads `.lenso/launchpad.json`
- resolves the blueprint and applied addons
- runs the same checks as `app diff`
- folds `.lenso/dev-doctor.json` into the result when present
- prints a compact status table
- writes `.lenso/app-proof.json` only with `--write-proof`

`app diff`:

- prints drift rows
- exits successfully when no drift exists
- exits non-zero when drift or parse failure exists

`app repair --dry-run`:

- prints planned safe repairs
- writes nothing

`app repair`:

- applies only safe repairs
- prints skipped conflicts
- suggests `lenso app verify --write-proof` after changes

## Host API

Add one read-only endpoint:

```text
GET /admin/data/launchpad/proof
```

It reads `.lenso/app-proof.json`.

Missing proof returns `empty` with:

```text
lenso app verify --write-proof
```

The endpoint never runs verification.

## Runtime Console

Launchpad 3.0 adds an App Proof section.

It shows:

- proof status
- blueprint
- applied addons
- drift count
- last checked timestamp
- top drift rows
- next command

Console continues to use the existing Services, Modules, Operations, Runtime
Story, and Remote Calls pages for deeper evidence. Launchpad stays the first
screen.

## Agent Context

`lenso agent context` and `lenso agent task` include App Proof when
`.lenso/app-proof.json` exists.

Agent context should make these boundaries explicit:

- generated control-plane files may be repaired
- existing service source files are user code
- unknown services should not be deleted
- module and service manifests remain runtime contracts

This gives coding agents a smaller blast radius when working inside generated
apps.

## Examples And Site

`lenso-examples` adds a `support-desk-proof` fixture with:

- Launchpad state
- dev doctor state
- app proof state
- agent task output that includes App Proof

`lenso-site` updates:

- Product Blueprints
- CLI Reference
- Runtime Console
- Troubleshooting

Docs should state that App Proof is a local generated-state check, not a
security attestation and not a deployment gate.

## Success Criteria

- `lenso app verify` reports ready for a fresh `support-desk` app with
  `support-sla`.
- `lenso app verify --write-proof` writes `.lenso/app-proof.json`.
- `lenso app diff` detects a missing workspace service entry.
- `lenso app repair --dry-run` reports the planned workspace repair.
- `lenso app repair` restores safe generated state without overwriting service
  source files.
- `GET /admin/data/launchpad/proof` returns proof state or an empty next
  command.
- Runtime Console Launchpad shows proof status, drift count, and next command.
- `lenso agent task "..."` includes App Proof context when the proof file
  exists.

## Test Plan

- CLI parser tests for `app verify`, `app diff`, and `app repair`.
- Unit tests for proof status folding and drift detection.
- Fixture test for `support-desk + support-sla` proof output.
- Host admin-data test for missing and present proof state.
- Console model tests for proof summary and next command selection.
- Regression check that `app repair` does not overwrite existing service source
  files.

## Deliberate Deferrals

- Blueprint version migration is deferred until blueprint recipes need versioned
  changes.
- User-authored blueprints are deferred until built-in blueprints prove the
  lifecycle.
- Marketplace distribution is deferred until local proof catches real drift.
- Kubernetes integration remains optional and outside App Proof.
