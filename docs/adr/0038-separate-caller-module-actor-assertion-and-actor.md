# Separate Caller Module, ActorAssertion, and Actor

Lenso vNext will distinguish the Kernel-established Caller Module from an Auth-issued portable `ActorAssertion` and from the typed `Actor` projection consumed by application code. The Kernel transports a sealed assertion without understanding its identity or authorization meaning; Auth Modules establish and verify assertions, SDKs and owning Modules project types such as `UserActor`, and each target business Module makes the final authorization decision.

## Consequences

- `ActorAssertion` uses an extensible namespaced actor kind rather than a Kernel enum of anonymous, user, service, and system variants.
- No authenticated assertion is represented as absence, not as an ambient `AnonymousActor`. A domain that needs a guest defines its own `GuestActor`.
- Ordinary Module-to-Module calls use Caller Module identity and do not synthesize `ServiceActor`. An automated business actor requires a bounded assertion with issuer, audience, and expiry.
- No `SystemActor` carries ambient superuser authority.
- Assertions carry authenticated identity, assurance, audience, validity, and optional issuer-namespaced claims rather than one platform-wide grants array. Explicit Authorization Modules may assist, but cannot widen authority or replace the target Module's final decision.
- Actor projections belong to the SDK or Module that defines their meaning: for example, Auth may own `UserActor`, Console may own `ConsoleAdminActor`, and a game Module may own `PlayerActor`.
- Provider bindings validate provenance, audience, validity, and actor kind and create the requested typed projection before invoking the business handler. Projection failure remains a Capability-defined domain outcome.
- `Principal` is not adopted as a universal synonym because it conflates these roles and conflicts with the existing Service Principal meaning.
