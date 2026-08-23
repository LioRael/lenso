# Request Capability recipe

This worked example creates portable `example.greeting@1` with one request
Operation. Replace the identity and domain shapes; retain the single-source and
freshness workflow.

## 1. Create the contract package

```text
crates/greeting-contract/
├── Cargo.toml
├── build.rs
├── capability.json
├── schemas/
│   ├── greet-request.schema.json
│   ├── greet-response.schema.json
│   └── greet-error.schema.json
├── src/
│   ├── lib.rs
│   └── generated.rs
└── generated/bindings.ts
```

`capability.json` is the source for identity, version, portability, Operation
names, interaction kinds, and Schema paths:

```json
{
  "id": "example.greeting@1",
  "version": "1.0.0",
  "portable": true,
  "cross_lane_transfer": true,
  "operations": [
    {
      "name": "greet",
      "interaction": "request",
      "request_schema": "schemas/greet-request.schema.json",
      "response_schema": "schemas/greet-response.schema.json",
      "domain_error_schema": "schemas/greet-error.schema.json"
    }
  ]
}
```

Request Schema:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["name"],
  "properties": { "name": { "type": "string" } },
  "additionalProperties": false
}
```

Response Schema:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["message"],
  "properties": { "message": { "type": "string" } },
  "additionalProperties": false
}
```

Domain Error Schema:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "oneOf": [{ "const": "empty_name" }]
}
```

Use structured error objects with stable `code` and payload Schemas when an
error carries fields. Keep expected business rejection here; keep deadlines,
cancellation, unavailable providers, and other Runtime Failures outside it.

## 2. Generate both projections

Use the installed tool's current help, then run the equivalent of:

```sh
lenso-contract-codegen generate \
  capability.json \
  src/generated.rs \
  generated/bindings.ts

lenso-contract-codegen check \
  capability.json \
  src/generated.rs \
  generated/bindings.ts
```

`src/lib.rs` normally exposes only the generated contract plus handwritten
convenience behavior that does not duplicate the portable Interface:

```rust,ignore
include!("generated.rs");
```

Use `build.rs` or the repository's deterministic generated-artifact check to
make drift fail compilation:

```rust,ignore
use std::path::Path;

use lenso_contract_codegen::check_generated;

fn main() {
    println!("cargo:rerun-if-changed=capability.json");
    println!("cargo:rerun-if-changed=schemas");
    println!("cargo:rerun-if-changed=src/generated.rs");
    println!("cargo:rerun-if-changed=generated/bindings.ts");
    check_generated(
        Path::new("capability.json"),
        Path::new("src/generated.rs"),
        Path::new("generated/bindings.ts"),
    )
    .expect("generated Capability artifacts are stale");
}
```

## 3. Use the generated sides

The generated Rust surface includes typed request/response/Domain Error values,
a Provider trait, Endpoint, Client, Capability constants, wire codecs, and an
invocation error that preserves Domain versus Runtime failure:

```rust,ignore
impl GreetingProvider for Greeter {
    fn greet(
        &self,
        context: InvocationContext,
        request: GreetRequest,
    ) -> LocalBoxFuture<'static, Result<GreetResponse, GreetingInvocationError>> {
        // Module-owned behavior; return Domain or Runtime failure explicitly.
    }
}

let endpoint = GreetingEndpoint::new(Greeter::new());

// Construct consumers during activation from the resolved dependencies for
// this Module Instance, not from an InvocationContext or a global registry.
let client = GreetingClient::from_dependencies(activate_context.dependencies())?;
let response = client.greet(GreetRequest { name: "Ada".into() }).await?;
```

The generated TypeScript surface supplies the corresponding Provider binding
and Client/value codecs used by Bun and browser Adapters. Do not hand-maintain a
parallel TypeScript Interface.

## 4. Source anchors

Use the selected dependency source first. Current complete examples are:

- `LioRael/lenso-protocols/crates/lenso-contract-codegen/tests/fixtures` for
  request, stream, event, value-profile, sensitivity, and compatibility inputs;
- `LioRael/lenso-secrets-module/crates/lenso-capability-secrets` for a published
  contract package with a build-time freshness gate; and
- `LioRael/lenso-bun-adapter/fixtures/bun` for generated TypeScript Provider
  usage across the process Adapter.

## Completion

The request contract is complete when the Descriptor/Schemas regenerate both
targets deterministically, stale outputs fail the build, Rust and TypeScript
compile/typecheck, and one generated consumer invokes one generated provider
while preserving success, Domain Error, and Runtime Failure channels.
