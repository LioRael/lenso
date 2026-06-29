# Service Boundary

This note names the microservice-facing boundary that already exists in Lenso.
It is intentionally smaller than a general microservice platform.

## Definition

A service is an independently running backend process that provides one or more
Lenso modules. The host connects to the service through `REMOTE_MODULES`, reads
the service manifest, and then loads the modules declared inside it.

```text
service = service manifest + one or more module manifests + optional managed service process
module = business capability declared inside a service or linked into the host
```

The host may start the service process from `.lenso/module-services.json`, or it
may connect to an already-running service through `REMOTE_MODULES`. Either way,
the service is not a peer runtime; the host remains the control plane.

V11 keeps `lenso module install` as the user-facing module entrypoint. A module
release can now describe a `service`, `linked`, or `bundled` source. Service
releases resolve to provider service packages or manifests; linked releases
enable Rust code already available to the host. `lenso service install` is still
valid, but it means "connect this provider process", not "enable every module it
contains".

## Host Responsibilities

The host owns:

- service source configuration and manifest loading;
- caller auth, capability checks, request limits, and header policy;
- runtime queues, retries, outbox claims, story records, and technical
  operations;
- service startup from `.lenso/module-services.json` when `autoStart` is true;
- service diagnosis through the CLI and Runtime Console metadata.

For operator commands and status meanings, use
[`service-module-operator-runbook.md`](service-module-operator-runbook.md).

## Service Responsibilities

The service owns:

- its implementation language, process, storage, and deployment package;
- the service protocol endpoint;
- the modules it provides;
- declared module HTTP routes, admin surfaces, runtime functions, and event handlers;
- module-local authorization and validation as defense in depth.

It must not claim host runtime rows, consume host outbox rows directly, write
host Runtime Story tables, or receive browser bearer tokens.

## Growth Order

Grow this boundary in this order:

1. keep the protocol and manifest compatibility stable;
2. improve service health, doctor output, and operator visibility;
3. document linked-module extraction through
   [`linked-to-service-module.md`](linked-to-service-module.md);
4. make catalog install and uninstall safer;
5. add deployment examples for independently running services.

Defer service discovery, gateways, service mesh, distributed transactions,
schema registry, and orchestration until real extracted modules need them.
