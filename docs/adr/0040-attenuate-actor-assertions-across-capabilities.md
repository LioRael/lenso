# Attenuate ActorAssertions across Capabilities

An ActorAssertion may propagate to another Module only when its audience already covers the target Capability and Operation. Otherwise, an explicitly bound delegation implementation must derive a narrower assertion that preserves parent provenance and cannot widen identity claims, audience, or validity. The original credential is never forwarded.

## Consequences

- Audience names stable Capability and Operation identities rather than processes or network locations.
- A derived assertion carries a stable parent or delegation reference and the current attenuated result rather than copying an unbounded delegation chain into Invocation Context.
- Background work has no implicit ActorAssertion. A Module that must act as a bounded automated business actor obtains an explicit assertion from Auth or delegation behavior.
- Caller Module identity continues to describe the direct runtime caller independently of the initiating or delegated Actor.
- The target Capability owns final business authorization and reports refusal as a Domain Error. The Kernel handles forged provenance as a protocol violation but does not define business permission outcomes.
- Auth, delegation, and target business Modules emit explicit security events when durable audit is required. A bound Audit Module may retain the full chain; lossy Runtime Diagnostics are never the audit source.
