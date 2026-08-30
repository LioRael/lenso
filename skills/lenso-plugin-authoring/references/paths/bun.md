# Bun request Plugin

Use this path only when the selected `@lenso/bun` packages, generated
Capability projection, Bun Adapter, and real cross-runtime harness exist.

Implement the generated Provider and export one Plugin definition:

```ts
import { definePlugin } from "@lenso/bun";
import { bindJobsProvider } from "@lenso/bun/capabilities/jobs";
import { jobs } from "./jobs.ts";

export default definePlugin({ providers: [bindJobsProvider(jobs)] });
```

The generated entrypoint imports this default export and starts the runtime.
Plugin code does not call a server primitive, parse JSON-RPC, or implement
process handshakes. Process, frame, cancellation, and wire failures belong to
the Bun Execution Adapter.

The current public SDK supports request Capabilities. Stream and Event
descriptors fail closed until their typed authoring sessions ship. Do not claim
that the Rust CLI scaffold or `lenso plugin pack` packages a Bun project unless
the selected CLI and owner repository demonstrate that path.

This path is complete when the package lock and generated Provider are exact,
build and typecheck pass, the generated entrypoint starts through the real Bun
Adapter, `bun_cross_runtime` preserves success/Domain/Runtime outcomes, and an
unsupported interaction or packaging request is reported as a prerequisite.
