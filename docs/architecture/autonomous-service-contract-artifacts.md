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
and rollout safety. These are declarations for review and future compatibility evaluation; Lenso
does not enforce them at runtime in M0.

## Validation

The public Rust validator and artifact check reject malformed declarations, duplicate contract
or Config field identities, unsupported formats, empty artifact paths, and contract references
to Modules the Service does not own. Callers that know the packaged file set can additionally use
`validate_autonomous_service_artifact_references` to reject unresolved paths. Every issue has a
stable code, deterministic JSON path, and next action. The packaged schema, committed generated
schema, and v2 fixture describe the same surface.
