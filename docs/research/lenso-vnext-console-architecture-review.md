# Lenso vNext Console Architecture Review

Date: 2026-08-20

## Question

Should vNext Console run inside every target App, remain an independent service,
or become an independent Lenso App composed from ordinary Modules? This note
uses the current Console and framework source as evidence. It identifies deep
seams, but it does not make the product decision.

## Decision outcome

ADR [0044](../adr/0044-run-console-as-an-independent-lenso-app.md) subsequently
initially selected the independent Lenso App shape with a thin optional target
Connector. ADR
[0060](../adr/0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md)
later superseded that universal production rule: a target-owned App Web UI is
an ordinary in-App composition, while a genuinely cross-App Console remains an
independent Lenso App. A separate non-Lenso implementation remains an escape
hatch rather than the official architecture.

## Executive findings

1. The current Console is already an independently deployed Lenso application,
   not merely a frontend. Its fixed composition contains Shell, Auth, Password
   Auth, Organization, System Registry, Console Access, Surface Gateway, and
   Story Modules ([composition.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/composition.rs#L68)).
2. Independence and modularity are not alternatives. The Console can remain an
   independent application while every responsibility above its Kernel is an
   ordinary Module.
3. The current independent identity boundary is sound: Console operators and
   access grants must not be borrowed from a managed App. The direct coupling of
   Console Access to Auth and Organization PostgreSQL repositories is not sound
   as a vNext seam ([console_access.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_access.rs#L689),
   [console_access.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_access.rs#L1607)).
4. PostgreSQL, Outbox, Workflow, Story, artifact custody, and workload control
   are not intrinsic Console runtime responsibilities. They are current
   implementation choices or optional product capabilities.
5. The strongest reusable boundary exposed by the alternatives is a thin,
   optional target-side Connector Module. It should expose narrowly scoped
   portable Capabilities from a target App; it must not become a new System
   Plane inside the Kernel.
6. The current UI loader does **not** execute arbitrary remote URLs. An
   authorized server operation downloads HTTPS artifacts, verifies their digest
   and manifest, materializes them locally, and the browser imports them from
   the Console origin ([console_artifacts.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_artifacts.rs#L148),
   [console_artifacts.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_artifacts.rs#L448),
   [console-module-runtime.ts](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/src/app/console-module-runtime.ts#L288)).
   Allowing arbitrary remote URLs would therefore be a new trust model, not a
   simplification of the current one.

## What the current Console owns

| Concern | Current owner and evidence | Independent-App responsibility? | vNext interpretation |
| --- | --- | --- | --- |
| Process and release boundary | A dedicated service boots API, migration, Worker, and combined API/Worker workloads ([api.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/bin/api.rs#L1), [serve.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/bin/serve.rs#L1), [worker.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/bin/worker.rs#L1), [migrate.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/bin/migrate.rs#L1)). | Yes, if Console is a durable multi-App operator product; no for an embedded developer view. | Deployment is an App choice, not a Kernel or Module kind. |
| Operator authentication and sessions | The fixed composition embeds Auth and Password Auth ([composition.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/composition.rs#L109)). Console Access creates users and sessions through concrete Auth repositories and lists/revokes persisted sessions ([console_access.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_access.rs#L772), [console_access.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_access.rs#L926)). | The **separate operator trust domain** is essential. Owning a specific Auth implementation or local user database is not. | Console should require authentication/session Capabilities. Local password, OIDC, or an external identity provider can satisfy them. |
| Authorization and organizations | Console Access owns administrator authority, organizations, and per-managed-Service grants; the schema persists administrators, grants, and audit rows ([0001_create_console_access.sql](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_access/migrations/0001_create_console_access.sql#L1)). Organization data is a separate schema but has a direct foreign key to Auth users ([0001_create_organization_schema.sql](https://github.com/LioRael/lenso-organization-plugin/blob/6bbfa444c47d8c9f47f25ccc353297f003ac8376/crates/organization/migrations/0001_create_organization_schema.sql#L33)). | Console-owned operator-to-target authorization is essential for a central Console. Organization is one policy implementation, not universally mandatory. | `console.access` should depend on typed identity/policy Capabilities, not concrete repositories or shared tables. |
| PostgreSQL state | The production deployment requires a dedicated PostgreSQL database and volume plus a separate migration workload ([compose.yml](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/compose.yml#L21)). Readiness directly checks `ctx.db` ([console_shell.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_shell.rs#L272)). | Durable state is needed only for installed stateful Modules. PostgreSQL itself is not part of Console semantics. | Each stateful Module owns its state contract and migration command. An embedded read-only Console can be stateless. |
| Outbox, functions, and Workflow runtime | `serve` always starts the generic Lenso Worker in normal mode. That Worker polls the platform Outbox, scheduler, and function runtime ([boot.rs](../../crates/lenso/src/host/boot.rs#L94), [lenso-worker/src/lib.rs](../../crates/lenso-worker/src/lib.rs#L80)). Yet the Console Access, Registry, and Story manifests declare HTTP/UI concerns rather than Console-owned workflow definitions ([console_access.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_access.rs#L53), [system_registry/mod.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/system_registry/mod.rs#L156), [story/module.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/modules/story/src/module.rs#L80)). | Not intrinsically. Asynchronous collection, reliable audit export, or long-running operations may opt into these facilities. | Install Scheduler, Durable Event, Workflow, or Outbox Modules only when another Console Module requires their semantics. |
| Target registry and connection state | System Registry owns managed-Service enrollment, target endpoints, signed evidence, Core documents, topology, workload operations, and audit state ([0001_create_managed_service_registry.sql](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/system_registry/migrations/0001_create_managed_service_registry.sql#L3), [0002_create_system_connections.sql](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/system_registry/migrations/0002_create_system_connections.sql#L1)). | A central multi-App Console needs a target catalog and trust relationship. A local embedded Console does not. | Separate target catalog, trust/enrollment, invocation, diagnostics, and control Capabilities; do not preserve one mandatory System Registry aggregate by default. |
| Runtime and business-operation gateway | The browser receives a receipt-bound client. The server then revalidates topology, release identity, contracts, operator grants, and target identity before forwarding a request ([console-module-client.ts](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/src/app/console-module-client.ts#L35), [surface_gateway.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/surface_gateway.rs#L195)). | A mediation boundary is essential when Console and target App are independent. Its current release/digest/System-topology machinery is not. | Bind generated clients to portable Capability contracts and ActorAssertion audience. Keep read, mutation, diagnostics, and workload-control authorities distinct. |
| Story aggregation | Story is marked optional in the Console composition ([composition.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/composition.rs#L98)). Its local query joins platform Outbox, function runs, story events, and provider calls ([queries.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/modules/story/src/backend/queries.rs#L4)); its federation layer explicitly says it is read-only and cannot advance workflows or write a source Service Store ([federation.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/modules/story/src/federation.rs#L1)). | No. It is a separately installable observability product. | `story.query` and `story.collect` should be optional Modules consuming business/audit evidence and/or Runtime Diagnostics. OTel-only Apps need neither. |
| UI shell and contributions | The Shell serves the SPA and content-addressed artifacts; dynamic routes select a declared surface, validate target context, load the ESM, and inject a restricted client ([console_shell.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/console_shell.rs#L91), [dynamic-console-module.tsx](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/src/app/dynamic-console-module.tsx#L78), [dynamic-console-module.tsx](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/src/app/dynamic-console-module.tsx#L342)). | A Console needs a Shell and a contribution model. Artifact storage and execution trust policy can vary. | UI Contribution discovery, asset resolution, UI execution, and capability-client injection should be separate seams. |
| Workload control and recovery authority | System Registry stores workload operations; deployment recovery also fences normal and restore workloads and disables background work ([0003_create_workload_control_operations.sql](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/modules/system_registry/migrations/0003_create_workload_control_operations.sql#L1), [lib.rs](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/service/src/lib.rs#L55)). | Not part of the local-first Console minimum. It becomes relevant to deployment operations and later distributed systems. | Keep it out of the first Console/App connector and out of Kernel. Add an explicitly authorized control Module later. |

## Essential responsibilities versus accumulated coupling

For an independent operator product, four responsibilities survive the vNext
simplification:

- authenticate an operator in a trust domain independent from target Apps;
- authorize that operator for an exact target Capability and operation;
- maintain explicit target connections when more than one App is managed; and
- render a Shell that discovers UI contributions and injects only their declared
  Capability clients.

Everything else should be conditional. PostgreSQL is a storage adapter. Local
password sessions, organizations, Story, OTel, Outbox, durable Workflow,
artifact caching, workload control, and multi-target discovery are ordinary
Modules or implementation choices. The current framework makes several of
these appear mandatory because every Host receives platform migrations and the
generic Worker runtime; for example, platform migrations create Outbox, Story,
config, idempotency, and delivery tables together
([migrations.rs](../../crates/platform-core/src/migrations.rs#L11)).

The current target-side System Plane demonstrates why the connector must remain
narrow. It combines capability discovery, schema-digest negotiation, workload
identity, enrollment, Module operations, runtime observability, installation,
and runtime control ([platform-system-plane/src/lib.rs](../../crates/platform-system-plane/src/lib.rs#L31),
[lenso-api/src/lib.rs](../../crates/lenso-api/src/lib.rs#L146)).
Those are separable product capabilities, not one Kernel-owned management API.

## Three vNext shapes

| Shape | Strengths | Structural costs | Best fit |
| --- | --- | --- | --- |
| **A. In-App Console Module** | No connector transport, discovery, or duplicated process; direct local Capability bindings; simplest proof that Console and Story are removable Modules. | Shares target App failure and trust boundary; makes independent operator Auth and durable access state awkward; one Console cannot manage several Apps; a “Console Module” easily becomes a new giant module. | Local development, a single-App embedded admin surface, and the first local-runtime spike. |
| **B. Independent non-Lenso service** | Strong process, release, state, and identity isolation; unrestricted implementation stack; natural multi-App product. | Reimplements composition, lifecycle, capability clients, Auth integration, and plugin semantics outside Lenso; weakens dogfooding; still requires a target-side connector; likely drifts into a second framework. | Only if Console requirements demonstrably cannot be expressed through Lenso Modules and Capabilities. |
| **C. Independent Lenso App composed from ordinary Modules, with a thin target connector** | Preserves independent Auth/state/failure domains while proving the Module system; Console Shell, Access, Target Catalog, UI Registry, Story, Audit, and storage can be replaced independently; the same deep seams can also support an embedded development composition. | Requires an explicit portable connector contract and actor delegation; adds a process and deployment; local-first vNext must not disguise the connector as generic Remote Module support. | Central operator Console and the most direct successor to the current independent service. |

The source evidence gives C the cleanest architectural fit, but this is not yet
an acceptance decision. A is still a valuable **composition profile** for local
development, and B remains a valid escape hatch if a concrete constraint
invalidates Lenso's own abstractions.

## Deep seams to preserve across all shapes

The Console Shell should depend on the same logical contracts whether their
providers are in the same App or behind a connector:

| Capability seam | Responsibility | Explicit non-responsibility |
| --- | --- | --- |
| `ui.contribution.catalog@1` | List routes, navigation, presentation metadata, asset references, and portable Capability requirements. | Does not grant business authority or execute assets. |
| `ui.asset.resolve@1` | Resolve an approved contribution to loadable bytes or a URL plus trust metadata. | Does not decide operator business permissions. |
| `console.operator.identity@1` | Authenticate operator credentials and establish a typed ActorAssertion. | Does not grant target operations. |
| `console.access@1` | Decide whether an ActorAssertion may use an exact target Capability operation. | Does not discover targets or forward calls. |
| `console.target.catalog@1` | Store/read named target identities and connection health when the composition needs multiple Apps. | Does not become a Kernel registry or placement system. |
| `console.target.invoke@1` | Invoke an explicitly portable target Capability with deadline, cancellation, domain/runtime error separation, and attenuated ActorAssertion. | Does not expose a stringly typed global Registry. |
| `runtime.inspect@1` | Read reconstructable composition, Module lifecycle, and binding state. | Does not include durable business data. |
| `runtime.diagnostics@1` | Stream best-effort, non-blocking Runtime Diagnostics. | Is not Story, audit, or a durable event log. |
| `runtime.control@1` | Optional dangerous lifecycle operations under separately bound authority. | Is absent from the local-first minimum and never implied by inspect access. |
| `story.query@1` | Optional durable business/runtime timeline. | Is not required for Console or OTel. |

For shape C, the target App installs an optional Connector Module that consumes
only the local Capabilities selected by App Composition and exports the portable
subset. The independent Console App installs the corresponding client Adapter.
This is an application-level Module boundary for v1, not a reason to add Remote
Module, discovery, placement, or Control Plane abstractions to Kernel.

## UI trust consequence of allowing arbitrary remote URLs

Current Module Surfaces are explicitly trusted and same-realm
([CONTEXT.md](https://github.com/LioRael/lenso-console/blob/1822b90da07585eb38469a7ff2d52573ce6f6fb9/CONTEXT.md#L59)).
Digest and manifest checks prevent accidental substitution, but same-realm JavaScript
still has the authority of the Console page. React error boundaries and a
restricted `ConsoleClient` do not sandbox malicious code.

Therefore a future arbitrary-URL policy needs two distinct execution contracts:

- **Trusted same-realm contribution**: may integrate deeply with React and the
  Host UI, but installation is equivalent to installing Console code. A remote
  URL must be explicitly trusted and should be pinned or materialized before
  execution.
- **Sandboxed remote contribution**: may come from an arbitrary URL, but runs in
  an isolated browsing context and receives a narrow, origin-checked message
  bridge. It cannot receive the same-realm Console client, DOM, bearer token, or
  unrestricted navigation authority.

Choosing “arbitrary URL” without choosing one of these trust meanings leaves
Q76 underspecified. It should not be hidden inside `ui.contribution.catalog@1`;
the contribution must declare an execution/trust class that the App Composition
explicitly permits.

## Questions left for the architecture interview

1. Is the production Console primarily a central operator product, while an
   embedded Console is only a development composition?
2. Must an installed Console always have a local recovery identity, or may some
   compositions rely entirely on an external identity provider?
3. Is Console Access a mandatory durable Module for production, or can a fully
   external authorization provider satisfy the same Capability?
4. Does “arbitrary remote URL” mean trusted same-realm code installation or
   untrusted sandboxed content?
5. Which target Capabilities are needed in the first local-first proof:
   contribution catalog, runtime inspection, diagnostics, business invocation,
   or only a strict subset?
6. Should the initial independent Console use an explicit HTTP/UDS Connector
   Module while generic Remote Module support remains deferred?
