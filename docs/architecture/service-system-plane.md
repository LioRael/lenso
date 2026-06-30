# Service System Plane

V18 adds a system-level manifest for teams that have moved past one provider
or one module, but still want Lenso to stay Kubernetes-optional.

The system plane is declared in `lenso.system.json` with protocol
`lenso.system.v1`. It is not a new runtime and it does not replace module or
service install state. It is a planning and visibility contract that names:

- services: independently running provider processes;
- modules: business capabilities installed into the host or provided by a
  service;
- dependencies: capability edges between modules or services;
- environments: local, staging, prod, or any operator-owned deployment lane.

## Boundary

Lenso keeps the service/module split explicit:

```text
service = remote process, SDK, deployment package, and service manifest
module = business capability contract used by the host and Console
system = graph that explains how services and modules form one product system
```

`lenso.service.json`, service packages, service workspaces, release plans, and
deployment state still describe providers and their process lifecycle.
`lenso.module.json` and module releases still describe business capability
installation. `lenso.system.json` sits above both so operators can understand
which service owns which module and which capability edges cross service
boundaries.

The host remains the control plane. A system manifest must not give services
permission to write host runtime tables, consume the host outbox, bypass
capability checks, or receive browser bearer tokens.

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
