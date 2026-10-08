# Platforms and external consumers

## Choose imports for the actual host

- Bun apps use public Bun adapters. Workers use [Workers Fetch assembly](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/workers/README.md) and [Workers Notes](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/workers/src/notes.ts): inject generated Env bindings inside assembly, keep D1/R2 borrowed, and exclude Bun SQL/SQLite, filesystem config and listener imports from the Worker graph.
- Workers starts an app per request; response bodies retain it until settlement. In-memory state is not persistence. Consume/cancel bodies, and distinguish platform `executionContext.waitUntil` from app-owned work lifetime; platform background work does not automatically keep app resources alive.
- Browser entries use browser/client exports only. Engine authoring stays a development dependency outside runtime/browser graphs.

## External applications and plugins

Inspect the application's installed package exports, declarations and peer requirements. Use its manifest/lockfile as the version authority; the pinned docs are a baseline, not an instruction to downgrade or copy versions. Matching manifest versions do not prove matching published APIs. If an entry/package is absent, find the matching supported contract or report the artifact gap; do not implement missing framework APIs or adapter glue.

External plugins use public `Plugin<Service>` or `@lenso/engine/authoring` contracts, never framework source paths or checkout-only scripts. Follow the standalone [content Engine plugin](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/cli/examples/content-plugin/index.ts) when authoring build extensions. No framework clone is required to consume published exports.

## Logging and telemetry when changed

Read [Log](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/log/README.md) or [OTel](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/otel/README.md) only when wiring observability.

- Reuse the application logger and one host-owned SDK bootstrap; SDK initialization belongs before application/CLI config imports, not plugin setup. Existing external SDK ownership retains flush/shutdown responsibility.
- Choose one HTTP propagation boundary to avoid duplicate tracing. Collect stderr JSON logs once; preserve stdout for finite CLI envelopes or MCP protocol messages. Keep credentials, bodies and unsafe exception text out of telemetry.
- Public Log/OTel integration is supported; no Observe query CLI exists in this baseline. Use the application's existing backend rather than invent a command.
