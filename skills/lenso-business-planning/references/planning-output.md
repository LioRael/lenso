# Planning output

Return one compact implementation handoff.

## Outcome

- actor and useful result
- first success and honest failure
- authoritative facts and trust boundary

## Plugin map

For each Plugin, record:

- responsibility and deletion boundary
- owned facts and lifecycle
- provided and required Capabilities
- execution needs without choosing an unnecessary process boundary

Also record configuration/resources, final authorization responsibility, first
slice behavior, and deletion proof.

## Capability map

For each cross-Plugin edge, record:

- Capability role and owning contract package
- consumer, eligible providers, and cardinality
- request, stream, or event Operations needed by the first slice

## First executable slice

- keyed Plugin Instances
- selected Plugin differences, required Capability roles, and configuration or secret references
- success, failure, and observable evidence
- primary implementation skill for each remaining owner

## Artifact handoff

Name the concrete artifact expected from each workflow:

- Capability package/Descriptor/Schemas/generated targets;
- Plugin package/factory or Bun entrypoint and lifecycle behavior;
- `plugins/` package/Instance/configuration differences;
- Host Catalog default, root Slot, attachment, and execution-policy changes when
  the existing Host cannot derive the intended App;
- focused success/Domain Error/Runtime Failure fixture; and
- real Adapter/host evidence when the slice crosses one.

The handoff is incomplete if a fact has multiple owners, a dependency is only
a code import, the first slice depends on later work, or the proof requires an
undeclared global registry/private table.
