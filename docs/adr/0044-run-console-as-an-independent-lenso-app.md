# Run Console as an independent Lenso App

> Superseded by
> [ADR 0060](0060-compose-target-web-ui-in-app-and-separate-cross-app-console.md),
> which distinguishes a target-owned App Web UI from a cross-App Console.

The production Console will be an independent Lenso App composed from ordinary
Modules. A target App that elects to expose Console functionality installs a
thin Connector Module; an embedded Console remains a local-development
composition that binds the same Capabilities directly. Console is not a Kernel
mode, a special Service type, or a Control Plane.

## Consequences

- A production Console binds Operator Identity and Access Policy Capabilities
  in a trust domain independent from target Apps. Local password, OIDC, or
  another provider may satisfy them; only an explicit loopback development
  profile may omit interactive authentication.
- Console state belongs to the Modules that give it meaning. PostgreSQL may be
  the default Adapter in an official deployment profile, but neither Console
  nor Kernel requires PostgreSQL or a universal State Module.
- Outbox, Workflow, Story, Audit, target catalog, and similar facilities are
  installed only when another selected Console Module requires their semantics.
- The target Connector exports only portable Capabilities allowlisted by App
  Composition, such as runtime inspection, Runtime Diagnostics, UI contribution
  discovery, and selected business operations. It exposes no stringly typed
  global registry and cannot mutate the target App Composition.
- HTTP, WebSocket, UDS, and future connection mechanisms are Adapter Module
  choices. Using one for Console does not introduce remote Module execution or
  a transport contract into Kernel.
- Console authenticates an operator, then attenuates the ActorAssertion for the
  exact target, Capability, and Operation. The target Module remains the final
  authorization authority and never receives a Console cookie or session as a
  substitute for an ActorAssertion.
- Runtime inspection and business invocation are separate authorities.
  Composition mutation, placement, workload reconciliation, and generic runtime
  control remain outside the local-first Console and are deferred with the
  distributed-runtime direction.
