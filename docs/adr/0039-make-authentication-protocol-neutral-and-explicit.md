# Make authentication protocol-neutral and explicit

Ingress Adapters will select one credential according to an explicit protocol policy and submit protocol-neutral `CredentialEvidence` to a bound Auth Capability. Authentication returns `Absent`, `Authenticated(ActorAssertion)`, or a classified rejection such as invalid, expired, revoked, or unsupported; Auth provider unavailability and other execution failures remain Runtime Failures rather than being collapsed into an anonymous or invalid identity.

## Consequences

- HTTP headers, cookies, WebSocket handshakes, and game protocol frames remain Ingress Adapter concerns rather than Auth or Kernel inputs.
- Once an Adapter selects a credential, rejection does not silently fall through to a different credential. An application that needs composite authentication declares that policy explicitly.
- An App may bind different named Auth Module Instances for different credential schemes and ingress paths; there is no implicit global first-success Auth chain.
- Credential material is sensitive and never enters Runtime Diagnostics, logs, App Composition, or an ActorAssertion.
- Only an Auth issuer selected by App Composition may establish a sealed ActorAssertion through the supported Interface. The Kernel or Runtime Adapter preserves issuer provenance without interpreting the assertion.
- Assertions are short-lived and normally validated without an Auth Store round trip. Operations that require immediate revocation knowledge explicitly call an Auth or Authorization introspection Capability.
- Session, revocation, and credential state belong to the Auth Module, which may own a private Adapter or require a durable Capability. Storage failure remains an Auth Runtime Failure and creates no Kernel database dependency.
- Protocol Adapters map authentication outcomes and Runtime Failures into protocol-specific responses; the Auth Module does not own HTTP status or game disconnect behavior.
