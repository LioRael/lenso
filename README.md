# Lenso

A Bun-first plugin framework with ordinary async services. Core owns instance dependencies and resource cleanup. Build-time Engine extensions and optional Web, Auth, Drizzle and Workers adapters remain separate from business code. Console belongs in a future independent repository.

## Run locally

```sh
cd /Users/leosouthey/Projects/framework/lenso
bun install --frozen-lockfile
bun dev
```

The included greeting starts on loopback and reports its actual URL, enabled plugins and readiness. In another terminal:

```sh
bun run cli inspect greeting greet --root examples/greeting --json
bun run cli call greeting greet '{"name":"Ada"}' --root examples/greeting --json
bun run client Ada
```

CLI and Web reuse the same input schema and service. Each CLI call starts a fresh app; HTTP shares the running instance's in-memory counter. Restart resets this counter. `LENSO_PORT` changes the listener port; `LENSO_URL` changes the sample client URL.

## Packages

<!-- prettier-ignore -->
| Package | Purpose | Usage |
| --- | --- | --- |
| `@lenso/core` | `definePlugin`, `defineApp`, validation, Promise lifecycle | API example below |
| `@lenso/engine` | typed discovery, generation, extensible build targets and dev scheduling | [Engine API and plugins](packages/engine/README.md) |
| `@lenso/cli` | command parsing, explicit service calls, terminal presentation and exit codes | [CLI contracts](docs/CLI.md), [development output](packages/cli/README.md) |
| `@lenso/web` | Fetch, oRPC and streaming request ownership | [Web API](packages/web/README.md) |
| `@lenso/auth` | provider adapters, typed middleware and shared service authorization | [Auth API](packages/auth/README.md) |
| `@lenso/db` | native Drizzle PostgreSQL, Bun SQLite and D1 resources | [Database and Notes](docs/DATABASE.md) |
| `@lenso/storage` | streaming local/S3/R2 objects and optional authorized file records | [Storage API and examples](packages/storage/README.md) |
| `@lenso/tasks` | durable PostgreSQL jobs, retries and cooperative worker lifecycle | [Tasks API](packages/tasks/README.md), [producer/worker example](examples/tasks/README.md) |
| `@lenso/workers` | request-owned Fetch app and platform bindings | [Workers API](packages/workers/README.md), [local D1 example](examples/workers/README.md) |
| `@lenso/log` | independent Pino logging, safe stderr output and active trace correlation | [Logging API](packages/log/README.md) |
| `@lenso/otel` | application-owned OpenTelemetry bootstrap, OTLP and optional oRPC/Workers entries | [Telemetry API](packages/otel/README.md) |

[Minimal templates](templates/README.md) consume real packed packages outside the workspace. They are template contents; no scaffold command or npm release is implied. PostgreSQL, SQLite and local D1 Notes use real storage and explicit migrations.

Web and all clients use exactly **oRPC 2.0.0-beta.42**, still a prerelease, with no
v1 compatibility path. See the [Web migration notes](packages/web/README.md).
Console owners must consume the same version and v2 wire format; Console is not a
dependency of logging or telemetry.

For an observed finite CLI call, configure a **local** OTLP receiver and use the
existing greeting preload (SDK initialization happens before the CLI/config):

```sh
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318 \
  bun --preload ./examples/greeting/src/telemetry.ts packages/cli/dist/bin.js \
  call greeting greet '{"name":"Ada"}' --root examples/greeting --json
```

The preload is for finite commands; long-running hosts own drain and bounded
SDK shutdown explicitly. See the telemetry package for owned/external SDK and
Workers configuration. Logging is separate: pass `createLogger()` from
`@lenso/log` as `startApp`'s `logger` option, or supply an existing logger.

## Core API

```ts
import { defineApp, definePlugin, startApp } from "@lenso/core";

const clock = definePlugin({
  id: "clock.primary",
  setup(context) {
    const timer = setInterval(() => {}, 1000);
    context.onCleanup(() => clearInterval(timer));
    return {
      async now() {
        return new Date().toISOString();
      },
    };
  },
});
const report = definePlugin({
  id: "report",
  requires: [clock],
  setup(context) {
    const service = context.get(clock);
    return {
      async run() {
        return service.now();
      },
    };
  },
});
const app = await startApp(defineApp({ plugins: [report, clock] }));
try {
  console.log(await app.get(report).run());
} finally {
  await app.stop();
}
```

IDs identify distinct instances. Dependencies use exact plugin references, and a plugin can access only declared initialized dependencies. Acquire resources during setup and register cleanup immediately. Setup failure rolls back registered resources, including the failing plugin's resources. Cleanup runs sequentially in global LIFO order, attempts every callback and aggregates failures. Concurrent/repeated `stop()` returns the same Promise. Business code stays ordinary async.

`onCleanup` returns an async disposer: `const release = context.onCleanup(close)`.
Use `await release()` for early release instead of calling `close()` separately.
Repeated calls and automatic shutdown share one completion, including failures;
shutdown waits for an early release still in progress. Registration remains
setup-only. For an external listener, register its removal once with
`onCleanup(() => emitter.removeEventListener(name, listener))`; core does not
discover arbitrary listeners or infer cleanup from a returned service.

Consumers can accept a small structural `Plugin<ServiceInterface>` rather than
import a specific provider; [Notes](examples/notes/src/notes.ts) already uses this
pattern with native database types and `NotesQueries`. Every `requires` edge is
mandatory and binds the exact supplied instance. Missing instances and duplicate
IDs fail before setup. If a factory has an optional provider argument, declare
and resolve it only when supplied, with an explicit absent-provider branch.
There is no implicit provider selection or permanently pending startup.
Dependencies are not child scopes: each plugin owns its registered resources,
and dependents release before dependencies during automatic app shutdown.
An optional `source: {file, export?, line?, column?}` on a plugin gives assembly
and lifecycle diagnostics an explicit declaration location; otherwise CLI uses
the actual app config path. Static inspect describes declarations, not service
health or methods returned by setup.

## Instance configuration

Ordinary `Plugin` factories can keep accepting ordinary options. Opt in when a
plugin needs shared validation, startup preflight or multiple sources:

```ts
import { bindConfig, definePluginConfig, startApp, valuesSource } from "@lenso/core";
import type { ConfigSource } from "@lenso/core";
import { envSource } from "@lenso/core/config/env";
import { jsonFileSource } from "@lenso/core/config/file";
import { z } from "zod"; // Any Standard Schema v1 implementation works.

const greetingConfig = definePluginConfig({
  schema: z.object({ prefix: z.string().trim().default("Hello") }),
  description: "Greeting text for one service instance.",
  fields: [{ path: ["prefix"], description: "Text before the name." }],
});

function createGreeting(id: string, config: { prefix?: string } | readonly ConfigSource[]) {
  return bindConfig(greetingConfig, config, {
    id,
    setup(_context, validated) {
      return { greet: (name: string) => `${validated.prefix} ${name}` };
    },
  });
}

const primary = createGreeting("greeting.primary", { prefix: "Hi" });
const secondary = createGreeting("greeting.secondary", [
  valuesSource({ prefix: "Hello" }),
  jsonFileSource({ id: "project-file", root: import.meta.dir, path: "greeting.json" }),
  envSource({
    id: "deployment",
    read: (name) => process.env[name],
    bindings: { prefix: { name: "GREETING_PREFIX" } },
  }),
]);
const app = await startApp({ plugins: [primary, secondary] });
try {
  console.log(app.get(primary).greet("Ada"));
} finally {
  await app.stop();
}
```

Contract, source reading and business setup are separate. `bindConfig` infers
schema input for ordinary objects and schema output for setup, including async
validation and transforms. `startApp` validates assembly, resolves every installed
configured instance, then starts resources only if all configurations pass.
CLI invocation, Web startup and Workers use this same boundary. Sources can
perform I/O during preflight; business setup has not started yet.

Sources are read in declaration order. Later **present top-level fields** replace
earlier fields; nested objects and arrays are replaced whole, never deep-merged.
Source IDs must be unique within each instance; name multiple value sources explicitly.
`undefined` object properties are omitted, `null` is a real schema input, and
defaults come only from the schema. Composition precedes one schema validation.
Input and validated output must be finite, acyclic plain data; resources,
functions, accessors and prototype-pollution keys are rejected. Snapshots are
copied and frozen without freezing caller objects. Instances and starts do not
share resolved snapshots.

Env bindings read only their explicit keys. Strings preserve empty strings by
default; numbers require finite decimal notation and booleans require exactly
`"true"` or `"false"`. Numeric/boolean empty strings are errors. An explicit
`empty: "omit"` lets schema defaults handle empties; `empty: "error"` rejects them.
Mark secrets with `sensitive: true` on a field or env binding, not just a suggestive
field name. Workers supply `(name) => bindings[name]` for their selected string
keys; D1/R2 bindings remain structured dependencies.

The file adapter is only in `@lenso/core/config/file`; root/env/Workers dependencies
do not import local filesystem code. Its `root` must be absolute and `path`
relative and lexically within that root. `select: ["application", "greeting"]`
selects a subobject. Missing files fail unless `optional: true` explicitly allows
`ENOENT`; permission, parsing and selection failures still fail. Path checks are
not a filesystem sandbox or a grant of access.

Custom sources need only a safe `descriptor: {id, kind, location?, fields?}` and
`async read({signal})` returning `{values, revision?}`. Grant each adapter its
needed capabilities through its own factory, not a global context. Release
temporary read resources in `finally`; no watch lifetime is started. Opaque
revision tokens are recorded separately per source, never compared, serialized
into manifests or treated as CAS/trust guarantees.

`resolveConfig(instanceId, {contract, sources}, {signal?})` also returns raw-input
top-level source history and accumulated sensitivity, separately from schema
output. It does not invent provenance for transformed/derived output fields.
`ConfigError.diagnostics` expose stable codes, instance, field path and safe
source attribution, not values, schema messages or original causes. Sources fail
closed, without implicit cache or fallback. Cancellation checks are cooperative;
an AbortSignal cannot kill arbitrary custom code.

Static inspect describes contracts and source declarations, never invokes source
reads or setup. JSON Schema is available only through an explicit
`jsonSchema: () => ...` converter. Defaults/examples are omitted and sensitive
schema subtrees are hidden. Trusted config top-level code and converters still
execute; metadata must itself be safe. See [CLI contracts](docs/CLI.md).
Notes and Tasks use these APIs in their existing applications. No configuration
center SDK, subscriptions, remote writes or hot reload are included. Future
subscription/write adapters are separate capabilities; new candidates must be
validated and explicitly applied by a lifecycle mechanism, not mutate active
plugin resources.

## Development boundaries

Use package.json for the pinned tools and scripts. The workspace has one Bun lockfile, TypeScript 7.0.2, thin Turbo orchestration and oxlint/oxfmt. Rebuild changed framework packages before running consumers: exports resolve to dist. `.lenso` and `dist` are reproducible framework-owned output; edit source/config and regenerate. Config top-level code must avoid resource acquisition. Root tests are focused; the PostgreSQL integration test requires `LENSO_TEST_DATABASE_URL` pointing to a disposable test database.

Run `bun run lint` and `bun run fmt:check` to check the repository, including templates,
scripts, root configuration and supported documentation formats. Generated output and
dependencies are ignored. Use `bun run lint:fix` for safe lint fixes and `bun run fmt`
to write formatting changes. Checks run in PR/main CI, but not automatically on save.

Lint permits local helpers and deliberate array mutation, rather than enforcing function
hoisting or ES2023 array methods. Async loop conditions may depend on state changed by
callbacks. Scoped exceptions cover MCP SDK callback slots and errors already preserved by
`AggregateError.errors`. These are explicit project choices in `.oxlintrc.json`.
The package table uses a local formatter ignore to keep its source compact; other Markdown
remains checked.

See [Releasing packages](docs/RELEASING.md) for local release checks, package validation
and the version PR/protected CI publication setup. Local release checks do not publish
anything; publication requires an explicit dispatch and configured Environment approval.

Plugins and config are trusted code, not a sandbox. CLI exposes only declared operations and does not invent a user identity. Auth adapters provide identity/policy seams, not a deployed account system. Cancellation is cooperative; detached work needs explicit ownership and registration. The included Bun server binds loopback and rejects foreign Host/Origin. Workers examples are local; cloud deployment, provider credentials, MCP/AI runtime and Rust extensions are outside this deliverable.
