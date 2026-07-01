# Lenso Launchpad V22 Design

## Goal

Make the first Lenso experience feel useful in minutes: create a real support-desk app, run it locally, see Launchpad status in Console, and hand an agent enough context to keep building.

## Product Shape

V22 adds **Lenso Launchpad**, a generated-first onboarding plane. It does not replace modules, services, system manifests, release trains, or runbooks. It sits above them and gives a new user one path:

```sh
lenso app create support-desk --blueprint support-desk
cd support-desk
lenso dev up
lenso agent context
```

The output is a host app with a TypeScript service, a Rust service, a service workspace, a service system manifest, Launchpad state, and agent-readable context.

## Boundaries

Launchpad JSON is generated control-plane state. Module authors do not hand-maintain it.

V22 does not add:

- background daemon supervision
- Docker or Kubernetes as a requirement
- AI provider integration
- marketplace behavior
- workflow DSL
- new module authoring JSON

`lenso dev up` is a foreground local development command. `lenso dev status` explains what is configured. `lenso dev stop` explains that foreground processes stop with Ctrl-C.

## CLI

New commands:

```sh
lenso app create <dir> --blueprint support-desk
lenso dev up
lenso dev status
lenso dev stop
lenso agent context
lenso agent task "add SLA escalation to support-ticket"
```

`app create` reuses existing host and service scaffolds. The support-desk blueprint generates:

- host project
- `services/support-api` TypeScript service
- `services/notification-worker` Rust service
- `lenso.workspace.json`
- `lenso.system.json`
- `.lenso/launchpad.json`

`agent context` reads the generated host state and emits Markdown with the system graph, service workspace, launchpad commands, testing commands, and host-owned runtime boundaries.

## Host API

Runtime Console needs one small endpoint:

```text
GET /admin/data/launchpad
```

It reads `.lenso/launchpad.json`. Missing state returns `empty` with the next command.

## Runtime Console

Console adds a Launchpad first-screen route:

```text
/launchpad
```

It shows:

- project name
- blueprint
- status
- service count
- module count
- next command
- launch checklist

This is a summary page, not a replacement for Services, Modules, Runtime Story, or Remote Calls.

## Examples And Site

`lenso-examples` records the support-desk Launchpad fixture and an example agent context output.

`lenso-site` moves the first quickstart toward the Launchpad path and keeps Kubernetes optional.

## Success Criteria

- `lenso app create support-desk --blueprint support-desk` creates a host app with TS and Rust services.
- `lenso dev status` works inside the generated app.
- `lenso agent context` emits useful Markdown without contacting an LLM.
- Console can render Launchpad state from the host API or mock data.
- Docs show the 10-minute path before deeper architecture.
