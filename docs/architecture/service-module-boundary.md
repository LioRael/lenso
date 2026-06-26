# Service Module Boundary

This note names the microservice-facing boundary that already exists in Lenso.
It is intentionally smaller than a general microservice platform.

## Definition

A service module is a `Remote` module whose backend runs outside the host
process and is loaded through the same `ModuleManifest` contract as linked
modules.

```text
service module = ModuleManifest + remote protocol endpoint + optional managed service process
```

The host may start the module process from `.lenso/module-services.json`, or it
may connect to an already-running service through `REMOTE_MODULES`. Either way,
the module remains a Lenso module, not a peer runtime.

## Host Responsibilities

The host owns:

- module source configuration and manifest loading;
- caller auth, capability checks, request limits, and header policy;
- runtime queues, retries, outbox claims, story records, and technical
  operations;
- service startup from `.lenso/module-services.json` when `autoStart` is true;
- service diagnosis through the CLI and Runtime Console metadata.

For operator commands and status meanings, use
[`service-module-operator-runbook.md`](service-module-operator-runbook.md).

## Module Responsibilities

The service module owns:

- its implementation language, process, storage, and deployment package;
- the service module protocol endpoint;
- declared HTTP routes, admin surfaces, runtime functions, and event handlers;
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
5. add deployment examples for independently running service modules.

Defer service discovery, gateways, service mesh, distributed transactions,
schema registry, and orchestration until real extracted modules need them.
