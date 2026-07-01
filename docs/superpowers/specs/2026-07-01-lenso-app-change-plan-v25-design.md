# Lenso App Change Plan V25 Design

## Goal

V25 turns Launchpad apps from one-time generated scaffolds into maintained
generated apps.

V24 answers:

```sh
lenso app verify --write-proof
lenso app diff
lenso app repair --dry-run
```

V25 answers:

```sh
lenso app plan --write-plan
lenso app upgrade --check
lenso app apply .lenso/app-change-plan.json
lenso app verify --write-proof
```

The product story becomes: create a service-ready app, evolve it safely, and
prove generated state stays clean.

## Positioning

This is **Generated App Lifecycle**, not a blueprint marketplace.

Blueprints and addons stay built into the CLI for this version. The feature
compares the current generated app state against the current built-in recipe and
produces an operator-readable plan. It does not re-run `app create`, overwrite
business code, or delete unknown services.

## Non-Goals

V25 does not add:

- remote blueprint registries
- blueprint signing or trust policy
- marketplace install flow
- user-authored blueprint DSL
- service mesh or service discovery
- Kubernetes requirement
- service source rewriting
- automatic deletion of unknown services
- distributed migration framework

## Concepts

### App Change Plan

`.lenso/app-change-plan.json` is a generated plan for safe app-level changes.

It records:

- protocol
- project name
- blueprint
- applied addons
- generated timestamp
- current proof status
- status
- safe changes
- blocked changes
- next command

Statuses:

- `ready`: no changes needed
- `changes`: safe generated changes are available
- `blocked`: at least one requested change is unsafe or ambiguous
- `failed`: required state could not be read
- `empty`: no Launchpad app state exists

Example:

```json
{
  "protocol": "lenso.app-change-plan.v1",
  "status": "changes",
  "generatedAtUnixMs": 1782909000000,
  "projectName": "acme-support",
  "blueprint": "support-desk",
  "addons": ["support-sla"],
  "proofStatus": "ready",
  "changes": [
    {
      "id": "workspace-service-support-sla",
      "kind": "workspace_service_add",
      "name": "support-sla",
      "action": "write",
      "safe": true,
      "message": "Add support-sla to lenso.workspace.json.",
      "command": null
    }
  ],
  "blocked": [],
  "nextCommand": "lenso app apply .lenso/app-change-plan.json"
}
```

### Safe Changes

Safe changes are limited to generated control-plane state:

- `.lenso/launchpad.json`
- `lenso.workspace.json`
- `lenso.system.json`
- missing generated service scaffold directories

Existing service source files are user code. V25 must not overwrite them.
Unknown services are preserved.

### Blocked Changes

A change is blocked when the CLI cannot prove it is safe:

- a generated service directory already exists with unexpected files
- a generated entry conflicts with a user-edited owner, port, command, or module
- a requested addon is not supported by the app blueprint
- the current app has no readable Launchpad state
- App Proof is failed and the plan depends on proof-backed state

Blocked changes should include the exact reason and the next command when one is
known.

## CLI

### `lenso app plan`

Reads the generated app state and compares it with the current built-in
blueprint plus applied addons.

Options:

```sh
lenso app plan
lenso app plan --write-plan
lenso app plan --addon support-sla --write-plan
lenso app plan --repo-root <path>
```

Behavior:

- without `--write-plan`, print the plan summary
- with `--write-plan`, write `.lenso/app-change-plan.json`
- if no changes exist, print `App is up to date`
- if blocked changes exist, exit non-zero

### `lenso app upgrade`

Operator-friendly alias for checking app lifecycle state.

Options:

```sh
lenso app upgrade --check
lenso app upgrade --write-plan
lenso app upgrade --repo-root <path>
```

Behavior:

- `--check` exits non-zero when safe or blocked changes are pending
- `--write-plan` writes the same file as `app plan --write-plan`
- the command text should point users back to `lenso app apply`

### `lenso app apply`

Applies a previously generated plan.

Options:

```sh
lenso app apply .lenso/app-change-plan.json
lenso app apply .lenso/app-change-plan.json --dry-run
lenso app apply .lenso/app-change-plan.json --repo-root <path>
```

Behavior:

- reject plans with blocked changes
- reject unknown protocol versions
- reject plans for a different project or blueprint
- apply only safe generated changes
- after apply, print `lenso app verify --write-proof` as the next command

## Inputs

The planner reads:

- `.lenso/launchpad.json`
- `.lenso/dev-doctor.json`
- `.lenso/app-proof.json`
- `lenso.system.json`
- `lenso.workspace.json`
- built-in blueprint and addon recipes

The Host does not generate plans. Runtime generation stays in the CLI.

## Host Admin Data

Add one read-only endpoint:

```text
GET /admin/data/launchpad/change-plan
```

It reads `.lenso/app-change-plan.json` and returns:

- `ready`
- `changes`
- `blocked`
- `failed`
- `empty`

Missing file returns `empty` with:

```sh
lenso app plan --write-plan
```

The endpoint must not infer or apply changes.

## Runtime Console

Launchpad adds a compact **App Lifecycle** panel.

It shows:

- blueprint
- addons
- proof status
- change-plan status
- safe change count
- blocked change count
- next command

States:

- no plan: show `lenso app plan --write-plan`
- ready: show `App is up to date`
- changes: show `lenso app apply .lenso/app-change-plan.json`
- blocked: show the first blocked reason and `lenso app plan`
- proof stale after apply: show `lenso app verify --write-proof`

The panel links lifecycle state to App Proof, but does not duplicate the full
proof table.

## Agent Handoff

`lenso agent context` and `lenso agent task` include App Change Plan when the
file exists.

The handoff must say:

- generated control-plane files may be planned and applied
- existing service source files are user code
- unknown services should not be deleted
- run `lenso app verify --write-proof` after applying a plan

## Examples

Add a fixture:

```text
fixtures/launchpad/support-desk-change-plan/
```

It should include:

- `launchpad.json`
- `dev-doctor.json`
- `app-proof.json`
- `app-change-plan.json`
- `agent-task.md`

The fixture check should assert:

- protocol is `lenso.app-change-plan.v1`
- status is either `ready` or `changes`
- plan references `support-desk`
- agent task includes App Change Plan context

## Docs

Update:

- Product Blueprints: create, add, plan, apply, verify
- CLI Reference: `app plan`, `app upgrade`, `app apply`
- Runtime Console: App Lifecycle panel
- Troubleshooting: blocked plan, stale proof, unknown service preservation

## Success Criteria

- `lenso app plan --write-plan` writes `.lenso/app-change-plan.json`.
- `lenso app upgrade --check` fails when pending app changes exist.
- `lenso app apply <plan>` applies only safe generated changes.
- Applying a plan never overwrites existing service source files.
- Unknown services remain untouched.
- `GET /admin/data/launchpad/change-plan` returns plan state or empty state.
- Runtime Console Launchpad shows lifecycle status and next command.
- Agent handoff includes App Change Plan boundaries.
- Existing App Proof, Launchpad, linked modules, and service module flows keep
  working.

## Test Plan

- CLI parser tests for `app plan`, `app upgrade`, and `app apply`.
- Unit tests for ready, changes, blocked, failed, and empty plan status.
- CLI fixture test for a support-desk app with `support-sla`.
- Apply dry-run test proving service source files are not overwritten.
- Host admin-data test for missing and readable change-plan files.
- OpenAPI generated-check after adding the Host endpoint.
- Console model/data tests for lifecycle summary and fetch path.
- Console `pnpm check`.
- Examples fixture check.
- Site docs `pnpm types:check` and `pnpm lint`.

## Deferred

- Remote blueprint catalogs
- Blueprint version compatibility negotiation
- Signed plans
- Multi-plan history
- Generated source migrations
- Kubernetes-aware app planning
- Automatic PR creation
