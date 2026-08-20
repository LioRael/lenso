# Materialize one App Composition

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Lenso will keep one App Composition as the sole application-level declaration of selected Modules, implementation bindings, and dependency selections. Product Blueprints, Addons, and Capability Packs are authoring recipes that materialize exact composition entries and then cease to participate in resolution; their origin may remain as informational provenance. Module and Service contracts remain owner-local, while deployment and environment state remain outside the App Composition.

## Consequences

- A Product Blueprint creates the initial App Composition but does not remain an authoritative repair baseline for an evolving application.
- Capability recipes materialize exact Module and Service references rather than forming persistent overlay layers.
- `lenso.app.json` is the single exact composition and lock artifact; it carries a revision and pins immutable Module releases, implementation bindings, and resolved dependency references.
- App Composition does not copy routes, events, permissions, Console Surfaces, Service configuration, process commands, release workflows, or deployment settings from owner-local contracts.
- App Composition is authoritative for authoring, build, and System connection only; it is not production Workload desired state or orchestrator input.
- Adding or changing a capability presents a concise Composition Impact Summary and updates the App Composition directly; atomic writes, idempotency, and recovery may remain internal implementation details.
- Module dependencies originate in owner-local contracts; App Composition records only the exact dependency selections that satisfied them.
- Recipe provenance remains informational and never participates in later resolution.
- Console loads a Surface only from the exact connected Module Release selected by the App Composition; Surface metadata is not copied into `lenso.app.json`.
- A Service-backed Module binding records a stable Service Reference and never deploys its Service. Missing or incompatible Services are reported as object-level connection state.
- Console reports unavailable, incompatible, or unmanaged runtime objects without automatically creating, upgrading, adopting, or deleting them.
- `lenso system dev` may realize local Workloads through a replaceable Local Control Adapter without changing the deployment-neutral meaning of App Composition.
- App Change Plan, Launchpad, and App Proof are not durable product concepts in the target experience.
- Console projects the App Composition together with current object-level Connection Status; on-demand diagnostics remain available without becoming a first-class workspace.
- Existing Launchpad, proof, and layered composition implementations require an explicit migration and must not be described as already removed until code and public contracts change.
