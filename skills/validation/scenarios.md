# Lenso skill behavioral scenarios

Use these as independent forward tests after a substantial skill change. Give
the evaluator the named skill, prompt, repository access needed by the task,
and an isolated writable workspace. Do not include the expected observations
in the evaluator prompt.

## Fixture discipline

Supply every artifact the prompt claims already exists. A missing generated
contract, target package, package lock, SDK backend, or runtime owner makes the
test harness incomplete; an evaluator that stops and names that prerequisite
has behaved correctly. Record such a run as **inconclusive**, not as a skill
failure. Keep fixtures free of the expected observations.

## 1. Planning route

**Skill:** `lenso-business-planning`

**Prompt:** "Plan the first useful Lenso slice for a signed-in support agent to
create a customer ticket. We may want Bun and a web UI later."

**Fixture:** read-only current Lenso owner repositories; no prewritten product
plan.

**Required observations:** the result owns Ticket facts/policy in a vertical
Plugin; treats Bun as an Adapter choice; names Capability edges/cardinality;
keeps final authorization with the target Plugin; cuts one success, Domain
Error, Runtime Failure, and observable proof; defers later UI/scale; and hands
each artifact to one workflow.

## 2. Request Capability

**Skill:** `lenso-capability-authoring`

**Prompt:** "Create a portable `support.ticketing@1` request Capability with
`create_ticket`. Rust and Bun providers must implement the same contract."

**Fixture:** an empty writable contract-package directory plus the current
`lenso-contract-codegen`, Rust contract runtime, TypeScript contract runtime,
and repository gates.

**Required observations:** Descriptor and package-local JSON Schemas are the
only portable source; Domain Errors remain separate from Runtime Failures;
Rust/TypeScript generated Provider and Client paths are concrete; generate,
check, and stale-output gates are run; Bun/Rust type checks and one cross-runtime
path are named; requirement cardinality is left to consuming Plugin source and
provider selection to Host resolution.

## 3. Native Rust Plugin

**Skill:** `lenso-plugin-authoring`

**Prompt:** "Implement the Rust ticket provider from an existing generated
contract. It owns durable ticket state and must reject invalid configuration."

**Fixture:** the exact generated contract crate, target Plugin/App repository,
current native Adapter dependencies, and a selected durable test store plus
migration policy. The evaluator must not invent these inputs.

**Required observations:** the agent finds the selected API versions; uses
`#[lenso::plugin]`, `#[lenso::provides]`, `PluginConfig`, typed Ports, and
linked generated registration rather than compatibility factories; validates
configuration;
uses lifecycle phases correctly; keeps persistence private and failure honest;
registers linked availability in the Host Catalog and configures any App-owned
difference under `plugins/`; and proves real invocation,
restart/cleanup, storage failure, and removal.

## 4. Bun Plugin

**Skill:** `lenso-plugin-authoring`

**Prompt:** "Provide the same ticket request Capability from Bun."

**Fixture:** the exact generated TypeScript contract, current
`@lenso/bun` packages, a target Bun package/App project, and the real Bun
Adapter test harness.

**Required observations:** the official `@lenso/bun` generated Provider is
used through `definePlugin` and a default export; the generated entrypoint owns
startup; Plugin code does not call `serve` or implement wire/process mechanics;
unsupported stream/event authoring fails closed; package lock, selected Adapter,
and generated endpoints are explicit; unsupported CLI packaging is reported
rather than invented; and a real `bun_cross_runtime` request test crosses the
boundary.

## 5. App configuration

**Skill:** `lenso-app-configuration`

**Prompt:** "Compose one HTTP ticket Plugin, one Ticket provider, Auth, and Web
Ingress. Add a second independent HTTP endpoint provider."

**Fixture:** a target App project with package manifests/locks, exact
Capability Descriptors/generated artifacts, configuration Schemas, and the
current authoring CLI. A synthetic package fixture proves authoring/resolution
only, not executable host integration.

**Required observations:** exact package Bundle/locks, stable Instance keys,
configuration, optional structured files, and disablement are the only visible
`plugins/` inputs; implementation choice, lanes, endpoints, requirements,
provider keys, and bindings stay absent; `lenso app check` reports an
intentional unresolved Host Slot; `lenso app show` explains defaults,
selection, bindings, and provenance; and removing one optional endpoint leaves
a valid derived App.

## 6. Execution Adapter

**Skill:** `lenso-runtime-extension`

**Prompt:** "Add a Python child-process execution class for portable request
Capabilities."

**Fixture:** the chosen Adapter owner repository and namespace, a generated
Python Capability backend/runtime package, the current core/conformance
packages, and a supported Python executable. Without the Python binding
backend or ownership decision, expect a concrete prerequisite handoff rather
than a handwritten wire contract.

**Required observations:** the work is classified as an Execution Adapter;
Kernel/Plan remain portable; one open execution class is registered; Plan
Instances are filtered exactly; generated codec/endpoint/handshake tables are
validated before readiness; process/frame/queue/cancel/shutdown/recreate
mechanics stay in the Adapter; Runner assembly is explicit; product behavior is
absent; and product-neutral plus real Python-process conformance is required.

## 7. Dynamic Plugin transition

**Skill:** `lenso-app-configuration` with `lenso-runtime-extension` as the
secondary workflow

**Prompt:** "Install a reviewed Tool Plugin into the Agent Harness, upgrade it,
prove one in-flight Turn is not migrated, then roll back."

**Fixture:** current Agent Harness CLI and accepted product ADRs, two immutable
reviewed Tool Bundles, the Plugin Store, durable Generation controller, and a
real Turn fixture. Do not pre-author the expected lock or Generation records.

**Required observations:** Plugin is treated as Plugin distribution rather than
a Kernel type; admission and activation are separate; automatic local
admission is used only if the exact risk profile qualifies; requested and
effective grants remain distinct; the candidate closes exact lock/Plan/
Artifact/Generation authority; readiness precedes switch; a Generation Lease
pins the in-flight Turn while the predecessor drains; history/provenance names
exact digests; rollback uses retained authority; and unsupported generic
marketplace, remote-distribution, or hot-Transition claims are excluded.

## 8. Multi-implementation Plugin Release

**Skill:** `lenso-plugin-authoring` with `lenso-runtime-extension` as the
secondary workflow

**Prompt:** "Publish the same Rust Agent Tool as portable Wasm and trusted
Process implementations, prefer Wasm in this Host, and prove Process is not a
runtime fallback."

**Fixture:** current `lenso-cli`, `lenso-plugin-sdk`, Wasm and Process Adapters,
a Host Catalog builder with implementation policy, and a real consumer. A V2
single-implementation Bundle is insufficient for this scenario.

**Required observations:** one editable Plugin source produces one V3 Release;
one Plugin Contract owns configuration, Capabilities, restart/criticality, and
state semantics; implementation records alone own target, entrypoint, exact
runtime package, and Execution Class; `plugin check`, `plugin dev`, and `plugin
pack` use their real gates; both implementations pass identical success,
Domain Error, cancellation, and lifecycle vectors; Host policy selects Wasm
before Plan resolution; no-match is rejected; a selected Wasm startup failure
fails the candidate Generation without attempting Process; changing to Process
creates a separately resolved Generation.

## Pass condition

Every required observation must be visible in the evaluator's decisions,
artifacts, or executed evidence. Packaging validation, a plausible prose answer,
or naming the correct architecture terms alone is not a pass.
