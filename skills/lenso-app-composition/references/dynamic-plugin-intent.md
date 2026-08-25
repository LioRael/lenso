# Dynamic Plugin intent

Use this branch only when the target product already exposes a supported Plugin
workflow. Static `lenso.app.json` remains the source-derived App Definition;
dynamic installation does not turn it into a mutable runtime registry.

## Keep the authorities separate

| Authority | Owner and meaning |
| --- | --- |
| Plugin Bundle/Release | publisher bytes and generated Manifest |
| Admission Receipt/Store | Host verification and immutable stored content |
| Desired State | App-owner selection, named Plugin Instances, Slot choices, and narrowed scope approvals |
| Change Proposal | resolver explanation and any real owner decision still required |
| Plugin Set Lock | exact admitted Releases selected for this App state |
| Resolved App Plan | complete immutable Module graph and bindings |
| App Generation Spec | exact Plan, Artifacts, grants, Host build, policy, and Plugin lock |

Store admission alone never activates a Release. Desired State contains user
intent, not Module endpoints, bindings, Artifact paths, execution classes, or
Plan fields. A publisher cannot grant its own permissions or choose another
Plugin's private binding.

## Product workflow

Inspect the target product's current help and accepted ADRs. The workflow must:

1. validate the complete Bundle and immutable Release identity without running
   Plugin code;
2. distinguish automatic low-risk admission from explicit review and report
   which policy applied;
3. edit Desired State through the product CLI/UI, recording only exact Release,
   named Instances, supported Slot choices, configuration, and narrowed scopes;
4. review the Change Proposal and resolve every remaining semantic ambiguity;
5. materialize exact lock, Plan, Artifact, grant, and Generation authorities;
6. stage behind the Ready Gate, atomically switch, and retain the predecessor
   as authorized rollback state while old Leases drain; and
7. inspect history/provenance before upgrade, rollback, removal, or retention
   planning.

Automatic local admission is product policy, never a bypass. In the current
Agent Harness it is restricted to low-risk local, stateless Tool Providers that
append to a `many` requirement without permissions, state, replacement, or
intra-Plugin dependencies. Any broader Release requires explicit review.

The Agent Harness currently exposes product-specific `plugins install`,
`status`, `upgrade`, `rollback`, `history`, `inspect`, and `remove` commands.
Read its current `--help`; do not present them as generic `lenso` CLI commands.
Its complete Desired State/Reconciler/hot-Transition product loop is still in
progress even though the structural App Generation runtime is implemented.

## Completion

Return the product and CLI version, Release/Receipt/Store identity, Desired
State diff, owner decisions, requested and effective grants, exact Plan and
Generation digests, Ready/switch/drain evidence, rollback handle, provenance,
removal result, and every unsupported step.
