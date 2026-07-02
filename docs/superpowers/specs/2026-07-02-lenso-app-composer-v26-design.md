# Lenso App Composer V26 Design

## Goal

V26 turns the Launchpad, Product Blueprint, App Proof, and App Change Plan work
into a stronger app-building surface: **Lenso App Composer**.

The user should be able to start from a business-shaped app, compose several
addons at once, see whether required services are operational, and hand an
agent a bounded task pack without needing to understand every internal Lenso
file first.

V26 does three things together:

1. **App Composer:** compose blueprint plus addons into one app lifecycle plan.
2. **Service Ops in the app lifecycle:** show service readiness and next
   operator commands where the user is already deciding what to do next.
3. **Agent Module Studio:** make `lenso agent task` produce a richer handoff
   that knows the current app, services, modules, change plan, and boundaries.

## Product Positioning

This is still an **agent-ready modular app framework for Rust business systems**.

V26 should make Lenso feel much closer to "describe the business app, then keep
building safely" without claiming to be a full AI app generator, a service mesh,
or a Kubernetes-only platform.

The public story becomes:

```text
Choose a business blueprint.
Compose app capabilities.
Run or connect services.
Inspect the app lifecycle.
Hand a scoped task to an agent.
Apply only generated-state changes that Lenso can prove are safe.
```

## Current Inputs

V26 builds on the existing surfaces:

- Launchpad app state in `.lenso/launchpad.json`.
- Dev Doctor state in `.lenso/dev-doctor.json`.
- App Proof state in `.lenso/app-proof.json`.
- App Change Plan state in `.lenso/app-change-plan.json`.
- Service workspace state in `lenso.workspace.json`.
- Service system state in `lenso.system.json`.
- Service lifecycle state in `.lenso/module-services.json`.
- Service deployment and release state in `.lenso/service-deployments.json` and
  `.lenso/service-releases.json`.
- Runtime Console Launchpad and Services pages.
- Existing public skills for start, business planning, module authoring, service
  authoring, and API clients.

## Non-Goals

V26 does not add:

- a remote blueprint registry
- blueprint signing
- marketplace policy
- service mesh
- required Kubernetes deployment
- service discovery
- distributed transactions
- schema registry
- a new built-in AI runtime
- browser-side apply or repair
- source-code overwrites for generated services
- automatic migration from linked modules to services

Kubernetes remains supported where existing delivery surfaces support it, but it
is not required for App Composer.

## Concepts

### Service

A Service is an independently running process or provider that Lenso connects to
through explicit manifests, lifecycle metadata, commands, readiness URLs, and
operator evidence.

### Module

A Module is an installable business capability. A module can be linked into the
host or provided by a service. Module installation stays valid for module-owned
capabilities; service installation is for out-of-process service providers.

This distinction matters in Composer output. It must say whether the next action
is a module install, a service install, a service start, or a generated app
state change.

### Blueprint

A Blueprint is a built-in app archetype, such as `support-desk`. It remains
curated in the CLI for this version.

### Addon

An Addon is a built-in app-level capability that extends a blueprint with
generated services, modules, dependencies, commands, or recommendations.

Addons are not a marketplace and not a user-authored DSL in V26.

### Composition Request

A Composition Request is the user's requested app shape:

```text
blueprint + zero or more addons + optional task text + optional apply mode
```

It can target a new app directory or an existing Launchpad app.

### App Change Plan With Composition

V26 reuses `.lenso/app-change-plan.json`. Composer should add an optional
`composition` block instead of creating another top-level JSON file.

Existing readers that only understand V25 can ignore the new block. V26 readers
can use it to explain requested addons, service readiness, module/service
install recommendations, and agent handoff context.

Example:

```json
{
  "protocol": "lenso.app-change-plan.v1",
  "status": "changes",
  "projectName": "acme-support",
  "blueprint": "support-desk",
  "addons": ["support-sla", "customer-profile"],
  "changes": [],
  "blocked": [],
  "nextCommand": "lenso app apply .lenso/app-change-plan.json",
  "composition": {
    "protocol": "lenso.app-composition.v1",
    "intent": "support-desk app with SLA and customer profile",
    "requestedAddons": ["support-sla", "customer-profile"],
    "appliedAddons": ["support-sla"],
    "pendingAddons": ["customer-profile"],
    "serviceActions": [
      {
        "id": "service:start:customer-profile-service",
        "kind": "service_start",
        "label": "Start customer-profile-service",
        "command": "lenso service start customer-profile-service api",
        "status": "recommended"
      }
    ],
    "agentActions": [
      {
        "id": "agent:task:customer-profile",
        "label": "Generate agent task pack for customer profile",
        "command": "lenso agent task --from-app-plan \"add customer profile lookup\""
      }
    ]
  }
}
```

## User Flows

### New App

```sh
lenso app compose ./acme-support \
  --blueprint support-desk \
  --addon support-sla \
  --addon customer-profile \
  --apply

cd ./acme-support
lenso dev doctor --live --write-state
lenso app verify --write-proof
lenso app next
lenso agent task --from-app-plan "add enterprise SLA escalation"
```

`--apply` creates the generated app and applies safe generated control-plane
state. It must not overwrite user source code.

### Existing App

```sh
lenso app compose \
  --repo-root ./acme-support \
  --addon notifications \
  --write-plan

lenso app explain --repo-root ./acme-support
lenso app apply ./acme-support/.lenso/app-change-plan.json
lenso app verify --repo-root ./acme-support --write-proof
```

Existing apps default to plan-first behavior. `--apply` is explicit.

### Operator Recovery

```sh
lenso app next --repo-root ./acme-support
```

The output should answer:

- Is there a Launchpad app?
- Is generated app state clean?
- Is an App Change Plan waiting?
- Are services configured?
- Are services started and ready?
- Is a host restart needed?
- What command should I run next?

### Agent Handoff

```sh
lenso agent task --repo-root ./acme-support --from-app-plan \
  "add customer priority scoring to support tickets"
```

The output should include:

- app blueprint and addons
- current services and modules
- service/module boundary rules
- app proof status
- change-plan status
- service readiness summary
- recommended files and commands
- the user's task text

It must not instruct an agent to loosen host-owned auth, runtime queues, retry,
outbox, Runtime Story, or Technical Operations boundaries.

## CLI Surface

### `lenso app compose`

Primary user-facing Composer command.

```sh
lenso app compose ./acme-support --blueprint support-desk --addon support-sla --apply
lenso app compose --repo-root ./acme-support --addon notifications --write-plan
lenso app compose --repo-root ./acme-support --addon customer-profile --explain
```

Rules:

- A new app requires a directory argument.
- An existing app uses `--repo-root`.
- Addons can be repeated.
- `--write-plan` writes `.lenso/app-change-plan.json`.
- `--apply` applies only safe generated control-plane changes.
- `--explain` prints the plan and recommended commands without writing.
- Unsupported addon combinations become blocked plan items.
- Re-requesting an already applied addon is idempotent.

### `lenso app next`

Operator-friendly summary command.

```sh
lenso app next
lenso app next --repo-root ./acme-support
lenso app next --repo-root ./acme-support --live
```

Rules:

- Without `--live`, read local state files only.
- With `--live`, run bounded readiness probes using existing doctor/service
  check behavior.
- Print one primary next action first.
- Then print supporting evidence grouped by app, change plan, proof, and
  services.

### `lenso app explain`

Human-readable explanation of the current app lifecycle.

```sh
lenso app explain
lenso app explain --repo-root ./acme-support
```

Rules:

- Explain why the next command was chosen.
- Explain what Composer will and will not change.
- Explain module vs service actions separately.
- Avoid raw internal jargon when an operator phrase is clearer.

### `lenso agent task --from-app-plan`

Extends the existing agent handoff. It should read the current App Change Plan
and include the composition block when present.

```sh
lenso agent task --from-app-plan "add SLA escalation"
lenso agent task --for-module support-ticket "add private notes"
```

`--for-module` scopes the handoff to one module when the module can be found in
Launchpad, workspace, or service manifests.

## Service Ops Behavior

Composer should summarize service state from existing files and commands before
inventing new service machinery.

States:

- `not_configured`: no service workspace or service provider state exists
- `configured`: service exists in workspace or module-services state
- `ready`: service has a passing readiness check
- `not_ready`: service is configured but readiness failed
- `restart_pending`: host-facing generated state changed after the last proof
- `unknown`: state exists but is incomplete or unreadable

Recommended commands:

- missing Launchpad state -> `lenso app create ...`
- missing dev doctor -> `lenso dev doctor --write-state`
- service not configured -> module/service install command
- service configured but stopped -> `lenso service start <provider> <service>`
- service not ready -> `lenso service status <provider> <service>`
- generated drift -> `lenso app plan --write-plan`
- pending safe changes -> `lenso app apply .lenso/app-change-plan.json`
- proof stale -> `lenso app verify --write-proof`

## Runtime Console

Console should keep `/launchpad` as the route, but position it as **App
Lifecycle** in copy and layout.

It should show:

- blueprint and applied addons
- pending composition request
- App Change Plan status
- App Proof status
- Dev Doctor status
- Service Ops summary
- primary next command
- link to Services when a service action is recommended

Console remains read-only for generated-state apply. It can copy commands and
link to evidence; it does not mutate app state.

## Examples

Add a fixture that proves the full V26 story:

```text
lenso-examples/fixtures/launchpad/support-desk-composer/
```

The fixture should include:

- `launchpad.json`
- `dev-doctor.json`
- `app-proof.json`
- `app-change-plan.json`
- `agent-task.md`
- a short README command transcript

The example should use a real business line:

```text
support-desk + support-sla + customer-profile
```

## Skills And Docs

Update the public skills so agents choose the new entry path:

- `lenso-start`: prefer `lenso app compose` for new app work.
- `lenso-business-planning`: map business prompts to blueprint plus addon
  choices and then generate an agent handoff.
- `lenso-module-authoring`: consume Composer context and honor module scope.
- `lenso-remote-module-authoring`: describe service-provided modules through
  service install and readiness commands.
- `lenso-starter-host`: point existing host app work at `app next`,
  `app explain`, and App Proof.

Update site docs:

- product blueprints
- quickstart
- CLI reference
- Runtime Console
- agent development
- troubleshooting

## Error Handling

Composer should block, not guess, when:

- Launchpad state is missing for an existing app.
- The requested addon is not supported by the blueprint.
- Two addons declare conflicting service names.
- A generated service directory exists with unexpected user files.
- A module/service install recommendation cannot be tied to a known manifest.
- App Proof is failed and the plan depends on generated state.
- Service state files are unreadable.

Blocked output must include:

- short reason
- affected file or service
- safe next command when known

## Ownership Boundaries

V26 must keep these boundaries:

- CLI owns generation, composition, apply, explain, and agent handoff.
- Host and Console read evidence and expose state.
- Runtime queues, retries, outbox, Runtime Story, Technical Operations, and auth
  stay Host-owned.
- Services do not receive browser bearer tokens.
- Services do not write Host runtime tables.
- Modules stay installable capabilities.
- Services stay out-of-process providers.

## Success Criteria

V26 is done when:

- A user can run one Composer command for `support-desk + support-sla +
  customer-profile`.
- Existing apps can plan multiple addons at once.
- `lenso app next` chooses a useful next command from app, proof, plan, and
  service state.
- Console shows the same lifecycle story without applying changes.
- `lenso agent task --from-app-plan` produces a useful scoped handoff.
- Module install and service install are described as separate actions.
- Kubernetes remains optional.

## Testing Strategy

Use focused verification:

- CLI unit tests for parser, composition planning, blocked addon conflicts,
  idempotent existing addons, and next-action ordering.
- CLI integration-style tempdir tests for a generated support-desk app with two
  addons.
- Host admin-data tests only if response DTOs or routes change.
- Console model tests for lifecycle summaries and service recommendations.
- Example fixture checks for the V26 support-desk composer fixture.
- Site and skill checks for command references and no stale "remote module"
  entry wording in the new app path.

Do not add broad smoke runs unless the implementation changes process startup,
HTTP routing, or generated contracts.

## Rollout

Implement V26 in this order:

1. Ensure App Change Plan CLI support exists in `lenso-cli`.
2. Add App Composer plan/apply/explain/next commands.
3. Add service-aware next-action summaries.
4. Extend agent handoff with composition and module scope.
5. Update Console App Lifecycle read-only display.
6. Add example fixture.
7. Update docs and skills.

