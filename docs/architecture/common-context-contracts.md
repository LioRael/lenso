# Common Context Contracts

`lenso.context.v1` is the contract-only envelope for context that crosses an
Autonomous Service boundary. Its Rust types, valid fixture, packaged JSON
Schema, and committed generated schema live in `lenso-service` and
`contracts/context/`.

## Glossary

| Contract | Meaning | Boundary |
| --- | --- | --- |
| Story Context | Durable business-operation and Service-local segment identity. | It survives retries and trace changes; it is not trace context. |
| Trace Context | W3C-compatible technical trace correlation plus diagnostic Baggage. | Baggage is untrusted and keys that carry actor or tenant authorization are rejected. |
| Service Principal | The authenticated caller Service or Workload identity. | `issuer`, `subject`, `audiences`, expiry, and credential ID make the claim verifiable and audience-bounded; endpoints and regions are not identity. |
| Delegated Actor Context | A narrowed user delegation for a Service-to-Service call. | It carries issuer, subject, audiences, intent, permissions, expiry, and delegation ID instead of a browser credential or arbitrary payload identity. |
| Tenant Context | Explicit tenant scope derived from verified identity context. | It carries issuer, tenant ID, audiences, expiry, and claim ID; Baggage is not a tenant authority. |
| Deadline | The absolute end-to-end time budget. | Receivers may preserve the same deadline; this contract does not implement propagation, cancellation, or enforcement. |
| Idempotency Key | A caller-chosen replay identity within an operation scope. | It is not a request ID, trace ID, Story ID, correlation ID, or causation ID. |
| Causation | The immediate cause of work, with optional broader correlation. | It preserves event and workflow ancestry without defining runtime propagation. |
| Region | Logical Operating Region and optional Failure Domain metadata. | It describes execution locality and is never a Service Principal. |

## Validation

The public `validate_common_context_contract` and
`validate_common_context_contract_value` functions return deterministic issue
codes, JSON paths, messages, and next actions. In particular,
`untrusted_actor_claim` and `untrusted_tenant_claim` reject authorization claims
placed in OpenTelemetry Baggage. Verifiable identity claims carry proof metadata
(`verificationMethod`, algorithm, and signature), and
`validate_common_context_contract_for_audience` rejects claims not issued for
the receiving audience. Cryptographic verification uses the referenced issuer
and verification method outside this declaration-only crate.

`ServiceContextPolicy` verifies signed actor and tenant claims before direct
HTTP, direct gRPC, or Event Inbox business behavior. It rejects wrong audience,
expiry, intent, missing or overbroad permissions, and incompatible Tenancy Mode.
The signed context is propagated instead of the actor's original credential.
