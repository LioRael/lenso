# Planning output

Return one compact implementation handoff.

## Outcome

- actor and useful result
- first success and honest failure
- authoritative facts and trust boundary

## Module map

For each Module, record:

- responsibility and deletion boundary
- owned facts and lifecycle
- provided and required Capabilities
- execution needs without choosing an unnecessary process boundary

Also record configuration/resources, final authorization responsibility, first
slice behavior, and deletion proof.

## Capability map

For each cross-Module edge, record:

- Capability role and owning contract package
- consumer, eligible providers, and cardinality
- request, stream, or event Operations needed by the first slice

## First executable slice

- keyed Module Instances
- explicit bindings and required configuration or secret references
- success, failure, and observable evidence
- primary implementation skill for each remaining owner

## Artifact handoff

Name the concrete artifact expected from each workflow:

- Capability package/Descriptor/Schemas/generated targets;
- Module package/factory or Bun entrypoint and lifecycle behavior;
- `lenso.json` Instances/contracts/bindings/package inputs;
- focused success/Domain Error/Runtime Failure fixture; and
- real Adapter/host evidence when the slice crosses one.

The handoff is incomplete if a fact has multiple owners, a dependency is only
a code import, the first slice depends on later work, or the proof requires an
undeclared global registry/private table.
