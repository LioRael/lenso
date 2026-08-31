# ADR 0064: Keep only portable core ownership in the main repository

- Status: accepted
- Date: 2026-08-22
- Extends: ADR 0030, ADR 0031, ADR 0050, ADR 0053, ADR 0054, ADR 0055,
  ADR 0057

## Context

The first vNext implementation was intentionally built in one Cargo workspace
so Kernel mechanics, Runtime Drivers, Execution Adapters, generated Capability
bindings, optional Modules, authoring tooling, and executable examples could
evolve together. That workspace produced the required tracer bullets, but it
does not describe their lasting repository ownership.

Physical co-location now creates three problems:

1. Kernel tests depend on the Greeting Capability, Native Adapter, and example
   App, making product examples appear to be prerequisites of the portable
   runtime.
2. Host-specific Drivers, Execution Adapters, optional Modules, authoring
   tools, and business examples share the release and review surface of the
   portable core even though their Interfaces and change cadence differ.
3. Portable contract source, generated wire values, and runtime-specific Rust
   bindings are packaged together. Moving those packages unchanged would make
   a nominal protocol repository depend on one Kernel implementation.

Repository extraction must reduce knowledge and release coupling. It must not
create a centralized Capability catalog, a coordinated System Release, a
dynamic plugin mechanism, or a second runtime graph.

## Decision

The main `lenso` repository owns only the portable core product Interfaces and
their executable conformance evidence:

- `lenso-app-plan`, which owns serializable Resolved App Plan data;
- `lenso-kernel`, which owns the portable runtime state machine and its narrow
  Runtime Driver and Execution Adapter Interfaces; and
- `lenso-runtime-conformance`, which owns product-neutral fixtures that make
  those Interfaces executable without depending on a concrete Driver,
  Execution Adapter, Capability package, or example App.

Repository-local ADRs, architecture documents, CI, and ordinary crate tests
remain here when they govern or verify those core Interfaces. They are
maintenance artifacts, not additional runtime products.

The following implementations are outside portable core ownership and will be
extracted after their release and conformance prerequisites are satisfied:

| Ownership | Current packages or files |
| --- | --- |
| Host runtime implementations | `lenso-runner`, `lenso-native-adapter`, `lenso-browser-driver`, `lenso-wasip2-driver` |
| Bun runtime integration | `lenso-bun-adapter`, Bun fixtures, and the TypeScript Module SDK |
| Protocol source and tooling | Runtime-neutral Capability Descriptors, Schemas, wire/error rules, `lenso-contract-codegen`, generated bindings, and portable conformance vectors |
| Authoring product | `lenso-authoring` and the `lenso` CLI, owned by the existing `lenso-cli` repository |
| Optional Modules and SDKs | Auth, OpenTelemetry, Story, Agent, game-session, Web Shell, UI Contribution, and other owner-specific packages |
| Examples | Greeting, Counter, Secure Greeting, executable fixtures, and example-only Capabilities, owned by `lenso-examples` until a product owner supersedes it |

Exact new repository names are an operational choice. The ownership and
dependency direction are normative even if several related implementations
initially share one outer repository.

## Dependency direction

All physical repositories depend inward toward the portable core:

```text
protocol source -----> generated bindings -----> Module implementations
                              |                          |
                              v                          v
portable core <------- host runtimes and Adapters <--- Apps and CLI
```

The arrows mean compile-time or release consumption, not runtime discovery.
The main repository must not depend, including through dev-dependencies or test
source, on a concrete Driver, Execution Adapter, product Capability, optional
Module, CLI, or example App.

`lenso-runtime-conformance` may depend only on `lenso-app-plan` and
`lenso-kernel`. It provides a product-neutral Capability and test Execution
Adapter solely to verify the core Interfaces. External Driver and Adapter
repositories will consume its published conformance surface or an equivalent
versioned test artifact; they do not move their implementations back into the
main repository for testing.

Tests follow ownership:

- core CI proves Plan and Kernel behavior through the deterministic Driver and
  Kernel-owned conformance fixtures;
- each Driver or Execution Adapter repository proves its implementation
  against the core conformance Interface;
- protocol ownership proves source-to-binding generation and cross-language
  wire behavior; and
- `lenso-examples` records tested combinations through ordinary versioned
  dependencies and lockfiles.

## Protocol ownership

A Capability package is not automatically an official Lenso protocol. A
framework-owned protocol must have independent consumers or providers, stable
role semantics, and a real need for cross-package or cross-language
interoperation. Business and example Capabilities remain with their owning
Module or example.

Before protocol extraction, the current code-generation shape must separate:

1. the runtime-neutral Descriptor, Schema, value profile, compatibility rules,
   and wire/error model; from
2. runtime-specific binding backends that emit types such as Kernel native
   handles and endpoints.

Canonical protocol source owns generated artifacts and conformance vectors.
Concrete Auth, OpenTelemetry, Story, or business Module implementations may
depend on that source but never become dependencies of the protocol source.

## Release and migration rules

Cross-repository dependencies use published Cargo/npm versions or an explicit
immutable tag during a bounded bootstrap. Cross-repository path dependencies
are forbidden. Package-manager lockfiles, SemVer, dependency-update pull
requests, and integration conformance replace synchronized repository commits.

Migration proceeds one ownership seam at a time:

1. remove core test dependencies on concrete products and enforce the inward
   dependency rule in CI;
2. verify real publication and consumption of `lenso-app-plan`,
   `lenso-kernel`, and any required conformance artifact;
3. separate runtime-neutral protocol source from runtime-specific generated
   bindings;
4. move examples and optional Modules to their existing owners;
5. move Bun integration and authoring to their owning repositories; and
6. extract host Runtime Drivers, Execution Adapters, and Runner only after
   their external conformance jobs pass against released core packages.

Each extraction preserves relevant Git history, records the last monorepo
commit, replaces path dependencies, runs both source and destination gates,
and removes the source only after the destination commit and release inputs are
read back. Accepted ADRs remain immutable in the main repository as historical
decision evidence; new implementation-specific decisions live with their
owner and link back here.

The migration must not create a temporary `lenso-extras` repository. Existing
owners such as `lenso-cli`, `lenso-examples`, and `lenso-auth-module` receive
their concerns directly. A new repository is justified only by a durable
Interface, owner, release cadence, and verification surface.

## Consequences

- Deleting every outer repository leaves a buildable, testable Plan and Kernel
  implementation with no product or host implementation residue.
- Core changes acquire an explicit SemVer responsibility because outer
  repositories no longer receive atomic path-dependency updates.
- Full-App proofs move out of core CI, while Kernel conformance remains local
  and deterministic.
- Driver, Adapter, protocol, Module, CLI, and example owners can release on
  independent cadences without changing Kernel ownership.
- The initial migration temporarily retains outer packages in this workspace;
  physical presence is not evidence of core ownership. CI prevents new inward
  dependencies while extraction proceeds.

## Rejected alternatives

### Keep concrete implementations because Kernel tests use them

Tests should cross the Kernel Interface through core-owned fixtures. Letting a
test dependency decide repository ownership reverses the production dependency
direction and makes the seam harder to extract.

### Keep every Driver and Adapter beside Kernel to prove portability

Kernel portability is proved by target compilation and its conformance
Interface. Each host implementation proves that it satisfies the Interface in
its owning repository. Co-location is not required for either claim.

### Put every Capability in one protocol repository

This centralizes business language and makes Module authors depend on Lenso for
contracts that they should own. The protocol repository contains only
deliberately standardized roles and portable contract tooling.

### Split into `lenso-core` and one temporary `lenso-extras` repository

The temporary repository would preserve the existing release knot and require
a second history migration. Direct movement to named owners is smaller and
leaves one lasting Interface per seam.

## Verification gate

Before the first physical extraction is merged:

- the portable-core repository boundary test passes;
- `lenso-kernel` has no direct or dev-dependency on a concrete Driver,
  Execution Adapter, product Capability, optional Module, CLI, or example;
- core tests pass through `lenso-runtime-conformance` and the deterministic
  Driver;
- required core packages have one verified publication and downstream
  consumption path; and
- the destination repository runs its owned tests using versioned, non-path
  core dependencies.
