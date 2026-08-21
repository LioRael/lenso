# Lenso authoring tooling

The `lenso-app-plan::authoring` module owns the language-independent App
Composition, package input, Capability contract, and Web profile data. The
`lenso-authoring` crate owns filesystem changes, package-manager inspection,
validation, Plan materialization, and native Runner orchestration.

Neither layer installs code into a running Kernel. Package managers acquire
packages and write their ordinary lockfiles before `check` or `resolve`.

## Project document

`lenso.json` contains:

- `composition.modules`: Module Instances, entrypoints, non-secret
  configuration, Capability endpoints and requirements, execution classes,
  and optional Web roles;
- `composition.bindings`: explicit consumer-to-provider bindings;
- `packages`: reviewable Cargo, Bun, npm, or OCI inputs, including the
  package-manager manifest and optional explicit lockfile path;
- `contracts`: exact Capability ID and Descriptor version mappings to the
  Descriptor and generated Rust/TypeScript artifacts; and
- `profiles`: pre-resolution Web recipes with explicit Web Shell, Browser
  Adapter, UI Contribution, and ordinary Module selections.

A Web profile fails closed unless its Shell requires `many
lenso.ui.contribution@1`, every selected contribution provides that Interface,
and its Browser Adapter requires exactly one `lenso.web.shell@1`. The ordinary
Plan resolver still validates every provider binding and execution class. Each
portable Capability required by a selected contribution must be mirrored by
the Browser Adapter with the same exactly-one provider binding; v1 rejects
ambiguous `many` projections rather than granting a broader browser client.
Adding, removing, or replacing a contribution therefore changes both the
reviewable project document and canonical Plan and requires a restart.

There is no Lenso-owned lock model. Cargo inputs are checked against
`Cargo.lock`, npm inputs against `package-lock.json`, and Bun inputs through
`bun pm ls` over `bun.lock`. OCI inputs must use an immutable `sha256:` digest.
The selected exact package-manager version is copied into the Resolved App
Plan as opaque execution identity; Kernel and Adapters do not hash or acquire
artifacts.

Every Capability used by Composition must have a matching Descriptor input.
`check` loads and validates that Descriptor and verifies that both generated
bindings are fresh. Non-empty Module configuration requires a schema. Fields
marked `x-lenso-sensitive: true` accept only a `{ "secret_ref": "NAME" }`
reference, so secret handling is explicit rather than inferred from key names.

## Resolved App Plan

`lenso resolve` serializes `lenso_app_plan::ResolvedAppPlan` itself. There is no
parallel authoring-owned Plan document. Serialization is canonical and byte
stable; loading rejects malformed, invalid, or non-canonical Plan files.

`lenso run` reads that exact file, assembles the native Tokio Runner and the
required built-in Bun production Adapter, translates Ctrl-C into cooperative
shutdown, and reports the terminal outcome. A linked native host supplies its
own statically linked Rust factories through the library API.

```text
lenso add --project lenso.json --key greeter --package example.greeter \
  --source cargo --version 1.0.0 --manifest Cargo.toml
lenso check --project lenso.json --execution-class lenso.native-rust@1
lenso resolve --project lenso.json --output .lenso/resolved-plan.json
lenso run --plan .lenso/resolved-plan.json --root .
```

Library hosts resolve once, persist or review the canonical bytes, load those
same bytes, and pass the resulting `ResolvedProject` to the Runner:

```rust,ignore
use lenso_authoring::{ProjectAuthoring, ResolvedProject, run_project};

let resolved = project.resolve(root, &options)?;
write_plan(resolved.canonical_bytes())?;
let approved = ResolvedProject::from_canonical_bytes(read_plan()?)?;
let outcome = run_project(&approved, driver, adapters, timeout).await?;
```

Changing Composition, package-manager lock state, bindings, configuration, or
profiles requires a new resolve and App restart. The running Kernel cannot
install Modules, discover providers, mutate the graph, or rewrite locks.
