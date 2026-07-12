# Service System Plane

V18 adds a system-level manifest for teams that have moved past one provider
or one module, but still want Lenso to stay Kubernetes-optional.

The system plane is declared in `lenso.system.json` with protocol
`lenso.system.v1`. This protocol is the legacy Provider-era System declaration;
its `services` collection contains Provider processes, not Autonomous Services.
It is not a new runtime and it does not replace module or Provider install
state. It is a planning and visibility contract that names:

- providers: independently running processes represented by the legacy
  `services` field;
- modules: business capabilities installed into the host or provided by a
  Provider;
- dependencies: capability edges between Modules or Providers;
- environments: local, staging, prod, or any operator-owned deployment lane.

`lenso.system.v2` is the mixed-topology successor. It keeps Provider-era
semantics intact while adding explicit Host, Provider, Autonomous Service,
Module, Workload, Producer, and Consumer kinds. Contract relationships carry
versioned artifact references and tenancy requirements. The graph projection
canonicalizes collections so logically equivalent source ordering produces
identical machine JSON.

System v2 validation reports stable codes and next actions for missing
ownership, unresolved references, ambiguous kinds, and incompatible Producer
and Consumer tenancy requirements. Public checks identify the protocol as
`lenso.system.v2` and its semantic kind as `mixed_system`; System v1 continues
to normalize to `provider_system` through its compatibility adapter.

## Boundary

Lenso keeps the legacy Provider/Module split explicit:

```text
Provider = Host-managed remote process, SDK, package, and v1 manifest
Module = business capability contract used by the Host and Runtime Console
System v1 = graph that explains how Providers and Modules form one product system
```

`lenso.service.json`, service packages, service workspaces, release plans, and
deployment state with protocol `lenso.service.v1` describe Providers and their
process lifecycle. They do not claim the data, runtime, lifecycle, or release
ownership of an Autonomous Service.
`lenso.module.json` and module releases still describe business capability
installation. `lenso.system.json` sits above both so operators can understand
which Provider exposes which Module and which capability edges cross process
boundaries.

The Host remains the control plane. A v1 System manifest must not give Providers
permission to write Host runtime tables, consume the Host Outbox, bypass
capability checks, or receive browser bearer tokens. Authentication, proxy
policy, retries, runtime queues, Outbox delivery, and Story evidence remain
Host-owned.

Neither System protocol is part of the Data Plane. Applications do not read a
System artifact to route requests, authorize callers, publish events, or
execute Workloads. It remains declarative input for checks, graphs, plans,
projections, and compatibility review.

The public `lenso-service` contract check reports both the detected protocol and
its semantic kind. It normalizes `lenso.service.v1` to `provider` and
`lenso.system.v1` to `provider_system` in a separate read model; it never
rewrites the source artifact. Unsupported or missing protocols return stable
machine-readable codes and a next action. Backend, CLI, and Runtime Console
consumers should use that shared result instead of inferring v1 semantics.

## Manifest Shape

```json
{
  "protocol": "lenso.system.v1",
  "name": "support-platform",
  "environments": ["local", "staging", "prod"],
  "services": [
    {
      "name": "support-suite-provider",
      "target": "operator",
      "modules": ["support-ticket"],
      "cwd": "services/support-suite-provider",
      "manifest": "http://127.0.0.1:4110/lenso/service/v1/manifest",
      "command": "pnpm start",
      "lang": "ts",
      "readyUrl": "http://127.0.0.1:4110/lenso/service/v1/status"
    }
  ],
  "modules": [
    {
      "name": "support-ticket",
      "installTo": "service:support-suite-provider",
      "capabilities": ["support_ticket.tickets.write"],
      "dependencies": ["auth"]
    },
    {
      "name": "auth",
      "installTo": "host",
      "capabilities": ["auth"]
    }
  ]
}
```

`target` is intentionally a string so local, external, Kubernetes, operator, or
future targets can be described before they become a first-class deployment
implementation.

## CLI Flow

Create and grow the manifest:

```sh
lenso system init support-platform --env local --env staging --env prod
lenso system add-service support-suite-provider \
  --target operator \
  --module support-ticket \
  --cwd services/support-suite-provider \
  --lang ts \
  --command "pnpm start" \
  --ready-url http://127.0.0.1:4110/lenso/service/v1/status \
  --manifest http://127.0.0.1:4110/lenso/service/v1/manifest
lenso system add-module support-ticket \
  --to service:support-suite-provider \
  --capability support_ticket.tickets.write \
  --dependency auth
lenso system add-module auth --to host --capability auth
```

Inspect it before wiring release and deployment automation:

```sh
lenso system graph --system-file lenso.system.json
lenso system plan --system-file lenso.system.json --check
lenso system diff --system-file lenso.system.json --check
lenso system apply --system-file lenso.system.json --dry-run
lenso system doctor --system-file lenso.system.json
lenso system release plan --env staging --output system-release-staging.json
lenso system release check system-release-staging.json
lenso system release apply system-release-staging.json
lenso system release promote --from staging --to prod --output system-release-prod.json
lenso system runbook generate system-release-staging.json --output system-runbook-staging.json
lenso system runbook record system-runbook-staging.json
```

`system plan` produces setup commands only for existing lower-level surfaces,
such as `lenso service workspace add` and `lenso service env add`. It does not
invent install commands that the host cannot execute yet.

V19 adds system drift and safe apply. `system diff` compares the graph to
host-local `.lenso` state: module installs, service-start state, service
environments, deployment observations, and service release records. `system
apply` writes only safe local files: `.lenso/module-services.json` and
`.lenso/service-environments.json`. It does not install modules, deploy to
Kubernetes, mutate an operator resource, or apply a service release.

V20 adds system release trains. `lenso.system-release.v1` records one
environment-scoped system change set: graph snapshot, affected services,
affected modules, drift precheck, policy result, rollback availability, and
next commands. Applying a system release writes only
`.lenso/system-releases.json`; service release plans, module installs, and
Kubernetes/operator deploys stay explicit commands.

V21 adds system runbooks. `lenso.system-runbook.v1` is generated from a system
release plan and turns the release into ordered operator steps: release policy
check, service release preparation, deployment evidence, and final release
recording. Recording a runbook writes only `.lenso/system-runbooks.json`.
Runbook JSON is a generated control-plane artifact, not a module authoring
surface.

## Console And API

The admin data API exposes:

```text
GET /admin/data/service-system
```

The endpoint reads `lenso.system.json`, returns `empty` when the file is
missing, `needs_attention` when it cannot be parsed or has unresolved graph
edges, and `ready` when the graph is coherent.

V19 also exposes:

```text
GET /admin/data/service-system/drift
```

The drift endpoint compares the declared system to host-local state and returns
`ready`, `drifted`, `needs_attention`, or `empty` with next commands for the
operator.

V20 also exposes:

```text
GET /admin/data/service-system/release-train
```

The release-train endpoint reads `.lenso/system-releases.json` and returns the
latest applied system releases plus the next promotion/history commands.

V21 also exposes:

```text
GET /admin/data/service-system/runbooks
```

The runbooks endpoint reads `.lenso/system-runbooks.json` and returns active or
recent runbooks, the current step, and the next operator commands.

Runtime Console uses the response on the Services page so operators can see the
system name, service count, module count, dependency count, environment lanes,
and graph issues beside provider lifecycle, deployment, release, Remote Calls,
Runtime Story, and Technical Operations evidence.

## Kubernetes Is Optional

The system plane can describe Kubernetes and operator-managed services, but it
does not require them. A host can use the same system manifest for:

- all-local service development;
- externally deployed services managed by another platform;
- reviewable Kubernetes manifests generated by the CLI;
- Lenso Operator custom resources in clusters that choose to adopt it.

That keeps the product story intact: embrace Kubernetes where it helps, while
keeping linked modules, local services, and externally managed services first
class.
