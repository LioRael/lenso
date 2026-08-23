# Worked planning example: create a support ticket

## Product request

"A signed-in support agent can create a customer ticket and receive a stable
ticket ID. Invalid tickets are rejected and an unavailable store is reported
honestly."

## Outcome

- **Actor:** authenticated support agent.
- **Useful result:** one durable open ticket with a stable ID.
- **Authoritative facts:** ticket ID, customer reference, subject, status,
  creator, and creation time.
- **Trust boundary:** credential evidence enters through HTTP; Auth interprets
  it; Support Ticket performs final authorization.
- **First success:** `POST /tickets` returns the created ID and an owner-local
  persistence fixture confirms that record after restart. Do not add a public
  read Operation only to test storage.
- **Honest failures:** invalid subject is a Domain Error; unavailable durable
  state is a Runtime Failure and never falls back to memory.

## Module cards

### `support-ticket`

- **Deletion boundary:** ticket facts, creation rules, final authorization,
  schema/migrations, and ticket operational metrics disappear together.
- **Owns:** Ticket aggregate and durable transaction/recovery semantics.
- **Provides:** `support.ticketing@1` request Operation `create_ticket` for the
  first slice; `get_ticket` remains deferred until a consumer needs it.
- **Requires:** an ActorAssertion supplied in invocation context or one explicit
  Auth role chosen by the contract design; no access to Auth private state.
- **Execution:** native Rust initially; a later Bun implementation may satisfy
  the same Capability without changing the Module type.
- **Proof:** authorized create, invalid-subject Domain Error, store-unavailable
  Runtime Failure, owner-local restart/recovery, deletion from Composition.

### `support-ticket-http`

- **Deletion boundary:** ticket HTTP route, credential-evidence extraction for
  this route, request/response mapping, and HTTP-specific limits.
- **Owns:** `POST /tickets` protocol behavior, not Ticket facts.
- **Provides:** `lenso.http.endpoint@1` route metadata/handler.
- **Requires:** `support.ticketing@1` exactly once and the selected Auth role
  exactly once.
- **Proof:** real HTTP request reaches generated Ticket client; typed outcomes
  map intentionally to HTTP status/body.

### `auth`

- **Deletion boundary:** credential interpretation and ActorAssertion issuance.
- **Provides:** the selected Auth Capability.
- **Does not own:** Ticket authorization policy.
- **Proof:** invalid credential never reaches ticket creation; assertion
  provenance/audience survives invocation.

### `web-ingress`

- **Deletion boundary:** HTTP listener, parsing, limits, readiness, cancellation,
  and protocol failure mapping.
- **Requires:** `many lenso.http.endpoint@1`.
- **Does not own:** ticket routes or business authorization.
- **Proof:** listener opens only after readiness and rejects malformed/oversized
  requests at the transport boundary.

## Capability edges

| Consumer | Requirement | Cardinality | Provider |
| --- | --- | --- | --- |
| `support-ticket-http` | `support.ticketing@1` | `one` | `support-ticket` |
| `support-ticket-http` | selected Auth Capability | `one` | `auth` |
| `web-ingress` | `lenso.http.endpoint@1` | `many` | `support-ticket-http` |

The Ticket Capability contains business inputs/outcomes, not HTTP status codes,
database rows, Axum/Hyper types, or package/process identities.

## First executable slice

Selected Instances: `tickets`, `tickets-http`, `auth`, `web-ingress`. Selected
Operations: authenticate, create ticket, describe/handle HTTP endpoint. Deferred:
ticket search, assignment, SLA, notifications, UI, multi-lane placement, remote
execution, and alternative providers.

Observable acceptance:

1. valid credential plus valid body returns a ticket ID and durable record;
2. empty subject returns the declared Domain Error/HTTP mapping;
3. invalid credential is rejected before ticket behavior;
4. unavailable storage yields a Runtime Failure/503-class response without an
   in-memory fallback; and
5. removing `support-ticket-http` and its binding leaves the non-Web Ticket
   Capability App valid.

## Implementation handoff

| Work | Primary skill | Completion artifact |
| --- | --- | --- |
| `support.ticketing@1` source and generated bindings | `lenso-capability-authoring` | Descriptor, Schemas, Rust/TypeScript outputs, compatibility/freshness proof |
| Ticket factory, storage boundary, policy | `lenso-module-authoring` | Module package plus Capability/lifecycle/removal tests |
| HTTP endpoint Module | `lenso-module-authoring` | endpoint provider, generated Ticket client, real HTTP proof |
| package/Instance/contract/bindings | `lenso-app-composition` | checked `lenso.json` and canonical Resolved Plan |
| only a missing listener/process/host mechanism | `lenso-runtime-extension` | conformance plus real host smoke; no ticket policy |
