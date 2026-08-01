# Lenso Console System Plane Architecture

Status: Approved on 2026-07-30

## Product position

Lenso Console is the operator-facing product for one Lenso System. It is served
by a separately installed Lenso Console Service: an ordinary Lenso Service with
its own API, Worker, Migration Workloads, Service Store, identity domain, release
lifecycle, and failure domain.

One Console Service manages exactly one Lenso System and may manage multiple
Services in that System. Different Systems and environments use different
Console Services. The Console is never a synchronous dependency of business
traffic, never enters a managed Service's Data Plane, and never reads a managed
Service Store directly.

The product is not a privileged business application. Business administration
uses application Data Plane contracts and the business application's own
identity domain. The Console instead realizes the System Plane: management
contracts, operator workflows, intent, coordination, projections, and evidence.

`Runtime Console` is retired as a product name. `runtime` remains a capability
domain, as in Runtime Observability and Runtime Stories.

## System topology and authority

```text
Console Operator
      |
      v
Lenso Console / Console Service API
      |
      v
Lenso Console Service
  - independent identity domain
  - Console Service Composition
  - System Registry
  - Management Intents
  - Console Projections
  - System Operations
      |
      | HTTPS + Workload Identity + bounded delegated authority
      v
managed Service / System Plane Protocol
  - authoritative observations
  - Management Operations
  - Operation Evidence
  - Service-local authorization and policy

Business clients and Services <---- Data Plane ----> managed Services
```

The Console owns the wish and the coordination; each managed Service owns what
actually happens inside its boundary. A Console Projection is disposable and
rebuildable. Console identity, approvals, Management Intents, System Operations,
audit evidence, and Module-owned workflow state are durable Console state.

| Fact | Authority |
| --- | --- |
| Operator identity, roles, sessions, and Console Automation Grants | Console Service |
| System Registry membership and Console-side enrollment record | Console Service |
| Management Intent, approval, projection, reconciliation, and System Operation | Console Service |
| Effective configuration, installed Modules, active Service Release, and health | Managed Service |
| Enrollment Grant, local policy, and revocation epoch | Managed Service |
| Management Operation lifecycle, local effects, and Operation Evidence | Managed Service |
| Enrollment and negotiated compatibility | Bilateral; effective authority is the intersection |
| Business identity and business data | The business Service and its Data Plane |

The Console Service is not registered as one of its own managed Services. Its
installation, upgrade, restore, recovery, and composition changes are applied
by an external Console Installation Authority.

## Console Service composition

The management-ready minimum is:

```text
Console Shell
+ exactly one Console Module fulfilling the identity Mandatory Console Role
+ exactly one Console Module fulfilling the System Registry Mandatory Console Role
+ zero or more explicitly selected optional Console Modules
```

The Console Shell is capability-neutral. It owns framing, routing, navigation,
presentation preferences, accessibility, localization, the Console Session
Gate, permission enforcement, failure isolation, and bootstrap diagnostics. It
owns no management page, management state, or management workflow.

Mandatory Console Roles are invariants, not permanently fixed packages. The
official distribution supplies first-party implementations, but compatible
Modules may replace them through a controlled Console Composition Change. A
missing, ambiguous, or incompatible mandatory role enters Console Recovery Mode.
An optional Module failure quarantines that Module and its declared dependents
without disabling unrelated capabilities.

A Console Module is an ordinary Lenso Module. One atomic, signed Module Release
binds its manifest, delivery, migrations, contracts, requested permissions, and
optional Console UI Artifact. The UI has no independent product identity or
version.

Supported delivery follows ordinary Module Delivery:

- `linked` code is full-trust native code included in a new Console Service
  Release and owns state in its part of the Console Service Store;
- `service` delivery executes out of process, owns state in the supplying
  Service Store, and has no direct Console Service Store access;
- dynamically loaded native libraries are unsupported;
- future runtimes such as Wasm require a new implemented contract revision.

UI composition has two lanes: Shell-rendered declarative surfaces and
Module-owned UI in a sandboxed iframe. Isolated UI uses the versioned Console
Bridge with short-lived handles derived from the exact Console Permission Grant.
The Shell never dynamically imports same-origin extension JavaScript, and a UI
artifact never receives the operator's original credential or an ambient
Console bearer token.

Installation does not imply authority. A Module Release requests stable,
operation-level permissions; the exact Console Service Composition grants a
reviewed subset pinned to the release digest. Permission expansion, a new
outbound destination class, new secret access, a trust-root change, or a move to
linked delivery is an explicit Approval Boundary.

## System Plane Protocol

The System Plane Protocol is the only standard management boundary between a
Console Service and a managed Service. The mandatory System Plane Core Protocol
identifies the Service, declares capabilities, and supplies common coordination
semantics. Independently versioned Capability Contracts define optional
observations and operations. UI and navigation concepts never enter this wire
contract.

The first binding is HTTPS, JSON, and OpenAPI. The Console initiates short-lived
calls using an enrolled Service Reference and local Endpoint Resolver. A fixed
well-known resource returns capability endpoint references. The Core requires
no agent, sidecar, persistent tunnel, WebSocket, shared broker, or Service Mesh.
Notifications may reduce polling latency, but snapshots and cursor-addressed
feeds must always be sufficient to rebuild state.

Protocol identifiers are:

- `lenso.system-plane.v1` for the Core Protocol;
- `lenso.system-plane.<capability>.v1` for a Capability Contract;
- `lenso.console-bridge.v1` for isolated Console UI communication.

Core and Capability Contracts version independently. Compatible additions use
feature identifiers within a major version. An incompatible optional capability
isolates only the affected Module-Service pairing; it does not disable other
capabilities or the managed Service's Data Plane.

### Observations and changes

Reads are side-effect-free revisioned snapshots or append-only, opaque-cursor
feeds. Every projected record retains its source Service, source revision,
observed and collected times, cursor or watermark, schema/contract digest, and
freshness state.

- `current` evidence may be displayed as live and used as the base of a plan;
- `stale` evidence may be shown with age but cannot authorize a mutation;
- `expired` evidence is historical only;
- `gap` means continuity cannot be proved.

Cursor expiry, retention loss, schema change, or an unfillable sequence hole is
an explicit Evidence Gap and requires a new snapshot. Absence from incomplete
data never proves deletion; deletion requires a Service-owned tombstone.

### Management operations

Every mutation follows one durable protocol:

1. read a fresh revisioned snapshot and negotiate exact contracts;
2. create an immutable Management Intent with target, desired outcome, expected
   revision, actor or automation, approvals, deadline, idempotency identity, and
   contract/schema digests;
3. request a side-effect-free plan and receive a time-bound receipt containing
   the exact plan digest, expected effects, risks, availability impact, and
   rollback or compensation support;
4. submit the unchanged plan; the Service atomically persists the Management
   Operation and authorization evidence before returning an Operation
   Acknowledgement;
5. follow the Service-owned operation identity to terminal Operation Evidence,
   then refresh authoritative observations.

A plan is not acceptance. A transport response is not acceptance. An
acknowledgement proves durable acceptance, not success. Only terminal
Service-owned evidence proves the operation outcome, and only a fresh
observation proves the resulting state.

Lost responses are resolved by operation identity or idempotency key, never by
blind replay. Timeouts remain `unconfirmed`; they do not fabricate failure.
Cancellation and compensation are new explicit operations with their own
authority and evidence.

A System Operation coordinates several Service-owned Management Operations. It
is not a distributed transaction: required and optional child outcomes remain
separate, partial success is explicit, and successful children are not silently
rolled back when another child fails.

### Reconciliation

Reconciliation compares an active Management Intent with matching in-flight
operations, fresh Service observations, and terminal evidence:

- `in_sync`: fresh observed state satisfies the intent;
- `converging`: a matching accepted operation is still progressing;
- `drifted`: fresh authoritative state contradicts the intent and nothing is
  converging;
- `unconfirmed`: evidence is missing or ambiguous;
- `not_managed`: the capability has no active Console-owned intent.

Connectivity, trust, compatibility, freshness, Service health, operation
outcome, and drift remain distinct. Drift detection never repairs state by
itself. Automated reconciliation still requires a Console Automation Grant,
Module policy, approvals, and managed-Service authorization, and must use the
normal plan-and-submit protocol.

## Identity, enrollment, and authorization

The Console Service installs its own Auth Module and system-management Modules. A Console
Operator exists only in that identity domain. A managed business Service may
independently install its own Auth Modules; it never imports the operator, reads
the Console browser session, or receives the original cookie, password, access
token, or identity-provider token.

Every production System Plane call uses mutually authenticated transport plus
short-lived, audience-bound Workload Identity bound to the live peer. The
official production profile uses SPIFFE/SPIRE X.509-SVID and JWT-SVID through
the existing Workload Identity provider boundary. Static cross-Service API keys,
shared bearer tokens, and a Lenso-owned certificate authority are not supported
production trust mechanisms.

Service Enrollment is a two-sided, revocable ceremony:

1. the Console creates a signed, expiring Enrollment Offer;
2. a Service owner reviews it locally, chooses the Service-owned Enrollment
   Grant, and persists the local record;
3. the Service returns a signed Enrollment Receipt binding both principals,
   System identity, capabilities, policy, revision, nonce, and expiry;
4. the Console verifies the receipt and stores its matching registry record.

Possession of an offer, certificate, catalog record, or network route grants no
authority. One Service environment permits only one active Console Service
Principal until explicit revocation or transfer. Discovery never widens the
Service-owned Enrollment Grant.

Authorization is the intersection of:

1. verified Workload Identity matching the enrolled Console Service Principal;
2. active bilateral enrollment;
3. negotiated contract compatibility;
4. the initiating Module's Console Permission Grant;
5. a bounded Delegated Actor Context or Console Automation Grant when required;
6. target revision, resource/tenant scope, and approval evidence;
7. the managed Service's current local policy decision.

An operator action receives a newly signed, short-lived, non-transitive
Delegated Actor Context for the exact target Service, intent, operation,
resource, tenant, approval, request digest, and idempotency identity. A
multi-Service operation issues one audience-specific context per target. The
managed Service records the accepted authorization snapshot but does not create
a local business user.

Credential rotation preserves stable Service Principals and enrollment. Either
side may revoke enrollment, and the managed Service may do so locally while the
Console is unavailable. Urgent invalidation advances a persisted authorization
epoch. Revocation blocks new work but never rewrites or silently cancels an
already accepted operation. Both sides retain independent append-only audit
evidence without logging secrets or bearer material.

## Capability placement

Current Runtime Console behavior is classified by authority, not by existing
pages:

| Current area | Target |
| --- | --- |
| Shell and navigation | Console Minimum structural behavior only |
| Authentication and operator context | Mandatory identity-role Console Module |
| Service enrollment and basic registry | Mandatory System Registry Module |
| Runtime overview | Optional Runtime Observability Module plus Service capability |
| Queues, functions, retries, dead letters | Optional Runtime Operations Module plus Service operations |
| Execution evidence | Optional Operation Evidence Module plus Service-owned evidence |
| Stories | Optional Runtime Stories Module plus Service Story Segment capability |
| Rich topology, drift, release, runbooks | Optional System Overview, Application Delivery, and Service Evolution Modules |
| Module discovery and lifecycle | Optional discovery/lifecycle Modules plus Service-owned Module Operations |
| Configuration and restart | Optional Service Configuration Module plus Service-owned planned operations |
| Business entity CRUD/query/action | Business Administration Surface outside Lenso Console |
| Source workspace, authoring, packaging, preview, release | External CLI/SDK, CI, and release tooling |
| Dynamic same-Host package loading and filesystem/process mutation | Retired |

Most operational capabilities are paired: an optional Console Module owns the
workflow, policy, projection, and UI, while a managed Service Capability
Provider owns authoritative observations and operations. Installing or exposing
one side never auto-installs the other. Discovery may recommend signed Module
metadata but cannot fetch executable code, grant permission, or activate it.

The Console Minimum contains no built-in Overview, Operations, Services,
Modules, Data, or Configuration page. A Console Composition Preset may select a
recommended set without changing ownership or silently activating Modules.

The initial official Console Module identities are:

- `lenso/system-registry`;
- `lenso/system-overview`;
- `lenso/runtime-observability`;
- `lenso/runtime-operations`;
- `lenso/operation-evidence`;
- `lenso/runtime-stories`;
- `lenso/application-delivery`;
- `lenso/service-evolution`;
- `lenso/module-discovery`;
- `lenso/module-lifecycle`;
- `lenso/service-configuration`.

The identity Mandatory Console Role is deliberately not assigned a permanently
fixed ModuleId: the official Auth composition supplies the default, while the
role remains replaceable.

## Installation, release, and recovery

The official product is one signed immutable Console Service Release. It binds
API, Worker, and Migration OCI Workload artifacts, the Shell, exact default
composition lock, mandatory Modules, configuration and contract digests,
migrations, provenance, SBOM, signatures, compatibility evidence, and rollback
constraints. The browser UI is served by the API Workload; it is not a hosted
tarball, embedded managed-Service asset, desktop application, or separately
installed frontend package.

`local`, `kubernetes`, and `externally_managed` are deployment adapters, not
different Console products. Kubernetes is optional. Production uses a dedicated
logical PostgreSQL Service Store and credentials plus external Secret References.
It may share physical infrastructure with other Services but never their schema,
role, migration history, or transaction boundary. Secret values are excluded
from configuration records, composition locks, logs, UI payloads, and backup
manifests.

Installation and upgrade are immutable reviewed plans applied by the external
Console Installation Authority. Mandatory-role replacement, permission
expansion, trust-root change, destructive migration, and irreversible effects
are Approval Boundaries. Linked Module changes require a new Console Service
Release. The Console may prepare its own composition plan, but it cannot mutate
its running deployment or become its own authority.

Rollback is allowed only when Store schema, configuration, migrations, and
external effects prove compatibility. Otherwise the release requires a reviewed
forward-recovery path. Managed Services continue operating through any Console
outage.

A Console Recovery Set binds encrypted Store bytes to the exact release,
composition, configuration, schema and contract digests, System identity,
Console Service Principal continuity, Secret References, and restore
preconditions. Secret values and live sessions are excluded. Projections may be
rebuilt, but identity, registry, enrollment receipts, permissions, intents,
operations, audit evidence, composition state, and linked Module state must be
protected.

Recovery fences the previous deployment, restores into a clean Store, resolves
secrets externally, starts with outbound mutations disabled, proves one
authoritative deployment, reconciles active operations with managed Services,
then rebuilds projections. Preserved identity and key continuity may preserve
enrollment; otherwise every managed Service requires explicit re-enrollment.
There is no universal backdoor. Break-glass access is local, single-use,
time-bound, auditable, and cannot perform managed-Service operations.

## Product, repository, and public surface boundaries

The GitHub repository is `LioRael/lenso-console`, renamed in place from its
former Runtime Console identity. It is the complete product repository and
owns the Console Service composition and release, Workload entrypoints, Console
Shell, Console Service API, registry, official Console Modules, projections,
intents, System Operations, Console Bridge, tests, OCI build, and deployment
templates.

Other ownership remains explicit:

- `LioRael/lenso` owns public Service, Module, identity, System Plane wire
  contracts, managed-Service Core/Capability Provider seams, generators, and
  architecture enforcement; it owns no Console executable or Console state;
- `LioRael/lenso-cli` remains external Console Installation Authority tooling;
- `LioRael/lenso-release` publishes and promotes `service:lenso-console` under
  the reviewed release process;
- business Module repositories own business behavior, Service-side Capability
  Providers, and separately identified companion Console Module source;
- the catalog indexes signed Module Releases and compatibility metadata, never
  independent Console npm packages.

Public Rust wire contracts live under `lenso-service::system_plane`, with
author declarations re-exported through `lenso::system_plane` and
`lenso::console`. A focused internal `lenso-platform-system-plane` package owns
Core routing, negotiation, common operation handling, and provider registration
without concrete capability policy or Console state.

`lenso::console` is available on the default facade for Module UI declarations.
`lenso::system_plane` is enabled by the opt-in `service` feature, which is also
included by `host`; this keeps ordinary declaration-only consumers lightweight
while giving Service and Console hosts one canonical verification surface.
Consumers must prefer `verify_enrollment_exchange` when accepting enrollment:
it verifies both the Console-signed Offer and Service-signed Receipt before
returning registry evidence, so receipt verification cannot accidentally omit
Offer trust.

The monolithic `lenso-platform-admin` and `lenso-platform-admin-data` Console
roles are deleted after behavior moves to its authority owner. Generic
`AdminDataSource`, action/query seams, `ModuleManifest.admin`, `AdminSurface`,
and the broad `Admin*` declaration family are not mechanically renamed: business
administration and System Plane management have different authority models.
`ModuleManifest.console`, Console surface/navigation declarations, and slots
remain. `ConsoleUiArtifact` carries the isolated artifact contract; fixed
Console areas and same-origin JavaScript bundle execution are removed. Specifically,
`ConsoleArea::{Runtime, Operations, Data, Configuration}` and
`AdminEmbeddedRuntime::JsBundle` have no target equivalent.

`ModuleManifest` remains the delivery-independent capability declaration from
the ubiquitous language. Release version, delivery form, artifact coordinates,
digests, and provenance belong to the enclosing Module Release. This resolves
older wording that placed release metadata directly in the manifest without
changing the approved atomic-release invariant.

The Console repository root is not published as `@lenso/runtime-console`.
Internal web source may use private `@lenso/console-web`; the isolated bridge is
`@lenso/console-bridge`. Module-owned UI is carried as an artifact inside its
owning Module Release. `@lenso/service-kit` is owned by the framework SDK; the
old Remote Module kit is retired after its supported behavior moved under
Service Module Delivery language.

Concretely, `@lenso/runtime-console-api` is retired rather than aliased;
Runtime Stories UI belongs to the Runtime Stories Module; identity UI belongs
to the Console identity-role Module; and business administration UI belongs to
its business Module rather than the Console repository. `@lenso/remote-module-kit`
is retired; Remote Module is not a public package category.

The external CLI surface is `lenso console install|upgrade|backup|restore|doctor`,
`lenso console operator bootstrap`, `lenso console composition plan|apply`, and
`lenso console dev` for the complete local Service. Module authoring and
lifecycle use ordinary `lenso module create|dev|install|update|disable|remove|doctor`
commands, including `lenso module dev --console-ui` and
`lenso module create --with-console-ui`.

The old command vocabulary has no compatibility aliases:

- `lenso console update` becomes Service-level `lenso console upgrade`;
- `lenso console bootstrap-admin` becomes `lenso console operator bootstrap`;
- `lenso console package create|apply-plan` and `lenso host update-console` are
  deleted in favor of Module Release and composition operations;
- `--runtime-console-root` becomes `--console-root` only where a source checkout
  is genuinely required;
- `--console-version` becomes an exact Service Release reference;
- `--no-console-extension` and `--no-console-plan` are deleted;
- `--with-console` becomes `--with-console-ui` and binds the UI to the same
  Module Release.

HTTP namespaces are:

- `/` for the Console web application;
- `/api/console/v1/*` for the Console Service API;
- `/health/live`, `/health/ready`, and `/health/startup` for health;
- `/bootstrap/v1/*` for network-restricted local bootstrap and recovery;
- `/system-plane/v1/*` on managed Services for Core and Capability Contracts.

The target contains no managed-Service `/admin/*`, `/console/*`, or
`/console/extensions/*` compatibility routes. Business Module HTTP remains in
the Data Plane, including `/modules/{module}/http/*`. Existing
`/system/delivery/*` management behavior is represented by its System Plane
Capability Contract.

The following managed-Host artifacts are retired explicitly:

- `.lenso/console/dist`, `.lenso/console/extensions`, and
  `.lenso/console-package-install-plan.json`;
- managed-Host Console extension registries and copied-bundle ledgers;
- `LENSO_CONSOLE_DIST_DIR`, `LENSO_CONSOLE_EXTENSIONS_DIR`,
  `LENSO_CONSOLE_BASE`, and all hosted-asset `LENSO_RUNTIME_CONSOLE_*` variables;
- `lenso-runtime-console.tar.gz` and hosted-archive download/publish workflows.

This retirement does not remove Module Console UI. The standalone Console
Service receives the selected composition from the management workflow,
downloads each immutable artifact, verifies its declared digest, and
materializes it in content-addressed storage. It records a composition receipt
only after every selected artifact is durable. Executable UI still runs only in
the sandboxed cross-origin frame described above; there is no replacement
same-origin registry or managed-Service asset route.

`lenso.system.json` may remain a developer or CLI planning artifact but never
becomes Console Registry runtime state. Historical tags, changelogs, and release
notes remain historical evidence.

There are no current users, so the target intentionally supplies no compatibility
packages, command aliases, route adapters, environment fallbacks, archive
installers, or data migrations.

## Handoff invariants

Implementation planning must preserve all of these constraints:

1. Console failure or absence never interrupts managed-Service business traffic.
2. The Console never reads or writes a managed Service Store.
3. Each management fact has exactly one authority; uncertainty stays explicit.
4. No state-changing request bypasses plan, durable acceptance, evidence, and
   Service-local authorization.
5. Operator credentials never cross into a managed Service.
6. Console composition never grants System Plane authority implicitly.
7. Optional capability incompatibility or failure remains isolated.
8. Business administration does not enter the System Plane.
9. Console self-change and recovery remain externally applied and non-recursive.
10. Releases, Modules, UI artifacts, contracts, permissions, and evidence are
    exact, immutable, digest-bound units rather than independently drifting files.
11. No legacy Runtime Console coupling survives solely for compatibility.

## Scope boundary

This specification decides architecture and product position. It does not define
an implementation sequence, migration plan, release schedule, hosted
multi-customer SaaS, multi-System Console, or Marketplace commerce. Those begin
only after this specification is approved and handed off.

## Decision record

This draft integrates the resolutions of:

- [Define the System Plane Protocol and connectivity model](https://github.com/LioRael/lenso/issues/430)
- [Inventory current Runtime Console capabilities and couplings](https://github.com/LioRael/lenso/issues/431)
- [Define the Lenso Console Service composition model](https://github.com/LioRael/lenso/issues/432)
- [Define the Console Module contract and lifecycle](https://github.com/LioRael/lenso/issues/435)
- [Define Console identity, trust, and delegated authorization](https://github.com/LioRael/lenso/issues/434)
- [Define system state ownership, commands, evidence, and drift](https://github.com/LioRael/lenso/issues/436)
- [Define Console Service installation, upgrade, backup, and recovery](https://github.com/LioRael/lenso/issues/437)
- [Classify current Console capabilities into the new architecture](https://github.com/LioRael/lenso/issues/428)
- [Finalize Lenso Console naming and repository boundaries](https://github.com/LioRael/lenso/issues/433)
