# Lenso System

Lenso models business systems that can begin as one modular application and evolve selected boundaries into independently operated services.

## Product Positioning

**Agent-Ready Business System Framework**:
The Lenso product category for small product teams that need coding agents and humans to build, inspect, and evolve long-lived Rust business systems through the same explicit contracts.
_Avoid_: AI runtime, code generator, SaaS backend template

**Business Slice**:
The smallest useful end-to-end business capability that is runnable through its applicable business behavior and user or operator surface.
_Avoid_: CRUD scaffold, compiling skeleton, demo-only fixture

**App Composition**:
The sole `lenso.app.json` declaration of the exact Module releases, `linked` or Service Reference implementation bindings, and resolved dependency selections that form one Lenso application. It carries its own revision and content digests, resolves owner-local contracts into a System Topology, and contains no deployment state.
_Avoid_: Deployment manifest, environment profile, generated file inventory

**Composition Authority**:
The authoring, build, and System connection scope within which an App Composition is authoritative. It never becomes production Workload desired state.
_Avoid_: Deployment authority, runtime reconciler, orchestrator input

**Product Blueprint**:
An initial authoring recipe that creates an App Composition for a recognizable product shape and ceases to be authoritative after materialization.
_Avoid_: Application desired state, runtime overlay, repair baseline

**Capability Recipe**:
A reusable authoring input that adds a coherent set of Module and Service references to an App Composition, then remains only as informational provenance.
_Avoid_: Runtime package, inherited overlay, install authority

**Composition Impact Summary**:
The concise preview of Module selections, implementation bindings, and dependency changes produced before one direct App Composition update.
_Avoid_: App Change Plan, approval workflow, deployment plan

**Composition Provenance**:
Informational metadata such as the Product Blueprint or Capability Recipe from which entries were materialized. It never participates in resolution or overwrites later application changes.
_Avoid_: Overlay, update channel, desired state

**App Diagnostic**:
An on-demand explanation of invalid references, incompatible contracts, or unavailable connections for an App Composition.
_Avoid_: Product workspace, App Proof, permanent Console section

**Composition Divergence**:
An object-level difference between the App Composition and connected runtime facts, reported as unavailable, incompatible, or unmanaged without automatic creation, upgrade, adoption, or deletion.
_Avoid_: Deployment drift, reconciliation request, System failure

**Local App Realization**:
The replaceable Local Control Adapter behavior through which `lenso system dev` creates and starts local Workloads referenced by an App Composition without making local process creation part of Composition semantics.
_Avoid_: Console process manager, production deployment, App Composition apply

**Module Product Contract**:
The minimal declaration through which a Module contributes at least one business capability to an App Composition. It identifies the Module's version and owner while APIs, events, permissions, jobs, and Console Surfaces remain optional contributions.
_Avoid_: Generic Data contract, generic Action contract, Proof bundle, implementation inventory

**Automatic Surface Composition**:
The rule that a compatible, authorized Module Console Surface becomes available from its accepted Module Product Contract without a second manual enablement state.
_Avoid_: Runtime discovery, manual route registration, implicit trust

**Surface API Grant**:
The digest-bound subset of operations from a Module-owned business API Contract that one exact Console Surface artifact may invoke for its selected Module and Service context, further limited by operation audience and capability.
_Avoid_: Generic Admin Data API, direct Service access, ambient browser authority

**Surface API Client**:
A generated client for the exact Module-owned business API Contract that uses Console's injected, same-origin Surface transport while preserving delegated actor, tenant, deadline, idempotency, and Story context.
_Avoid_: Handwritten fetch wrapper, generic query command, database client

**Console Surface Gateway**:
The contract- and grant-bound same-origin transport through which a Console Surface reaches its selected Module's real business API. It routes and attenuates authority without defining business operations or carrying them through the System Plane.
_Avoid_: Generic Admin API, System Plane capability, arbitrary reverse proxy

**Surface Operation Request**:
The Gateway request that binds one exact API Contract digest and operation identifier to typed input and request context, leaving method and path resolution to the admitted Contract.
_Avoid_: Raw URL request, arbitrary headers, generic action

**Surface Operation Authorization**:
The intersection of the Surface API Grant, the Console Actor's current authority, and the target Module's final business authorization. No layer may expand authority granted by another.
_Avoid_: Proxy-trusted authorization, Surface-only capability check

**Surface Contribution**:
An optional, typed extension from one Module into a declared slot of another Module's Console Surface, referencing an allowed Module API operation rather than executable UI or a generic Admin Action.
_Avoid_: Admin Actions page, arbitrary plugin, cross-Module import

**Lenso Microservice Framework**:
The Service contracts, communication, ownership, resilience, workflow, identity, evidence, and local-system capabilities through which independently operated Autonomous Services participate in one Lenso System.
_Avoid_: Deployment platform, service mesh, microservices-by-default

**Hero Workflow**:
The product path from a business prompt to a runnable Business Slice, through agent-assisted change and Console inspection, and onward to independent Service ownership when a Module boundary becomes ready.
_Avoid_: Feature catalog, scaffold command, deployment pipeline

**Service Capability Tier**:
An explicit statement of which Lenso Service contracts and runtime responsibilities one language kit implements, without implying parity with another kit.
_Avoid_: Language preference, roadmap promise, best-effort compatibility

## Language

**Module**:
A business capability with an explicit contract that can run linked into a Host, be exposed by a Provider, or be owned by a Service.
_Avoid_: Plugin, component

**Host**:
The application boundary that composes linked Modules and may coordinate externally provided Modules.
_Avoid_: Main service, central server

**Service**:
An independently delivered logical boundary that owns its data, contracts, runtime responsibilities, and release cadence. A Service may be realized by several Workloads and many Service Instances.
_Avoid_: Process, container, pod

**Provider**:
A separately running integration endpoint that provides one or more Modules to a Host while relying on Host-owned runtime coordination.
_Avoid_: Service, Provider Module

**Autonomous Service**:
A Service that owns its runtime work, persistence, lifecycle, and release cadence while participating in a Lenso System.
_Avoid_: Provider, Provider Module

**Workload**:
A process role that realizes part of a Service, such as serving APIs, executing background work, or applying migrations.
_Avoid_: Service, Module

**Service Instance**:
One running replica of a Workload.
_Avoid_: Service, Workload

**Lenso System**:
A federation of Hosts, Services, and Modules that together deliver one business system.
_Avoid_: Cluster, deployment

**System Plane**:
The system-level contract and coordination surface that describes topology, policy, releases, configuration, and aggregated operational evidence without carrying business traffic.
_Avoid_: Host control plane, service mesh

**Data Plane**:
The Service-to-Service request and event paths that execute business behavior independently of System Plane availability.
_Avoid_: System Plane, Console

## Events

**Event Contract**:
The stable name, version, and payload meaning of a fact published by a Module or Service.
_Avoid_: Topic, message

**Event Envelope**:
The transport-independent metadata that identifies an Event Contract and carries its causation, actor, tenant, and trace context.
_Avoid_: Broker message, payload

**Transport Adapter**:
The boundary that delivers Event Envelopes through a chosen message infrastructure without changing their contracts.
_Avoid_: Event Contract, Broker

**Inbox**:
A Service-owned record of received Event Envelopes used to make repeated delivery safe.
_Avoid_: Queue, dead-letter queue

## Service Communication

**Provider Protocol**:
The Host-owned interaction contract through which a Provider exposes Modules while relying on Host runtime coordination.
_Avoid_: Service Contract, public API

**Service Contract**:
A request-response operation contract owned by an Autonomous Service and used directly by other Services without routing through a Host.
_Avoid_: Provider Protocol, internal implementation

**Service Client**:
A contract-derived caller interface that applies Lenso context, resilience, and evidence conventions without hiding the underlying Service Contract.
_Avoid_: Host proxy, Provider Module client

**Service Kit**:
The authoring and delivery surface that helps a Service produce artifacts conforming to its Service Contract and operational evidence.
_Avoid_: Remote Module Kit

## Data Ownership

**Service Data**:
The records and migration history owned exclusively by one Service and accessible to other Services only through Service Contracts.
_Avoid_: Shared tables, system database

**Service Store**:
The logical persistence boundary for a Service. Several Service Stores may use one physical database cluster while retaining separate ownership and access controls.
_Avoid_: Database server, shared schema

**Distributed Business Process**:
A business operation that crosses Service ownership boundaries and reaches consistency through explicit messages, progress, and compensation rather than one database transaction.
_Avoid_: Distributed transaction, cross-service transaction

**Event Choreography**:
A Distributed Business Process in which Services react to Event Contracts without one participant owning the end-to-end progression.
_Avoid_: Workflow, Saga

**Durable Workflow**:
A Service-owned, persisted definition and execution record for a Distributed Business Process with explicit progress, timeouts, retries, and operator intervention.
_Avoid_: System Plane job, event handler chain

**Saga**:
A Durable Workflow whose completed steps have explicit compensating behavior when the overall business outcome cannot be completed.
_Avoid_: Database rollback, distributed transaction

## Identity

**Service Principal**:
The stable identity of a Service used for authentication and authorization independently of its network location or current Service Instances.
_Avoid_: IP address, hostname, deployment name

**Workload Identity**:
The short-lived runtime credential through which a Service Instance proves its Service Principal.
_Avoid_: API key, shared secret, user token

**Delegated Actor Context**:
A bounded, audience-specific representation of the initiating actor and permitted intent carried across a Service boundary without forwarding the actor's original credential.
_Avoid_: Browser token, impersonation token

## Discovery

**Service Reference**:
A stable logical reference to a Service that does not encode its current network endpoints or deployment platform.
_Avoid_: URL, Kubernetes Service name, IP address

**Endpoint Resolver**:
A Data Plane boundary that translates a Service Reference into currently usable Service endpoints through local configuration or an external discovery provider.
_Avoid_: Service registry, System Plane lookup

## Resilience

**Call Policy**:
The explicit resilience and safety contract for one Service operation, including its Deadline, retry eligibility, idempotency, concurrency isolation, circuit breaking, and overload behavior.
_Avoid_: Global retry config, middleware defaults

**Deadline**:
The end-to-end time budget for an operation, propagated as remaining time across Service boundaries.
_Avoid_: Per-hop timeout

**Idempotency Key**:
A stable operation identity that lets a Service recognize repeated attempts without repeating the business effect.
_Avoid_: Request ID, trace ID

## Contract Evolution

**Contract Version**:
An independently identifiable revision of a Service or Event Contract whose compatibility can be evaluated before release.
_Avoid_: Service release, implementation version

**Compatibility Verification**:
Evidence that a Consumer and Provider combination can communicate without violating their declared Contract Versions.
_Avoid_: Integration test, schema parse check

**Contract Retirement**:
The deliberate removal of an obsolete Contract Version after its consumers, deprecation window, and replacement evidence have been resolved.
_Avoid_: Deletion, cleanup

## Operational Evidence

**Story Context**:
The stable business-operation identity and causation context propagated across requests, events, retries, and workflows independently of any one technical trace.
_Avoid_: Trace context, request context

**Story Segment**:
The durable business progress and outcome evidence recorded by one Service for its part of a Story Context.
_Avoid_: Span, log entry

**Federated Runtime Story**:
The system-wide business timeline assembled from Service-owned Story Segments and enriched by correlated traces, metrics, and logs.
_Avoid_: Distributed trace, centralized log

## Configuration

**Config Contract**:
A Service-owned declaration of configuration fields, validation, sensitivity, scope, mutability, and activation requirements.
_Avoid_: Environment file, settings page

**Config Revision**:
An immutable, validated set of non-secret configuration values prepared for controlled activation and rollback.
_Avoid_: Runtime override, mutable config

**Secret Reference**:
An opaque reference that lets a Service resolve a sensitive value from its environment's Secret Provider without placing the value in Lenso configuration state.
_Avoid_: Secret value, environment variable

## Tenancy

**Tenancy Mode**:
A Service Contract declaration that operations are not tenant-scoped, may be tenant-scoped, or require Tenant Context.
_Avoid_: SaaS mode, organization feature

**Tenant Context**:
The verified tenant scope carried by an operation across requests, events, background work, and workflows.
_Avoid_: Tenant request field, default tenant

**Tenant Isolation**:
The Service-owned enforcement that prevents data or operations from crossing Tenant Context boundaries regardless of the physical Service Store layout.
_Avoid_: Organization membership, database layout

## Edge

**Edge Contract**:
The system-owned declaration of which Service Contracts are externally exposed and under what path, version, authentication, cross-origin, rate, and lifecycle policies.
_Avoid_: Gateway config, public Service Contract

**Gateway Adapter**:
The boundary that translates Edge Contracts into configuration for local or production traffic infrastructure.
_Avoid_: API Gateway, Host proxy

## Topology

**Operating Region**:
A geographic or infrastructure locality operated as one coordinated reliability boundary for a Lenso System.
_Avoid_: Availability zone, cluster

**Failure Domain**:
A named infrastructure boundary whose failure may affect a group of Service Instances and is carried in operational evidence without becoming business identity.
_Avoid_: Service, Region

## Service Extraction

**Extraction Plan**:
A reviewable, evidence-backed plan for moving a Module from linked execution into an Autonomous Service, including boundary violations, contract changes, data movement, verification, Cutover, and rollback.
_Avoid_: Scaffold, migration script

**Cutover**:
The controlled change that makes an Autonomous Service authoritative for an extracted Module after compatibility, data, and behavioral evidence passes.
_Avoid_: Deployment, release

## Development Evidence

**System Sandbox**:
A local, disposable execution environment that preserves Lenso Service contracts and failure semantics without requiring production orchestration or external infrastructure.
_Avoid_: Staging, production emulator

**Failure Scenario**:
A repeatable test definition for timeout, duplication, reordering, overload, or partial unavailability across Service boundaries.
_Avoid_: Mock, chaos experiment

**Environment Verification**:
Evidence that a System behavior proven in a System Sandbox also works through the selected real transports, identity providers, stores, gateways, and orchestrator.
_Avoid_: Unit test, local smoke

## System Lifecycle

**System Lifecycle**:
The internal, command-scoped process that connects and reports a Console-managed Lenso System without owning deployment or release state.
_Avoid_: Deployment controller, Runtime Observation

**System Connect**:
The user-facing operation that connects an existing Lenso System to Console by establishing its Management Binding, enrollment, trust, and Module Surfaces. Its internal execution may be planned and resumed without exposing deployment-style Plan and Apply concepts.
_Avoid_: Deployment, environment onboarding, production apply

**Management Binding**:
The Console-owned record that binds one Lenso System identity and topology digest to enrolled Service identities, Control Adapter identities, permitted operations, and authority policy.
_Avoid_: System Profile, environment, deployment desired state

**System Lifecycle Coordinator**:
The external authority that conducts one System Lifecycle across independently owned Service boundaries without becoming the owner of Service state.
_Avoid_: Console Reconciler, deployment controller

**System Lifecycle Run Journal**:
The durable cross-boundary record of a System Lifecycle run's planned, completed, and awaiting effects and evidence. It supports resumption but is not authoritative for Service-owned state.
_Avoid_: System source of truth, Console audit log, secret store

**Partial System Run**:
A System Lifecycle outcome in which some effects are complete while others failed or await authority or evidence. Completed effects remain recorded and the run may resume without implying global rollback.
_Avoid_: Rolled-back System, ready System

**Active System Run**:
The single mutating System Lifecycle run permitted for one Lenso System while read-only planning and status remain concurrent.
_Avoid_: Parallel apply, global process lock

**Awaiting Evidence**:
A System Lifecycle state in which an effect was dispatched but its owning Service has not yet supplied enough evidence to prove success or failure.
_Avoid_: Timeout failure, automatic retry

**System Status**:
The read-only summary of topology and authority binding, object Connection Status, Workload Operational State, and recent Workload Control Operations. Any observed Deployment or Service Release version remains an externally owned fact.
_Avoid_: System proof, deployment controller, observability platform

**Connection Status**:
The current `connected`, `unavailable`, `incompatible`, or `unmanaged` result for one Service, Module Surface, or Control Adapter, accompanied by a direct reason when it is not connected.
_Avoid_: System readiness score, degradation model, Proof

**Local Enrollment Authority**:
Development-only authority that may accept bilateral enrollment for an explicitly declared, locally owned Service reachable through a loopback endpoint.
_Avoid_: Automatic discovery, unsigned registry seed

**Enrollment Handoff**:
The owner-local exchange in which a Service owner accepts a signed Enrollment Offer and returns the corresponding signed Enrollment Receipt, whether transported online or offline. It never exposes remote System Plane enrollment activation.
_Avoid_: Registry write, implicit enrollment

**Unmanaged Enrollment**:
An active System Registry enrollment that is absent from the current Management Binding. It is reported as drift and is not automatically adopted or revoked.
_Avoid_: Discovered service, orphan to delete

**Workload Control Operation**:
A typed operational action that suspends, resumes, restarts, or scales an already deployed Workload without selecting, publishing, or replacing its Service Release. Local adapters may realize suspension and resumption as process stop and start.
_Avoid_: Deployment, release, arbitrary infrastructure command

**Workload Suspension**:
A reversible operation that asks the deployment authority to make an already deployed Workload inactive while preserving its Deployment and Service Release identity for later resumption.
_Avoid_: Undeploy, scale to zero, process kill

**Workload Control Adapter**:
The least-privileged component through which Console submits authenticated, typed Workload Control Operations to a Workload's deployment authority. Deployment credentials remain with that authority and are addressed through secret references rather than stored by Console.
_Avoid_: Managed deployment system, deployment adapter, remote shell, credential store

**Workload Control Capability**:
A typed operation that a Workload Control Adapter explicitly declares it can perform for a particular Workload under current authority policy.
_Avoid_: Assumed platform feature, best-effort command

**Workload Reference**:
The stable identity of a controllable Workload declared by a Service and resolved by a Workload Control Adapter independently of ephemeral Pods, processes, or instances.
_Avoid_: Pod name, process ID, Service-wide wildcard

**Target Capacity**:
The provider-neutral desired capacity requested by a Scale operation when the Workload Control Adapter declares that it can safely reconcile that request with deployment controls such as autoscaling.
_Avoid_: Kubernetes patch, provider-specific scaling payload

**Operation Record**:
The concise audit record of a Workload Control Operation, including actor, target, requested action, policy outcome, execution result, time, and associated log reference.
_Avoid_: Deployment record, release evidence bundle, secret-bearing log

**Operational Hold**:
A durable, reversible instruction owned by a deployment authority that keeps an already deployed Workload suspended without changing its selected Service Release. It remains until explicitly resumed, subject to any policy-required expiry.
_Avoid_: Replica patch, undeploy, Console process state

**Active Workload Operation**:
The single mutating Workload Control Operation permitted for one Workload at a time, bound to an idempotency key and observed revision so retries are safe and conflicting actions are rejected.
_Avoid_: Request queue, best-effort retry

**Workload Operational State**:
The deployment-authority evidence of whether a Workload is active, suspended, transitioning, or unavailable. While active, it is complemented rather than replaced by Service-owned health evidence from the System Plane.
_Avoid_: Last requested action, Service health

**Local Control Adapter**:
A host-local Workload Control Adapter that remains available after a development System Lifecycle Coordinator exits and controls only explicitly owned local Workloads.
_Avoid_: CLI coordinator, Console child-process manager

**Workload Operation Handle**:
The stable identifier returned when a Workload Control Adapter accepts an asynchronous Workload Control Operation and used to observe its progress and terminal result.
_Avoid_: Synchronous command result, proof of success

**Control Adapter Identity**:
The pre-registered authority-bound identity through which Console mutually authenticates a Workload Control Adapter referenced by a Management Binding.
_Avoid_: Discovered endpoint, bearer token URL

**Unknown Operational State**:
The explicit state reported when current deployment-authority evidence cannot be obtained. It neither inherits the last requested action nor authorizes Console to bypass the Workload Control Adapter.
_Avoid_: Suspended, failed Workload, cached state

**Kubernetes Integration**:
An optional realization of Lenso Service and Workload contracts under Kubernetes authority. A Lenso System does not require it and Console does not make it the production deployment authority.
_Avoid_: Required Lenso runtime, Console deployment controller

## Delivery

**Service Release**:
An immutable, environment-independent release unit that binds one Service version to its Workload artifacts, Contract Versions, configuration declaration, migration intent, compatibility evidence, provenance, and rollback metadata.
_Avoid_: Container image, deployment

**Deployment**:
The environment-specific realization of a Service Release through a selected infrastructure adapter.
_Avoid_: Service Release, promotion

**Promotion**:
The approval to deploy the same verified Service Release artifacts into a later environment without rebuilding them.
_Avoid_: Rebuild, release

## Delivery Policy

**Policy Pack**:
A versioned, environment-scoped set of deterministic requirements for planning, releasing, promoting, or performing a high-risk operational action.
_Avoid_: CI script, runtime authorization policy

**Policy Evidence**:
The explainable inputs and results that show why a proposed action satisfies or violates a Policy Pack and what must change before it can proceed.
_Avoid_: Pass/fail status, deployment log

## Agent Collaboration

**Agent Plan**:
A machine-produced, reviewable proposal that includes intended changes, evidence, policy results, reversible steps, and any Approval Boundaries before execution.
_Avoid_: Generated command list, autonomous action

**Approval Boundary**:
An explicit point at which a person must authorize an irreversible, production-impacting, trust-changing, or policy-bypassing action.
_Avoid_: Confirmation prompt, manual workflow

## Reliability

**Reliability Contract**:
A Service-owned declaration of availability, latency, dependency criticality, health semantics, degradation, backlog limits, error budget, and rollout safety expectations.
_Avoid_: Monitoring config, Call Policy

**Reliability Profile**:
A reusable baseline of Reliability Contract expectations that a Service can adopt and refine for an environment or business criticality level.
_Avoid_: Deployment preset, SLO template

**Degraded Mode**:
An explicit, observable Service behavior used when an optional or degradable dependency cannot satisfy its contract.
_Avoid_: Failure, fallback implementation
