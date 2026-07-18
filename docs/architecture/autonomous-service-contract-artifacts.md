# Autonomous Service contract artifacts

`lenso.service.v2` lets an Autonomous Service declare the versioned contracts it owns without
adding runtime behavior. Every contract keeps the owning Module identity stable so a Module can
move from linked to Provider to Autonomous Service form without renaming its business contract.

## Request-response and event contracts

`serviceContracts` contains direct request-response contracts. Each entry has a stable
`contractId`, owning `moduleId`, Contract Version, Tenancy Mode, common-context requirements, and
one repository-relative artifact reference. The artifact format is `openapi` or `protobuf`; a
Service need not use both.

`eventContracts` contains transport-independent business event contracts. Each entry has the
same stable identity, ownership, version, tenancy, and common-context fields, with a `json_schema`
or `protobuf` artifact.
The declaration does not select a broker, topic, delivery mode, or Transport Adapter.

## Config Contract

`configContract` is one versioned, Service-owned schema artifact. Every field declares:

- `path` and `shape`;
- whether its value is sensitive;
- `service`, `region`, or `tenant` scope;
- `immutable` or `mutable` evolution;
- `hot` or `restart` activation.

Sensitive fields describe configuration requirements, not secret values. Runtime revisions,
activation, Secret Providers, and enforcement remain outside this contract-only slice.

## Reliability Contract

`reliabilityContract` is a Service-owned schema artifact recording whole-Service availability,
latency, dependency criticality, health semantics, Degraded Modes, backlog limits, error budget,
and rollout safety. A Service selects the `development`, `standard`, or `critical` profile and may
override the profile's queue backlog, Workflow backlog, timer lag, retry exhaustion,
compensation pressure, error-budget consumption, readiness, and liveness defaults. The existing
availability, latency, and queue-backlog declarations remain explicit Service values. Overrides
are validated before startup and resolve to one deterministic `effectiveValues` object.
The existing `healthSemantics` strings remain reviewer-facing explanations; the effective
`readiness` and `liveness` enum values are the machine-evaluated health contract.

| Profile | Workflow backlog | Timer lag | Retry exhaustion | Compensation pressure | Error-budget consumed | Readiness | Liveness |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `development` | 1,000 | 60,000 ms | 100 | 100 | 100% | `serving` | `process_running` |
| `standard` | 250 | 30,000 ms | 25 | 25 | 100% | `serving` | `runtime_operational` |
| `critical` | 50 | 5,000 ms | 5 | 5 | 80% | `healthy` | `runtime_operational` |

`GET /runtime/reliability` evaluates those effective values against Service-owned evidence. Queue
pressure includes local Outbox and Runtime Function work; Workflow pressure includes in-flight
instances, overdue durable timers, exhausted steps, and pending or failed compensation work.
Deployment composition may inject a `ReliabilityObservationSource` for dependency availability,
availability, latency, and error-budget observations. Every check returns a stable state, evidence
references, issue code, and next action. Unavailable critical dependencies make the Service
unavailable; unavailable degradable dependencies activate their declared Degraded Mode and make
the Service degraded; optional dependencies do not reduce Service state.

`/health/live` and `/health/ready` use the resolved `process_running` or `runtime_operational` and
`serving` or `healthy` semantics. A `serving` Service may remain ready in an explicit Degraded
Mode, while `healthy` readiness requires every observation to meet its expectation. M3 reports
and explains evidence only: the report explicitly cannot block production promotion, execute a
canary policy, or trigger automated rollback. Those enforcement paths remain production-delivery
work.

## Validation

The public Rust validator and artifact check reject malformed declarations, duplicate contract
or Config field identities, unsupported formats, empty artifact paths, and contract references
to Modules the Service does not own. Callers that know the packaged file set can additionally use
`validate_autonomous_service_artifact_references` to reject unresolved paths. Every issue has a
stable code, deterministic JSON path, and next action. The packaged schema, committed generated
schema, and v2 fixture describe the same surface.

Compatibility evolution is defined in the generated
[`contract-compatibility.md`](contract-compatibility.md) reference and its packaged golden pairs.

## Extraction Readiness Report

`lenso.extraction-readiness-report.v1` is the first public artifact for moving
one Host-owned linked Module toward an Autonomous Service. The public
`lenso-service` evaluator combines the Module manifest, a validated
`lenso.system.v2` graph, and structured boundary, Contract, and active Consumer
evidence. CLI-owned source analyzers supply repository evidence; they do not
define separate readiness rules.

Findings use the existing `safe`, `needs_attention`, `breaking`, and `blocked`
compatibility vocabulary plus stable issue codes, evidence references, and next
actions. Missing or incomplete source analysis, missing Service or Event
Contracts, ambiguous evidence, unverified Consumers, cross-Module imports, and
in-process boundary calls fail closed. Runtime functions, schedules, Durable
Workflows, admin declarations, Console contributions, and Runtime Story display
metadata remain visible as `needs_attention` preservation work without becoming
an unexplained blocker.

Readiness evaluation is always non-mutating. Its versioned effects object fixes
repository writes, Workload startup, data movement, and authority changes to
`false`. The human projection is rendered from the same report returned as
JSON. Generated schema, blocked support-ticket proof, and corrected
support-ticket proof live under `contracts/extraction/` and are freshness-checked
by `just generated-check` and `just arch-check`. Provider v1 and System v1 keep
their existing Host-managed semantics and are not accepted as Autonomous
extraction evidence.

## Durable Workflow version evolution

Migration `0014_pin_workflow_definition_artifacts` fails closed when a Store
still contains running pre-artifact Workflow Instances. Operators must drain
those instances with the previous Service release before retrying the upgrade;
the migration never adopts the new worker's definition as legacy state.

Every Durable Workflow Instance started after that gate retains the exact
`lenso.workflow-definition.v1` artifact and a canonical SHA-256 digest beside
its selected version. Deploying another definition or restarting a worker does
not rewrite those fields. A worker must register an exactly matching artifact
before it can claim retry or timer work for that instance; an unsupported or
same-version-but-different definition rolls the claim transaction back without
changing workflow state.

`POST /runtime/workflows/definitions/compatibility` compares immutable
definition versions and returns the deterministic `safe`, `needs-attention`,
`breaking`, or `blocked` category plus stable paths and next actions. The
authoritative examples are generated at
`contracts/workflows/lenso.workflow-compatibility.v1.json`.

`POST /runtime/workflows/{owner}/{name}/migration-plans/dry-run` is read-only.
It identifies in-flight source-version instances and their persisted steps,
derives a deterministic state mapping to the registered target definition,
includes compatibility evidence and rollback constraints, and returns a stable
content-addressed plan ID. Every plan reports `mutatesState: false` and the
`in_flight_workflow_migration` Approval Boundary. Lenso does not automatically
execute a plan during definition deployment or worker restart. The generated
Autonomous Service Runtime OpenAPI is the versioned migration-plan contract;
Runtime Console may consume that contract but is not in the execution path.

## Direct HTTP bindings

An OpenAPI `serviceContract` can generate `DirectHttpBindings` from its versioned artifact. Each
operation requires `operationId`; `x-lenso-idempotency` may declare `idempotent` or
`requires_key`, and the required `x-lenso-call-policy` declares the protocol-neutral attempt, circuit-breaker,
concurrency-isolation, receiver-overload, and optional business-fallback policy. Protobuf
contracts use the equivalent `lenso-call-policy` method annotation. Both generators reject
malformed limits, unsafe retries, duplicate fallback triggers, and unnamed fallback handlers with
deterministic paths and reason codes.

The generated HTTP server binding rejects missing or expired absolute deadlines, required
Idempotency Keys, and declared overload before the business handler. The generated gRPC binding
provides the equivalent `DirectGrpcServerPolicy` admission boundary for a tonic service handler;
it validates the absolute deadline, required Idempotency Key, and overload limit from the request
before business handling. Generated HTTP and gRPC clients resolve logical Service References
directly, enforce the same Service-local circuit and bulkhead state, preserve their native
protocol failures, and never use the Provider proxy path. Unknown write safety remains
non-retryable. Explicit fallback declarations name a handler, but the owning Module or Service
composition must register that handler; platform code never creates a business result when one is
absent.

Each terminal response or policy error carries attempts, ordered policy events, terminal outcome,
the applied fallback name when relevant, and native HTTP or gRPC diagnostics. A controlled clock
drives open, half-open, and recovery transitions in deterministic tests without wall-clock sleeps.
The committed HTTP and gRPC binding fixtures are regenerated by `just generate` and checked by
`just generated-check` and `just arch-check`.

## Workload Identity

`WorkloadIdentityProvider` is the public integration boundary for issuing and
verifying short-lived credentials that prove stable `service:<service-id>`
Service Principals. Lenso does not issue production certificates or operate a
certificate authority. `SpiffeWorkloadIdentityProvider` is the first production
implementation: it reads rotating X.509-SVID, JWT-SVID, and bundle material from
an operator-owned SPIFFE Workload API. `SpiffeWorkloadIdentityConfig` maps one
exact `spiffe://<trust-domain>/service/<service-id>` identity to its stable Lenso
Principal. Verification uses the cached JWT bundle and requires no synchronous
Runtime Console, Host, or System Plane lookup.

The provider boundary retains its original synchronous `issue` method for
source compatibility and adds a defaulted `issue_async` extension. Synchronous
providers need no migration; SPIFFE composition uses `issue_async` for Workload
API issuance.

Production HTTP or gRPC composition builds mTLS from the provider's live
`X509Source`. The transport extracts the authenticated peer SPIFFE ID from the
verified client certificate and supplies it as `AuthenticatedTransportBinding`.
The provider accepts a JWT-SVID only when its signature, audience, expiry, trust
domain, Service path, and subject-to-mTLS binding all pass. Live sources consume
issuer rotation updates for subsequent handshakes without changing the Service
Principal or rebuilding the provider. See
[`ADR 0024`](../adr/0024-select-spiffe-spire-as-the-first-production-workload-identity.md).

`SystemSandboxWorkloadIdentityProvider` is deterministic and visibly
development-only. It uses a caller-supplied sandbox signing key, rejects
non-development environments, and records explicit authenticated, expired,
stale, rotated, and rotation-failed evidence. Key rotation changes credential
and key identities while preserving the logical Service Principal. Provider
host tokens are not accepted as Workload Identity credentials.

SPIFFE evidence retains only the verification outcome, stable Service Principal,
JWT digest, and signing key ID. JWT-SVIDs, private keys, certificate key material,
Workload API join tokens, registrations, and production mutations remain
Approval Boundaries and are never stored as Lenso evidence.

Direct HTTP and gRPC receiver constructors require a provider and receiving
audience. They verify issuer, audience, expiry, signature, and
an `AuthenticatedTransportBinding` supplied through trusted transport
extensions before business admission. The binding is evidence from the
authenticated connection; request headers, URL, hostname, IP address, process,
replica, Operating Region, and Failure Domain never become Service identity.
The authenticated Principal is exposed to local business authorization through
the HTTP request or gRPC admission result.
Debug-only `new_without_workload_identity` constructors remain for legacy
contract and call-policy fixtures; they are absent from release builds and are
not Autonomous Service receiver boundaries.

Event consumers use the identity-required `consume_service_events_once_at`. The
receiver verifies the signed Service Principal in the Event Envelope against
the receiving audience and authenticated Transport Adapter binding before
invoking Module-owned behavior. Missing or invalid identity is persisted as an
`unauthorized` delivery outcome and the Module handler is not called.
The explicitly named `*_without_workload_identity` helpers remain only in
debug builds for legacy policy and transport fixtures; release builds cannot
compose them as Autonomous Service receiver boundaries.

## Delegated Actor and Tenant Context

`DelegatedContextProvider` is the issuer integration boundary for signed,
short-lived actor delegation and tenant claims. The development-only
`SystemSandboxDelegatedContextProvider` issues deterministic audience-bound
claims for local tests; production composition supplies its own provider.
Delegated Actor Context carries one explicit intent and a bounded permission
set, never the initiating browser credential.

`ServiceContextPolicy` is shared by direct HTTP, direct gRPC, and Event Inbox
admission. It verifies proof, expiry, audience, exact intent, required and
allowed permissions, and the Service's `required`, `optional`, or `none`
Tenancy Mode before Module behavior. Required mode rejects absent scope;
optional mode preserves verified explicit scope without inventing one; none
mode rejects accidental scoped execution. Tenant claims must share the actor
delegation issuer and cannot outlive the delegation. Endpoint identity,
OpenTelemetry Baggage, and default tenants are never authority sources.

Accepted Event Inbox decisions persist local evidence containing only stable
actor, delegation, tenant, claim, audience, and outcome identifiers. Rejected
decisions use the same safe reason codes and never persist proof signatures.
Runtime function enqueue, claim, retry, and handler execution persist the
explicit `TenantId`; schedules and lifecycle work remain explicitly unscoped.

## Durable Workflow start, recovery, transition, and inspection

`ModuleManifest.runtime.workflows` is the public declaration seam for
engine-neutral Durable Workflow definitions. Each `lenso.workflow-definition.v1`
definition has a stable owning Module, name, version, input and result contract
references, and ordered step metadata. A step may declare `retryPolicy` with a
total `maxAttempts` and one `delaysMs` entry per retry, plus a positive
per-attempt `timeoutMs`. The generated JSON Schema under `contracts/workflows/`
is the committed machine contract for this shape.

Autonomous Service composition collects those Module declarations without
binding them to Runtime Functions or Provider behavior. A start request selects
one exact definition version, then commits a stable Workflow Instance and its
initial step to the owning Service Store in one transaction. The instance keeps
the selected definition version, input, Story Context, optional tenant scope,
state, and timestamps. A later deployment may select a newer version for new
instances, but inspection reads the pinned version recorded by the existing
instance.

A declared Event Contract delivery can start that same definition inside the
Service Inbox transaction through `start_workflow_from_event_in_tx`. The Event
identity is the durable start trigger, and the complete Event Context is stored
with the instance. Module behavior advances a pending step through
`advance_workflow_step_with_event_in_tx`, supplying a stable transition identity
and one outgoing Event Contract publication. The runtime locks the pinned step,
marks it complete, creates the next declared step when present, and writes the
outgoing event to the Service Outbox before the caller commits. A rollback loses
all of those writes together; redelivery of the same transition returns the
committed outcome without publishing again.

Failed execution retains the original step identity while appending one durable
attempt record with a stable failure classification, code, message, attempt
number, and transition identity. Retry scheduling uses the definition pinned by
the instance. Reaching the declared attempt budget marks the step `exhausted`
and the instance `failed`; it cannot remain indefinitely running.

Retry and step-timeout timers persist their due time, attempt number, and stable
transition identity in the Service Store. Workers claim due work through a
lease, and an expired claim is reclaimed after restart. A due timeout takes
precedence over its abandoned retry attempt. Resolving a claim, recording the
attempt, scheduling the next retry, and changing terminal workflow state share
one transaction. A successful retry still uses the normal workflow transition
and Outbox transaction, so replay after an uncertain commit returns the
committed transition without repeating business effects or outgoing work.

Production composition uses the system clock. The development-only
`SystemSandboxWorkflowClock` can advance controlled UTC time so repeated timeout
proofs produce identical classifications and schedules without wall-clock
sleeps.

Outgoing workflow events must match an Event Contract declared by the owning
Service. The runtime derives the Event Type and content schema from that
declaration, carries Story, trace, delegated actor, tenant, deadline,
idempotency, and region context forward, replaces the Service Principal with
the executing Service credential supplied by composition, and records the
completed step as the new causation identity. Cross-Service steps do not read a
remote Service Store and do not use the System Plane as a relay.

Module behavior can decompose a pending step through
`start_child_workflow_in_tx`. The parent step becomes `waiting_for_child` in the
same transaction that creates the version-pinned child instance and its first
step. The child keeps a distinct instance identity plus explicit parent
instance, parent step, and causation links. Story, delegated actor, tenant,
deadline, and present idempotency context are decoded, validated, and copied
from durable parent execution context; they are never reconstructed from trace
or log data.

After a child reaches `completed` or `failed`, a stable completion delivery can
call `resume_parent_from_child_in_tx`. The child link, parent transition, and
next parent step commit together. Redelivery of the same completion returns the
already-committed outcome and cannot create another parent step. A restart
reloads both pinned definition versions from the Service Store. Child failure
or a worker that no longer supports a pinned parent or child version records
durable failure evidence and a stable deployment or recovery next action on the
parent inspection surface.

The Service runtime exposes versioned start and inspection results through
`POST /runtime/workflows/{owner}/{name}/instances` and
`GET /runtime/workflows/instances/{instance_id}`. Errors use the standard
problem-details envelope with stable workflow codes and `next_actions`.
Inspection includes completed transition identity, safe outgoing Event Contract
metadata, retry policy, latest failure, attempt history, timer due times and
claim state, and child workflow evidence for Runtime Console and other operator
consumers. It reads only Service-owned workflow tables and does not require the
Host, Runtime Console, System Plane, or an external workflow engine.

Operator control is a separate durable dispatch gate, not a new business
lifecycle state. Pausing changes an instance from control state `active` to
`paused` while preserving its running, failed, or compensating execution state,
completed steps, attempts, timers, child links, compensation evidence, and any
already-issued worker claim. A paused instance cannot dispatch a new step,
child, retry attempt, or compensation. Work that was already claimed remains
durable, and already-committed Outbox work is not retracted. A completion that
would create new outgoing work is deferred until resume, and the same stable
transition or idempotency identity is used again. Resume only reopens that
dispatch gate and never recreates completed work.

`POST /runtime/workflows/instances/{instance_id}/operator-actions/{action}/dry-run`
returns a deterministic `lenso.workflow-operator-plan.v1` document for `pause`,
`resume`, a selected exhausted-step `retry`, cooperative `cancel`, strong
`terminate`, or recorded human `intervene`. The plan is read-only and binds
the instance revision, pinned definition version, selected step, attempts,
timers, pending work, in-flight claims, children, declared compensations,
irreversible effects, preserved state, affected resources, required authority,
and expected terminal state into a SHA-256 plan identity. Applying
the matching action requires a deployment-owned authority verifier and a
Bearer credential for that exact plan; missing verifier configuration fails
closed. Any intervening state change returns `workflow_stale_plan` and requires
a new dry run.

An authorized manual retry reopens only the selected exhausted step, retains
its step identity and prior attempt evidence, and schedules one new durable
attempt with an intervention-derived transition identity. Story, causation,
delegated actor, tenant, deadline, and idempotency context stay on the original
Workflow Instance. Every applied or idempotently repeated action exposes stable
JSON and records the verified actor, authority, reason, time, plan, prior state,
resulting state, affected step when present, retry transition when present, and
next action in the Service Store. Authority credentials are never persisted.

Cooperative cancel stops future ordinary steps and timers. Completed effects
with declared compensation are selected in their stable order and the Workflow
remains `compensating` until their remote completion Events arrive; only then
does it become `cancelled`. A cancel with no selected compensation becomes
`cancelled` immediately. Repeating the exact authorized plan is idempotent,
while a fresh cancel after terminal intent is recorded returns a stable
eligibility conflict.

Terminate is the separate strong `terminated` state. It stops future ordinary
steps and timers, preserves completed effects and existing compensation
evidence, and records `cleanupReported: false`; it never claims that cleanup or
compensation occurred. Human intervention may target the whole instance or one
stable step without rewriting business state. Cancel, terminate, and
intervention each pass an explicit Approval Boundary and retain actor,
authority, reason, time, tenant scope, affected resources, prior state, and
resulting state in the Service Store. Repository access supplies none of this
runtime authority.

Steps may declare one compensation with a stable name, unique positive order,
request Event Contract, and completion Event Contract. When a controlled
timeout selects compensation, the Service Store retains the completed effect,
stable compensation identity, declared order, attempts, and history. Dispatch
and the outgoing request share the workflow owner's Service Store and Outbox
transaction. The request payload binds the persisted Workflow Instance,
compensation, effect, and action identities rather than accepting caller-owned
values. The Workflow remains `compensating` while the remote Service reverses
its business effect in its own Inbox transaction and publishes the declared
completion Event. Only that correlated completion marks the effect and
compensation complete. A failure instead records `compensation_failed`, its
failure code and next action, a workflow-level final outcome, and an explicit
intervention Story Segment. This preserves exactly-once business effects over
at-least-once delivery without a distributed transaction.

## Story Segment Feed

Every Autonomous Service retains versioned Story Segment evidence in its own
Service Store. A Feed entry has stable Story and Segment identity plus a
positive evidence revision; source Service and Workload; operation and contract
identity; status, attempt, tenant, causation, and timestamps; and Workflow
Instance, pinned definition, step, parent, compensation, or intervention
identity when applicable. New evidence revisions append rows under the same
Segment identity. The Store rejects updates and deletes, and an identical
identity/revision append is a deterministic duplicate.

`GET /runtime/story-segments` returns `lenso.story-segment-feed.v1` in Store
sequence order. Its HMAC-signed opaque cursor is scoped to the source Service,
authenticated reader Service Principal, and requested tenant partition, so a
consumer can retry a page or resume after an API Workload restart. Entries
outside the configured retention window are not returned and a cursor that has
fallen behind retained evidence fails explicitly. Feed reads do not persist an
acknowledgement and never update Workflow Instances, steps, timers, dispatch
gates, Inbox, or Outbox state.

The endpoint fails closed unless deployment composition supplies a Workload
Identity provider, exact audience, reader-to-tenant policy, retention window,
and durable cursor-signing key. The Bearer credential must also match the
authenticated transport binding. Tenant-aware Services expose only the single
authorized partition requested by the reader, and credential proofs or cursor
keys are never written into Story evidence or returned by the Feed.

## Federated Runtime Story

The Story observability boundary consumes at least two authenticated Segment
Feeds and assembles `lenso.federated-runtime-story.v1`. Each source Service and
tenant partition has an independent durable cursor in the aggregation Store.
Collected revisions are append-only and idempotent under source Service,
Segment identity, and evidence revision. The read model selects the latest
revision without changing the stable node identity, so late evidence completes
the existing Story rather than creating a replacement Story.

Source availability is part of the evidence model. `unreachable`, `stale`,
`unauthorized`, `truncated`, and `retention_expired` are explicit typed Segment
gaps. Successful collection resolves transient availability and authorization
gaps; truncation and retention gaps remain visible because later success cannot
prove that lost evidence never existed. A deliberate cursor restart changes
only aggregation state and does not acknowledge or mutate the source Feed.

Every Feed envelope and Segment must match the configured source Service and
requested tenant partition before persistence. Federated reads select one
tenant partition, so a reused Story identity cannot expose another tenant's
Segments or gaps. Trace, metric, and log providers may attach technical
evidence to stable Segment nodes. Enrichment failure is non-fatal and never
changes Story identity, Workflow state, or business completion evidence.

The runtime admin backend exposes collected Federated Runtime Stories through
the existing Stories list and detail API. It projects Segment provenance and
causation into cross-Service nodes and edges, and projects Workflow instances,
steps, attempts, timers, children, compensations, and interventions into
explicit state-bearing entities. The projection retains typed gaps rather than
mapping missing evidence to success. Reads use the authenticated request's
tenant partition and never merge tenant scopes.

Collection also snapshots the latest report-only `lenso.reliability-report.v1`
evidence for each source Service. The shared report contract includes the
selected profile, explicit overrides, deterministic effective values, Degraded
Modes, pressure and SLO checks, evidence references, issue codes, and next
actions. Runtime Console renders that backend projection and does not duplicate
Story aggregation or Reliability Contract evaluation in TypeScript.

## Event Envelopes

A declared JSON Schema `eventContract` generates a versioned
`lenso.event-contract.v1` artifact. The artifact fixes the Producer Service,
owning Module, Event Contract identity and version, Tenancy Mode, common-context
requirements, payload schema, and canonical Event Type without selecting a
broker or delivery product.

`EventEnvelope` uses `lenso.event-envelope.v1` to carry that identity together
with a stable Event ID, occurrence time, the declared `lenso.context.v1`
fields, and typed content metadata. The generated artifact embeds the validated
authoritative payload schema. Public validation compares identity and context
to that artifact, validates content against the embedded schema, and maps
missing, malformed, untrusted, or incompatible values to stable issue codes,
JSON paths, and next actions.

Payload schemas are validated and executed as JSON Schema Draft 2020-12 with
format assertions enabled, including nested structures, arrays, unions, and
references resolvable from the packaged schema.

The CloudEvents 1.0 structured representation keeps the canonical Event ID,
type, Producer source, Module/contract subject, occurrence time, and complete
Lenso Event Envelope as `data`. Decoding checks the CloudEvents attributes
against the authoritative embedded envelope before returning it, including
when the Event Contract declares only a subset of common context. Topic,
partition, offset, broker, and vendor settings are not part of either contract.

`just generate` publishes the generic envelope schema plus the support Event
Contract artifact and round-trip fixture under `contracts/events/`. Their
freshness is enforced by generated-artifact tests and `arch-check`.

## Transport Adapter and local delivery

The Autonomous Service runtime exposes a protocol-neutral `TransportAdapter`
boundary for publishing and receiving Event Envelopes, positive and negative
acknowledgement, health, and diagnostics. Adapter methods expose stable Lenso
delivery types; broker topics, partitions, offsets, consumer groups, and vendor
clients remain outside Module code and Event Contracts.

The local adapter persists transport deliveries and diagnostics in an injected
PostgreSQL Store and needs no external broker, Kubernetes, service mesh,
Runtime Console, or System Plane. Producers write the business change and
Service-owned Outbox publication intent in one transaction. Consumers persist
the received envelope in their own Inbox and invoke Module-owned behavior in a
Service Store transaction before acknowledging delivery. Service-local Outbox,
Inbox, and terminal evidence remain inspectable through
`GET /runtime/event-deliveries`.

The first production adapter binds operator-provisioned NATS JetStream streams
and durable pull consumers. Its topology and credential-bearing client are
injected by Service composition and never enter Event Contracts or Module code.
The adapter uses publish acknowledgements, explicit delivery acknowledgement,
negative acknowledgement, and acknowledgement-timeout redelivery while keeping
diagnostics in the Service Store. It validates existing topology but performs
no implicit production provisioning or cleanup. See
[`ADR 0023`](../adr/0023-select-nats-jetstream-as-the-first-production-transport-adapter.md).

Inbox consumption is idempotent by consumer and stable event identity. Delivery
failures use protocol-neutral `retryable`, `non_retryable`, `expired`,
`unauthorized`, `incompatible`, `poison`, and `exhausted` reasons while retaining
the handler reason code and adapter diagnostic. A Service-owned retry policy
persists attempt count, next-attempt time, history, and terminal outcome;
controlled-time consumers can advance the schedule without wall-clock sleeps.
Poison and exhausted events enter durable dead-letter state with the original
Event Envelope, contract identity, delivery history, failure evidence, and
operator next actions. Terminal isolation lets later healthy events continue.
Versioned operator results support deterministic inspection, non-mutating
replay and cleanup plans, explicit production replay approval, and explicitly
approved destructive cleanup. Replay preserves the original business event,
Contract Version, Story Context, and causation while recording a distinct
delivery attempt. Cleanup retains Inbox deduplication state, delivery evidence,
and replay audit records, and excludes unresolved, retained, or actively
replaying dead letters.
