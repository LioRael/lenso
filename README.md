# Lenso

A Bun-first plugin framework with ordinary async services. Core owns instance dependencies and resource cleanup. Build-time Engine extensions and optional Web, Auth, Drizzle and Workers adapters remain separate from business code. Console is an optional external consumer of explicitly selected and authorized Manage operations.

## Choose a starting point, not a directory contract

Start with the [single-package CLI or Bun Web template](templates/README.md), or
the [server/Web workspace template](templates/workspace/package.json) when the
applications have independent build or runtime boundaries. Templates recommend
organization; they are not a list of supported layouts.

A plugin can be one ordinary TS module, a workspace dependency or an npm package.
Keep business functions callable and testable without Core or Engine; setup only
connects capabilities and their owned lifetimes. Move a module by changing its
import, or move a workspace by changing its workspace declaration and dependency
locations, not the business implementation. Engine paths belong to the selected
application root; Core receives instances and explicit configuration, not folders.

Share code across applications with explicit dependencies. Each start still owns
its own resources and resolved configuration; shared code does not share a
database client, in-memory state or authority automatically. Web, Tasks, Manage
and MCP remain opt-in. A Manage declaration is not an HTTP mount or permission
grant. Keep browser runtime schemas in safe exports, and consume router-derived
public client types with `import type`, never a server initialization barrel.

## Run locally

```sh
cd lenso
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
| `@lenso/mcp` | optional stdio and authenticated Fetch MCP over explicit operations and a borrowed app | [MCP contracts and limits](packages/mcp/README.md) |
| `@lenso/web` | Fetch, oRPC and streaming request ownership | [Web API](packages/web/README.md) |
| `@lenso/auth` | provider adapters, typed middleware and shared service authorization | [Auth API](packages/auth/README.md) |
| `@lenso/db` | native Drizzle PostgreSQL, Bun SQLite and D1 resources | [Database and Notes](docs/DATABASE.md) |
| `@lenso/storage` | streaming local/S3/R2 objects and optional authorized file records | [Storage API and examples](packages/storage/README.md) |
| `@lenso/tasks` | durable PostgreSQL jobs, retries and cooperative worker lifecycle | [Tasks API](packages/tasks/README.md), [producer/worker example](examples/tasks/README.md) |
| `@lenso/workers` | request-owned Fetch app and platform bindings | [Workers API](packages/workers/README.md), [local D1 example](examples/workers/README.md) |
| `@lenso/log` | independent Pino logging, safe stderr output and active trace correlation | [Logging API](packages/log/README.md) |
| `@lenso/otel` | application-owned OpenTelemetry bootstrap, OTLP and optional oRPC/Workers entries | [Telemetry API](packages/otel/README.md) |
| `@lenso/manage` | optional instance-bound operation catalogs, agent tools and an explicitly mounted oRPC adapter | [Manage API](packages/manage/README.md) |

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

## Coding-agent skills

- [lenso-develop](.agents/skills/lenso-develop/SKILL.md): change an application's
  plugins, services, Web/Auth entries, operation exposure or Engine extensions.
  Not generic TS/Bun help, execution-only requests, framework maintenance or delivery.
- [lenso-diagnose](.agents/skills/lenso-diagnose/SKILL.md): investigate existing
  assembly, configuration, build/dev, Auth or task failures and authorized recovery.
  Not new interfaces, arbitrary SQL/migrations, secret inspection or bulk retries.

These help a **coding agent** use public packages, types, shell and CLI. A
**runtime agent** still needs an actually exposed operation, verified identity
and service authorization; loading a skill grants none of those.

The authoritative folders are `.agents/skills/lenso-develop` and
`.agents/skills/lenso-diagnose`, including their `references/`. For an external
application, obtain just these two complete folders from a reviewed repository
archive/revision containing them and copy them into the application's project-local
`.agents/skills/`. No framework checkout, global installation or tool configuration
change is required. Load `SKILL.md` explicitly if the agent does not discover that
directory. Keep both folders from the same revision; update from that source rather
than maintaining separate agent-specific copies. Skills are not included in the
current npm package file lists, and this change does not publish them.

References link to public documentation and examples on `main`. Match those
documents to the application's installed packages/lockfile before using an API;
a source merge or template is not proof of registry availability. Check actual
installed exports and build provenance: matching version strings alone do not
prove that newer configuration, listener or observability entries are present.

### Find the supported entry

<!-- prettier-ignore -->
| Task | Public entry | Documentation / example | Platform or boundary |
| --- | --- | --- | --- |
| Plugin dependencies and resource ownership | `@lenso/core` | [Core API](#core-api), [greeting plugin](examples/greeting/src/greeting.ts) | Exact instance bindings; ordinary async services |
| Validated instance configuration | `@lenso/core/config`, `/config/env`, `/config/file` | [Configuration](#instance-configuration), [Notes configuration](examples/notes/CONFIGURATION.md) | File adapter is local-host only; no config center, subscriptions or hot reload |
| Declare CLI operations or extend build/dev | `@lenso/cli`, `@lenso/engine/authoring` | [CLI](docs/CLI.md), [Engine](packages/engine/README.md), [Notes operations](examples/notes/src/operations.ts) | Bun build host; `inspect` imports trusted code, `call` starts a fresh app |
| Expose MCP operations to a runtime agent | `@lenso/mcp` `serveStdio`, `serveBorrowedStdio`, `createHttpMcp` | [MCP](packages/mcp/README.md), [borrowed host](examples/mcp-host/README.md) | Local stdio remains available; optional HTTP borrows a running app and requires a host verifier/current business authorization; no automatic listener |
| Select capabilities or mount agent/HTTP management | `@lenso/manage`, `/agent`, `/orpc` | [Manage](packages/manage/README.md), [Notes declarations](examples/notes/src/operations.ts) | Exact instance; borrowed running app; current-identity admission and trusted per-call binding; no automatic mounts or durable receipts |
| Web and verified service identity | `@lenso/web`, `@lenso/web/bun`, selected `@lenso/auth` entries | [Web](packages/web/README.md), [Auth](packages/auth/README.md), [shared Notes assembly](examples/notes/src/application.ts), [Notes server](examples/notes/src/server.ts) | Bun listener is separate from Fetch; Auth does not install login routes |
| Database, files and durable tasks | Selected `@lenso/db`, `@lenso/storage`, `@lenso/tasks` entries | [Database](docs/DATABASE.md), [Files](packages/storage/README.md), [Tasks](examples/tasks/README.md) | Bun SQL/SQLite versus Workers D1/R2; shipped durable task provider is PostgreSQL |
| Observe execution or diagnose unknown errors | `@lenso/log`, selected `@lenso/otel` entries; application's authorized status operations | [Logging](packages/log/README.md), [Telemetry](packages/otel/README.md), [Tasks query](examples/tasks/README.md#authentication-and-durable-ownership) | Stderr collector / configured telemetry backend or Workers platform; no built-in Observe query CLI |
| Use packages outside this repository | Public `exports`, real packed packages | [Minimal consumers](templates/README.md), [Workers example](examples/workers/README.md) | Templates are files, not a scaffold/install/deploy command; no framework-internal imports |

Auth migration scripts have a public `@lenso/auth/migrations/*` export.
Storage and Tasks ship migrations but do not currently export migration subpaths;
that is a separate package-boundary gap, not permission to guess internal paths.
Tasks does expose `migratePostgresTaskQueue` from `@lenso/tasks/postgres` for
explicit provisioning, as documented in [Tasks](packages/tasks/README.md#postgresql-ownership-migration-and-retention).
Use reviewed, version-matched source scripts with the application's authorized
migration workflow. Several packages omit their README from tarballs, so the public
documentation links above are the consumer route. Skills do not add a migrator,
management authority or telemetry collector.

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

Ordinary `PluginContext` implementations need not provide `config()`.
`bindConfig` callbacks receive a `ConfiguredPluginContext` with that capability;
calling a bound plugin without a preflight-capable context fails before its setup.

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

`running.configuration(plugin)` returns a copied, frozen `ConfigState` for an
exact installed instance: `state: "unconfigured" | "resolved"`, `fields` containing
raw-input `{path, sourceIds, sensitive}` metadata, and `sources` containing only
`{id, kind}`. It projects the captured startup snapshot without reading sources
again; values, revisions, file locations, env bindings and callbacks are absent.
`PluginContext.configuration` provides the same metadata for the current plugin
or its exact declared `requires`, and rejects undeclared instances. This context
method is optional in the type for compatibility with consumer-created contexts;
core always supplies it. The app accessor retains startup metadata after stop,
while context access, like `get` and `config`, rejects a stopped app.

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

Use package.json for the pinned tools and scripts. The workspace has one Bun lockfile, TypeScript 7.0.2, thin Turbo orchestration and oxlint/oxfmt. By default, package exports resolve to built JS and declarations in `dist`; rebuild changed framework packages before running default consumers.

For same-repository Bun development, explicitly opt in to the `lenso-source` export condition for `@lenso/core`, `@lenso/engine`, `@lenso/cli` and `@lenso/web`:

```sh
bun --conditions=lenso-source packages/cli/src/bin.ts inspect greeting greet --root examples/greeting --json
bun --conditions=lenso-source packages/cli/src/bin.ts call greeting greet --root examples/greeting --stdin --json
bun --conditions=lenso-source packages/cli/src/bin.ts dev --root examples/greeting
```

Supply JSON on stdin for `call`. This path runs current framework source without rebuilding those four packages; other packages still use `dist`. For matching types, add `"customConditions": ["lenso-source"]` to the consumer tsconfig with `moduleResolution: "Bundler"` or `"NodeNext"` (and a compatible `module`). Bun dev forwards custom conditions to both the Engine worker and application process. Browser consumers must use browser-safe entries such as `@lenso/core/browser`, `@lenso/web/client` and `@lenso/web/openapi-client`; enabling this condition does not make server entries browser-safe.

Published archives include these packages' `src` so opted-in exports are not broken, but only tools that support TypeScript source may opt in. The default published exports and CLI executable remain JS plus `.d.ts`; neither Bun's built-in condition nor default import/types resolution switches to source. Normal builds, package tests and release checks still verify `dist`; Turbo waits for the Engine's own build before its typecheck/tests to avoid racing `dist` removal.

`.lenso` and `dist` are reproducible framework-owned output; edit source/config and regenerate. Config top-level code must avoid resource acquisition. Root tests are focused; the PostgreSQL integration test requires `LENSO_TEST_DATABASE_URL` pointing to a disposable test database.

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
