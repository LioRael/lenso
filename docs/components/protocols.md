# Lenso Protocols

This Rust workspace owns runtime-neutral protocol tooling and portable
conformance fixtures under `crates/` and `spec/`. Its protocol crates do not
define Kernel, host-runtime, product-Capability, or Plugin behavior. JavaScript
and TypeScript SDK work is moving to
[`lenso-js`](https://github.com/LioRael/lenso-js) under
[ADR 0077](../adr/0077-consolidate-the-rust-main-chain-by-language-and-product.md),
but the npm protocol runtime packages below are still separate published
dependencies, not members of either current workspace.

## Packages

- `lenso-contract-codegen`: generates Rust and TypeScript bindings from a
  runtime-neutral Capability descriptor.
- `lenso-contract-runtime`: provides the small, platform-neutral wire primitive,
  serde, and portable JSON surface shared by generated Rust bindings.
- `@lenso/contract-runtime`: provides the matching dependency-free TypeScript
  wire types, portable JSON behavior, and forward-compatible Domain Error
  decoding as an npm package.
- `lenso-plugin-authoring`: provides runtime-neutral typed Ports and generated
  contract references, including source-named requirement connection.
- `lenso-process-protocol` and the separately published
  `@lenso/process-protocol`: retain the exact HTTP V1 protocol,
  transport-neutral Authoring V2 values, and the versioned execution-target
  capability-profile contract.
- `spec/fixtures/portable-contract`: cross-language value-profile conformance data.

Generated bindings retain contract-specific values, Provider traits, Clients,
Endpoints, and operation dispatch. The runtime owns only reusable wire behavior;
patch and minor runtime releases must preserve that behavior, with conformance
tests and generated artifact drift checks guarding both sides of the boundary.

Rust request Providers return `NativeRequestFuture<Operation>` directly. The
generated Endpoint preserves the typed domain/runtime result without wrapping
the Provider future in a second allocation; erased dispatch remains available
only as the compatibility boundary. This Provider signature starts with
`lenso-contract-codegen` 0.4 and requires `lenso-kernel` 0.1.4 or newer.

Generated Rust Clients also implement
`lenso_plugin_authoring::CapabilityClient`. This is the portable seam used by
lifecycle-bound `Port<C>` fields: Plugin glue can connect a whole generated
Client from Plan-owned dependencies without knowing its request, stream, or
Event handle layout. Rust Capability crates generated with this version must
depend on `lenso-plugin-authoring`. New generated clients can narrow that
view by exact `requirement_id`; unscoped lookup remains the compatibility path.

TypeScript bindings also expose a typed Provider interface and a generated
contract reference. Runtime packages consume the runtime-neutral
`CapabilityProviderBinding`; Plugin authors implement only their generated
`Provider` alias and register it with `bindProvider`. Decoding, encoding,
Domain Error preservation, unknown Operations, and thrown Plugin failures stay
inside generated contract code instead of leaking into a Bun or Node runtime.

Capability owners generate and check only the language projections they ship.
The original paired form remains available for packages that intentionally own
both artifacts:

```sh
lenso-contract-codegen generate capability.json --rust src/generated.rs
lenso-contract-codegen check capability.json --rust src/generated.rs

lenso-contract-codegen generate capability.json --typescript src/capability.ts
lenso-contract-codegen check capability.json --typescript src/capability.ts

lenso-contract-codegen generate capability.json src/generated.rs generated/bindings.ts
```

## Validation

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
```

The Rust checks above run here. Cross-language consumers in `lenso-js` must
build, typecheck, and test against exact published npm runtime versions. Changes
to those runtime packages still need their own source-package gates until they
are consolidated.
