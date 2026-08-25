# Bun Module recipe

Use this branch for product behavior implemented in TypeScript and executed as
one Bun child process per Module Instance by the supported Bun Execution
Adapter. The Module author uses the generated Provider bindings and
the official `@lenso/bun` package (or its lower-level
`@lenso/bun-module` export); the Adapter owns handshake, JSON-RPC framing,
process control, cancellation, shutdown, and host-failure translation.

## 1. Confirm the supported surface

Inspect the selected `@lenso/bun`, `@lenso/contract-runtime`, generated
contract package, and `lenso-bun-adapter` versions. Official TypeScript
Capability projections are centralized in `@lenso/bun`; do not copy them back
into Rust Capability or Module repositories. The current public Module SDK
supports request Capabilities over the production `json-rpc-http` wire. Stream
and Event descriptors must fail closed until the installed SDK exposes their
typed authoring sessions.

Reference sources live in `LioRael/lenso-bun-adapter`:

- `packages/lenso-bun/README.md` exposes the supported unified authoring and
  Capability imports;
- `packages/lenso-bun-module/README.md` and `src/index.ts` define the lower-level
  Module server API;
- `fixtures/bun/sdk-request-provider.ts` is the smallest Provider; and
- `crates/lenso-bun-adapter/tests/bun_cross_runtime.rs` proves the real process
  boundary.

## 2. Create the package

```text
greeting-bun/
├── package.json
├── bun.lock
├── generated/greeting.ts
├── src/module.ts
└── src/module.test.ts
```

Generate `generated/greeting.ts` from the same Descriptor used by Rust. Keep
business behavior in `src/module.ts`; keep wire messages and process lifecycle
out of the Module package.

## 3. Implement the generated Provider

```ts
import { defineModule, serve } from "@lenso/bun";
import {
  bindGreetingProvider,
  type GreetingProvider,
} from "@lenso/bun/capabilities/greeting";

const greeting: GreetingProvider = {
  async greet(_context, request) {
    if (request.name.trim().length === 0) {
      return {
        ok: false,
        error: { kind: "domain", error: "empty_name" },
      };
    }
    return {
      ok: true,
      value: { message: `Hello, ${request.name}!` },
    };
  },
};

serve(defineModule({ providers: [bindGreetingProvider(greeting)] }));
```

Return the generated success/Domain Error envelope. Throwing or returning a
runtime failure is reserved for operational failure, not an expected business
outcome. `defineModule` rejects empty providers, duplicate Capability IDs, and
interaction kinds unsupported by the installed SDK.

## 4. Compose the process entrypoint

Until TypeScript source-derived Module/Manifest generation is complete, the
App project selects the package through Bun or npm lock state, sets
`execution_class` to the installed Bun class (currently
`lenso.bun-process@1`), and points `entrypoint` to the executable script. It
declares the generated Capability endpoint and every requirement just as a
native Module does. The Resolved Plan, not the script, decides which Instances
exist.

Run the package's ordinary typecheck/test/build gates, then cross the real
Adapter boundary when entrypoint, cancellation, deadline, shutdown, generated
binding, or process behavior changed. A direct unit test alone does not prove
the Bun Module contract.

## Completion

This branch is complete when the owning `@lenso/bun` projection is fresh, the
Provider passes unit checks, the App resolves with the Bun execution class and
locked entrypoint, and one Rust-to-Bun or Bun-to-Rust request proves the real
process Adapter path. Report the remaining source-derived TypeScript packaging
gap instead of inventing generated Manifest support.
