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
Module; treats Bun as an Adapter choice; names Capability edges/cardinality;
keeps final authorization with the target Module; cuts one success, Domain
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
path are named; cardinality/provider choice is left to Composition.

## 3. Native Rust Module

**Skill:** `lenso-module-authoring`

**Prompt:** "Implement the Rust ticket provider from an existing generated
contract. It owns durable ticket state and must reject invalid configuration."

**Fixture:** the exact generated contract crate, target Module/App repository,
current native Adapter dependencies, and a selected durable test store plus
migration policy. The evaluator must not invent these inputs.

**Required observations:** the agent finds the selected API versions; implements
generated Provider plus `NativeModuleFactory`; validates entrypoint/config;
constructs exact endpoints; uses lifecycle phases correctly; keeps persistence
private and failure honest; registers the factory but still edits Composition;
and proves real invocation, restart/cleanup, storage failure, and removal.

## 4. Bun Module

**Skill:** `lenso-module-authoring`

**Prompt:** "Provide the same ticket request Capability from Bun."

**Fixture:** the exact generated TypeScript contract, current
`@lenso/bun-module` packages, a target Bun package/App project, and the real Bun
Adapter test harness.

**Required observations:** generated TypeScript Provider plus
`@lenso/bun-module` is used; the Module does not implement wire/process
mechanics; unsupported stream/event authoring fails closed; package lock,
script entrypoint, Bun execution class, and endpoints are composed explicitly;
and a real child-process Adapter test crosses the boundary.

## 5. App Composition

**Skill:** `lenso-app-composition`

**Prompt:** "Compose one HTTP ticket Module, one Ticket provider, Auth, and Web
Ingress. Add a second independent HTTP endpoint provider."

**Fixture:** a target App project with package manifests/locks, exact
Capability Descriptors/generated artifacts, configuration Schemas, and the
current authoring CLI. A synthetic package fixture proves authoring/resolution
only, not executable host integration.

**Required observations:** package-manager inputs/locks, stable Instance keys,
contract inputs, exact endpoints/requirements, `one` Auth/Ticket bindings, and
`many` endpoint bindings are visible in `lenso.json`; non-empty configuration
has a Schema; `check` rejects an intentional missing binding; canonical Plan
diff is reviewed; and removing one optional endpoint leaves a valid Plan.

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

## Pass condition

Every required observation must be visible in the evaluator's decisions,
artifacts, or executed evidence. Packaging validation, a plausible prose answer,
or naming the correct architecture terms alone is not a pass.
