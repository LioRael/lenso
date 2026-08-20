# Service Boundary

> **Legacy v0.3.x architecture:** This page describes the maintained
> Service-oriented implementation and is not normative for vNext. Read
> [lenso-vnext.md](lenso-vnext.md) for vNext decisions.

This note names the microservice-facing boundary that already exists in Lenso.
It is intentionally smaller than a general microservice platform.

## Definition

A Service is an independently running backend that owns and provides one or
more Lenso Modules. The Host selects Modules from the reviewed Application
Module Lock and connects only to the exact Provider Service installations that
own those selected exports.

```text
Service = Service manifest + one or more Module manifests + owned runtime
Module = business capability delivered Linked or exported by one Service
```

Provider Services are activated from the target-owned Service Installation Set.
Autonomous Services retain their own runtime and Service Store while
participating through the System Plane. In both modes, a Service contains
Modules; it is never represented as a Remote Module.

Module Ecosystem V1 has exactly two delivery forms. `linked` is the primary,
feature-rich in-process Module experience. `service` binds a Module Release to
one exact Service Release and export. There is no public `remote`, `source`, or
`bundled` Module model. Installing a Service does not implicitly enable every
Module it exports.

## Host Responsibilities

The Host owns:

- exact Module selection from the Application Module Lock;
- Provider endpoint and identity policy from the Service Installation Set;
- target-owned endpoint and credential resolver adapters injected through
  `HostComposition`;
- verification of the live Provider descriptor against locked artifacts;
- caller auth, capability checks, request limits, and header policy;
- runtime queues, retries, outbox claims, story records, and technical
  operations;
- service diagnosis through the CLI and Console metadata.

## Provider Runtime Authority

`lenso.provider-runtime-plan.v1` is the sole input to Provider transport
adapters. `lenso-module-management` compiles it from three reviewed artifacts:

1. the Application Module Lock selects each exact Module Release and export;
2. the retained Module Planning Context supplies the immutable Module Release
   and canonical Manifest bytes for that exact digest;
3. the environment's Service Installation Set supplies the exact Service
   Release, installed export, endpoint binding, and identity policy.

The compiler checks all identities and digests, excludes unselected sibling
exports, and keeps Autonomous Service Modules out of the Provider plan. A live
Provider descriptor may confirm that the running endpoint matches this plan;
it cannot add a Module, replace a Manifest, choose a release, or widen a
contract. Any mismatch is a startup error with stable operator evidence.
Endpoint adapters return candidates only; they cannot widen the plan's allowed
transport bindings or change Service and Module identity. Credential references
remain opaque until the adapter selected by the exact trust profile resolves
them in memory. Raw credentials must not appear in plans, diagnostics, or Debug
output.

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
