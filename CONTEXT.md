# Lenso System

Lenso models business systems that can begin as one modular application and evolve selected boundaries into independently operated services.

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
_Avoid_: Service, Remote Module

**Autonomous Service**:
A Service that owns its runtime work, persistence, lifecycle, and release cadence while participating in a Lenso System.
_Avoid_: Provider, Remote Module

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
_Avoid_: System Plane, Runtime Console

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
_Avoid_: Host proxy, Remote Module client

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
