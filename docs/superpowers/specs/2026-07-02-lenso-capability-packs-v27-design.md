# Lenso Capability Packs V27 Design

## Goal

V27 turns Composer from a built-in blueprint/addon path into a team-owned
capability authoring path.

The user should be able to create a reusable local capability pack, validate it,
compose it into a Launchpad app, see its lifecycle in Console, and hand an agent
a bounded task without turning the pack into a marketplace package or a new
runtime.

The headline is:

```text
Compose official blueprints, then grow your own reusable capability packs.
```

## Product Positioning

Lenso remains an agent-ready modular app framework for Rust business systems.

V27 strengthens the product in three directions:

1. **Capability Pack Authoring:** make a local team-owned business capability
   packageable without publishing it.
2. **Composer Integration:** let App Composer consume local packs beside built-in
   addons.
3. **Agent and Console Workbench:** make packs visible, checkable, and usable as
   scoped agent handoff targets.

This is not a marketplace release. It is the local reuse layer that should exist
before remote registries, signatures, policy, or ecosystem distribution.

## Concepts

### Capability Pack

A Capability Pack is a local bundle that describes one reusable business
capability and points to its module, service, console, and agent assets.

It is not a new runtime source. It is a small authoring manifest that helps the
CLI compose existing Lenso primitives.

### Module

A Module remains an installable business capability. It can be linked into the
host or provided by a service.

### Service

A Service remains an out-of-process provider. A pack may include a service
manifest, ready/status hints, and package metadata, but it does not make the
service a peer runtime.

### Pack Manifest

Each pack has one `lenso.capability.json`:

```json
{
  "protocol": "lenso.capability-pack.v1",
  "name": "support-sla",
  "label": "Support SLA",
  "summary": "Adds SLA tracking to support tickets.",
  "supports": {
    "blueprints": ["support-desk"]
  },
  "modules": [
    {
      "name": "support-sla",
      "manifest": "module/lenso.module.json"
    }
  ],
  "services": [
    {
      "provider": "support-sla-provider",
      "service": "api",
      "language": "ts",
      "manifest": "service/lenso.service.json"
    }
  ],
  "agent": {
    "defaultTask": "add or change Support SLA behavior"
  }
}
```

The manifest stays intentionally small. It links files that already have their
own contracts instead of duplicating those contracts.

## User Flows

### Create A Pack

```sh
lenso capability init support-sla \
  --dir ./capabilities/support-sla \
  --lang ts \
  --for-blueprint support-desk

lenso capability check ./capabilities/support-sla
lenso capability inspect ./capabilities/support-sla
```

`init` creates the pack manifest and minimal README. It should reuse existing
service/module scaffolding helpers where they already exist, but V27 does not
need a separate template engine.

### Compose A Pack Into An App

```sh
lenso app compose ./acme-support \
  --blueprint support-desk \
  --pack ./capabilities/support-sla \
  --apply

cd ./acme-support
lenso app next
lenso agent task --for-capability support-sla \
  "add enterprise SLA escalation"
```

Pack composition writes through the existing App Change Plan. It must not
overwrite user source files.

### Existing App

```sh
lenso app compose \
  --repo-root ./acme-support \
  --pack ./capabilities/support-sla \
  --write-plan

lenso app explain --repo-root ./acme-support
lenso app apply ./acme-support/.lenso/app-change-plan.json
```

Existing apps stay plan-first. `--apply` remains explicit.

## CLI Surface

### `lenso capability init`

```sh
lenso capability init support-sla --dir ./capabilities/support-sla --lang ts
lenso capability init customer-profile --dir ./capabilities/customer-profile --lang rust
```

Rules:

- `--lang` supports `rust` and `ts`.
- `--for-blueprint` can be repeated.
- The output directory must not contain conflicting generated files.
- The command creates `lenso.capability.json` and `README.md`.
- If service/module scaffolding is already available in the CLI, reuse it.
  Otherwise print the next command that creates the missing asset.

### `lenso capability check`

```sh
lenso capability check ./capabilities/support-sla
lenso capability check ./capabilities/support-sla --json
```

Checks:

- manifest exists
- protocol is `lenso.capability-pack.v1`
- name is a stable slug
- referenced module manifests exist
- referenced service manifests exist
- referenced paths stay inside the pack directory
- blueprint support is declared
- duplicate module, service, or provider names are blocked
- service language is `rust`, `ts`, or absent

### `lenso capability inspect`

```sh
lenso capability inspect ./capabilities/support-sla
```

Prints pack name, supported blueprints, modules, services, agent task hint, and
recommended composer command.

### `lenso app compose --pack`

```sh
lenso app compose ./acme-support \
  --blueprint support-desk \
  --pack ./capabilities/support-sla \
  --addon customer-profile \
  --apply
```

Rules:

- `--pack` can be repeated.
- Packs and built-in addons can be composed together.
- A pack must support the target blueprint unless the app has no blueprint.
- A pack that is already applied is idempotent.
- Conflicting service or module names become blocked plan items.
- Pack state is written into `.lenso/app-change-plan.json` under the existing
  `composition` block.

### `lenso agent task --for-capability`

```sh
lenso agent task --for-capability support-sla \
  "add enterprise SLA escalation"
```

The handoff includes:

- pack manifest summary
- module and service assets
- service readiness and next commands when known
- App Change Plan status
- module/service/host-owned boundary rules
- requested task text

## App Change Plan Extension

V27 keeps `.lenso/app-change-plan.json` as the only app lifecycle plan file.

Composer adds optional pack state inside `composition`:

```json
{
  "composition": {
    "protocol": "lenso.app-composition.v1",
    "requestedAddons": ["customer-profile"],
    "requestedPacks": ["support-sla"],
    "appliedPacks": [],
    "pendingPacks": ["support-sla"],
    "capabilityPacks": [
      {
        "name": "support-sla",
        "path": "../capabilities/support-sla",
        "status": "pending",
        "modules": ["support-sla"],
        "services": ["support-sla-provider/api"],
        "nextCommand": "lenso capability check ../capabilities/support-sla"
      }
    ]
  }
}
```

Older readers can ignore the new fields.

## Runtime Console

Console keeps the `/launchpad` route and App Lifecycle positioning.

It adds a compact Capability Packs section:

- requested packs
- pending packs
- failed pack checks
- first recommended pack command
- link to Services when a pack service action exists
- agent handoff command when available

Console remains read-only. It can show and copy commands, but it does not apply
packs, mutate source files, or install services.

## Examples

Add one fixture:

```text
lenso-examples/fixtures/capabilities/support-sla-pack/
```

The fixture should include:

- `lenso.capability.json`
- `README.md`
- sample module manifest
- sample service manifest
- sample App Change Plan after composing the pack
- sample agent task output

The fixture proves:

```text
support-desk + local support-sla capability pack
```

## Skills And Docs

Update public skills so agents choose the right path:

- `lenso-start`: use Composer for apps, Capability Packs for reusable team
  capabilities.
- `lenso-business-planning`: map business prompts to built-in addons or local
  packs.
- `lenso-module-authoring`: consume pack-scoped context when the requested work
  is inside one module.
- `lenso-remote-module-authoring`: describe service-provided pack modules with
  service readiness and manifest checks.
- `lenso-starter-host`: use `app next`, `app explain`, and pack checks for
  existing app work.

Update site docs:

- quickstart
- product blueprints
- CLI reference
- Runtime Console
- agent development
- troubleshooting

## Error Handling

Composer blocks instead of guessing when:

- `lenso.capability.json` is missing
- pack protocol is unknown
- pack path escapes the pack root
- pack does not support the app blueprint
- referenced module or service manifest is missing
- a pack conflicts with an existing service, provider, or module name
- a pack check fails and the plan depends on it
- App Proof is failed and the pack would mutate generated app state

Blocked output must include:

- reason
- affected pack path
- safe next command when known

## Ownership Boundaries

V27 keeps these boundaries:

- CLI owns pack authoring, checking, composition, explain, and agent handoff.
- Host and Console read pack evidence and expose state.
- Module install remains the business-capability install path.
- Service install remains the provider/process install path.
- Capability Pack is local authoring metadata, not a new runtime source.
- Services do not receive browser bearer tokens.
- Services do not write Host runtime tables.
- Runtime queues, retries, Outbox, Runtime Story, Technical Operations, and auth
  stay Host-owned.
- Kubernetes remains optional.

## Non-Goals

V27 does not add:

- remote capability registry
- package signing
- trust policy
- marketplace submission
- dependency solver
- service mesh
- API gateway
- required Kubernetes path
- distributed transactions
- schema registry
- automatic source migrations
- browser-side apply

## Success Criteria

V27 is done when:

- a user can create a TS or Rust local capability pack
- `lenso capability check` catches broken pack manifests
- `lenso app compose --pack` writes pack-aware App Change Plans
- `lenso app next` and `app explain` surface pack next actions
- Console shows pack lifecycle state without applying changes
- `lenso agent task --for-capability` produces a scoped handoff
- examples prove `support-desk + support-sla-pack`
- module install and service install remain separate
- Kubernetes remains optional

## Testing Strategy

Use focused checks:

- CLI parser tests for `capability` commands, `app compose --pack`, and
  `agent task --for-capability`.
- CLI unit tests for pack manifest validation, duplicate detection, path escape
  blocking, blueprint support checks, and idempotent pack composition.
- CLI tempdir test for composing `support-desk` with a local `support-sla` pack.
- Host admin-data tests only for changed DTO pass-through.
- Console model tests for pack lifecycle summaries.
- Example fixture check for the support SLA pack.
- Site and skill reference checks for `lenso capability` and `--pack`.

Do not add broad smoke runs unless the implementation changes process startup,
HTTP routing, or generated contracts.

## Rollout

Implement V27 in this order:

1. Add Capability Pack CLI model and `init/check/inspect`.
2. Add `app compose --pack` and pack-aware App Change Plan composition.
3. Add `agent task --for-capability`.
4. Pass pack composition through Host admin-data when DTOs change.
5. Add Console App Lifecycle pack summary.
6. Add examples fixture.
7. Update docs and skills.
