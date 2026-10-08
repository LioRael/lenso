# CLI and local agent development

Use `lenso help --json` for current commands, flags, side effects and exit codes. Finite commands support one `schemaVersion: 1` envelope on stdout: `{ok:true,data}` or `{ok:false,error}`. Logs use stderr. Exit 0 means success, 2 arguments/input failure, 3 discovery/assembly failure, 1 runtime/build/output failure. `dev --json` is unsupported; its human lifecycle is managed separately. For custom entry readiness, use `reportDevReady` from `@lenso/engine/dev-ready` after successful startup.

## Declare existing service operations

The trusted `lenso.config.ts` default export is `defineApp({plugins})`. An optional named export `operations` is the CLI allowlist; the default export must not contain `operations`. Reuse the application's existing schema and service; do not implement another business handler. For example:

```ts
import { defineApp } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { greeting } from "./src/greeting";
import { greetingInput } from "./src/contracts"; // also used by Web input validation

export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    input: greetingInput,
    description: "Greet a name using the ordinary service.",
    effect: "write", // greeting increments an in-memory count
    source: { file: "src/greeting.ts", export: "greeting" },
  }),
];
export default defineApp({ plugins: [greeting] });
```

For a service with several operations, its plugin or a companion factory can return
`{ plugin, operations }`. Use `defineOperation` inside that factory with the exact
plugin object, install `plugin`, and explicitly export the chosen declarations as
the config's named `operations`. Notes uses this pattern in
[its companion factories](../examples/notes/src/operations.ts); it does not automatically expose service methods
or add anything to the separate MCP allowlist. Keep functions and schemas in code,
not JSON contributions or manifests.

Optional [`@lenso/manage`](../packages/manage/README.md) companion factories bind
an explicit subset of those same Operations to the exact plugin instance.
`selectManageOperations` returns the original declarations, without copying schemas
or introducing handlers. Notes and Tasks use this pattern. A manage declaration
alone exposes nothing: named `operations` selects CLI; named `mcpOperations`
independently selects MCP, whose trusted launch `allow` list still applies.
For existing configs without `mcpOperations`, MCP retains the `operations` fallback;
an explicit `mcpOperations = []` disables it. Agent/HTTP adapters select their own
lists and borrow an explicitly supplied running app.

`defineOperation` checks the service method's input type. The schema uses Standard Schema v1, including Zod 4. `inspect [plugin-id [method]]` derives JSON Schema from that same object's Standard JSON Schema converter. Field names/types remain discoverable, including credential fields; payload defaults/examples are omitted. If conversion is absent or unsupported, it reports `runtime-validation-only` and `inputSchema:null`; it never starts resources to infer methods. Source locations are explicit declaration metadata, otherwise the exact config file; line/column are omitted unless supplied or reported by Bun build diagnostics.

Calls validate input before setup, start one app, invoke the declared own service method with validated input and the original service as `this`, then stop. Unknown methods fail before setup. Both business and cleanup failures survive in ordered causes. No eval, automatic retry or inferred exposure occurs. Each CLI call owns a fresh app instance; HTTP normally retains an app. An effect description provides no idempotency or authorization guarantee. Keep authorization inside shared application/service rules or an already authorized public API. CLI does not invent an actor from JSON input.

Operations may also declare `destructive`, `outputDescription`, `retry`
(`safe`, `unsafe`, `unknown`) and `cancellation` (`cooperative`, `request-only`,
`none`, `unknown`). Inspect, generated manifests and optional adapters use the
same Engine description. Omitted destructive/output metadata is `null`; retry
and cancellation default to `unknown`. These are descriptions, not runtime
guarantees: no automatic retry, abort propagation or rollback is introduced.
In particular, cancelling a protocol request does not cancel a durable job.
Application-owned thin methods can adapt multi-argument services and obtain
authentication evidence from a trusted entry without accepting identity in
business input. Map known authorization errors to safe `CliError` codes at
that application boundary; unknown service errors remain opaque.

Methods needing request evidence or an Auth-produced actor may declare
`context: true`; its type comes from the real method's second parameter.
CLI's named `operationBinding` (or programmatic `invoke` binding) supplies that
context after raw input validation and setup. Use
`OperationBinding<typeof operations[number]>` to retain the contextual type.
MCP uses only its explicit launch `binding`, never the CLI binding. Context is not
input JSON, and credentials are verified by Auth/service rules on every call.
Single-input methods need no binding. Passing a signal does not establish
cooperative cancellation; existing cancellation metadata remains descriptive.

`confirmation: "required"` and `approval: "required"` refuse invocation unless
the trusted entry binding supplies a successful `confirm`/`approve` callback.
Those callbacks must verify the specific invocation through a real confirmation
flow or explicit approval owner. JSON flags do not satisfy either requirement.
Allowlist, permission, confirmation and approval are independent; destructive
metadata grants none of them. Shared calls never retry unknown write outcomes.
Output is finite, redacted JSON with a default 1 MiB budget; programmatic bindings
may select a budget, while MCP always applies its configured host limit.

The local developer controls trusted config, application root, code and launch
environment, but a declared business call still follows the shared actor and
object/tenant policies. Local code/DB administration is not a permission exposed
to a tool. A stdio process uses its launch identity; it does not authenticate a
different remote user per request. Keep credentials in the trusted environment,
not arguments, schemas or discovery metadata. The optional
[`@lenso/mcp`](../packages/mcp/README.md) adapter adds a fixed operation allowlist,
not elevated authority. Remote ingress requires its own verified authentication
and the same service policies; no remote MCP listener, scope/audience credential
issuance or authorization shortcut is supplied.

## Short feedback path

1. Build changed framework packages so exports resolve current dist; use package.json for scripts.
2. Inspect the operation, then change its shared schema or service source.
3. Run the app's typecheck and a focused test. Invoke with `--input-file <path>` or `--stdin` to avoid secrets in shell history; inline JSON remains available for nonsensitive input.
4. If an already owned HTTP server is relevant, verify its typed client too. Use only owned ports/processes.

SDK lifetime is serial setup and synchronous cleanup registration, global LIFO sequential cleanup, startup rollback and aggregated cleanup failures. `onCleanup` returns an async disposer sharing one completion with shutdown, even after early failure. `stop()` caches one Promise even on failure. Original setup/cleanup errors keep their identity; `lifecycleFailure(error)` adds attribution for object errors without mutating them. Primitive thrown values have only the containing phase. Cancellation, request drain and detached task ownership remain the host/application's responsibility. A finalizer must not await its own disposer or its app's stop Promise.

## Diagnostics and output

Assembly codes are `duplicate-id`, `missing-dependency`, `cyclic-dependency`, `invalid-id` and `invalid-source`. CLI errors include code/phase/message, with optional pluginId/operation/source/details/ordered causes. `help --json` lists the CLI codes. Unknown application error text, stack traces and raw input are omitted because they may contain secrets; validation details expose paths, not rejected values. Config import errors report the config path; build diagnostics report Bun's source positions where available. In-process engine APIs retain causes where possible; the JSON boundary emits safe diagnostic data.

`inspect` reports `inspection: "static"`, ordered enabled plugin instances,
their mandatory `requires` instance bindings, existing contributions and explicit
CLI operations. Plugin `source` metadata is used when declared; the real config
path is the fallback. Missing dependencies name the consumer and missing
instance; duplicate IDs identify available declaration locations. Contributions
use the existing redaction policy, not a second configuration renderer.
Inspection never loads `lenso.engine.ts`, runs Engine/application setup or infers
runtime service methods/health. Trusted config top-level code and schema converters
still execute. It does not dump resolved runtime configuration or override
provenance. JSON Schema defaults/examples are omitted, but validation constraints
such as `const`/`enum` remain discoverable; do not embed secrets in schema metadata.

### Runtime plugin configuration

An opted-in plugin's `config` description identifies its contract, safe field
metadata, ordered source IDs/kinds/locations and explicitly bound env names.
This is the same application declaration used by `startApp`, not Engine's own
build configuration. Inspect/generate never invoke configuration `source.read`,
resolve file paths to contents, fetch env bindings or connect remote sources.
Existing trusted config top-level code and converters can still perform their
own side effects; inspection is not a sandbox. The CLI's existing launch-env
redaction is separate from configuration source evaluation.

Configuration JSON Schema requires an explicit contract converter; Standard
Schema alone promises validation, not conversion. All defaults/examples are
omitted. A sensitive field or source binding hides its entire top-level schema
subtree, conservatively omitting reference/combinator definitions that could
repeat sensitive annotations. When any field is sensitive, converter output is
shape-only: value-carrying constraints and extensions are omitted throughout.
Runtime values, opaque revisions and resolved
override history never enter inspect output or generated manifests.

`call` validates operation input first, then `startApp` resolves **all installed
configured instances** before any business setup, just like direct/Web/Workers
startup. Configuration failures use phase `config`, top-level code
`config-invalid`, and ordered safe causes with instance, field path and source
ID/location. Source codes include `config-source-failed`, `config-invalid-data`,
`config-env-invalid`, `config-file-missing`, `config-file-invalid` and
`config-cancelled`. Exit status is 1; this is distinct from trusted TS config
import/assembly failure (3) or operation input failure (2).

Preflight reads can perform I/O; custom sources own their temporary resource
cleanup. Failed sources do not silently fall back. See the
[configuration API and source semantics](../README.md#instance-configuration).
Configuration validity grants no Auth, filesystem or network authority.

The CLI routes trusted application console methods to stderr, redacts sensitive keys, credential-bearing URLs, authorization strings and known secret environment values, and redacts results. Applications must still avoid printing sensitive values; arbitrary direct stdout writes/native logs and secrets under innocuous keys cannot be reliably isolated in this trusted in-process model. This is not a sandbox. Returned data must be acyclic plain finite JSON; unsupported values produce `serialization-failed`, including an undefined result.

Command and declared operation spans use only the OpenTelemetry API. Initialize
the application's SDK **before** the CLI/config via an explicit Bun preload
using `@lenso/otel/bun`; `flushOnCliExit: true` finishes bounded export in the CLI's
`finally` path. Export failure reports a fixed stderr diagnostic without changing
the business exit code. Existing SDK owners use external mode and retain their
own flush/shutdown responsibility. No SDK, exporter, signal/exception hook or
credentials are configured by importing CLI/core.
An application can supply `defineApp({plugins, logger, instanceId})` using its
existing logger; keep configuration imports free of resource acquisition.
`@lenso/log` defaults to stderr JSON and enriches current trace/span fields;
collect that stream once rather than also exporting it through an OTel log SDK.

## Generated ownership

`generate` writes `.lenso/manifest.json`, `server.ts` and `client.ts` by default. Build plugins can add owned files; the command reports their paths from `.lenso/.engine-files.json`. Conflicting owners, edited outputs and path/symlink escapes fail before overwriting files. See [Engine authoring](../packages/engine/README.md) for `lenso.engine.ts`, discovery/generation hooks, explicit capability replacement and custom build targets. Manifest schemaVersion is 1, contains plugin/dependency and operation descriptions, and identifies `lenso.config.ts` as source. Keys are sorted; unchanged bytes are not rewritten. Missing router produces an empty client entry, removing stale imports. Metadata must be plain JSON and contain no secrets; sensitive fields are redacted. No timestamps or setup results enter generated output. Determinism depends on deterministic config metadata; import-time randomness, environment-sensitive declarations and module caches remain app-owned constraints. A new CLI process loads current source; repeated in-process discovery follows normal module caching. Edit source/config, then regenerate; do not hand-edit generated files.

Engine setup is build-time trusted code: check/generate/build execute it, while inspect/call read canonical `lenso.config.ts` and never load Engine config. Dev runs generation and beforeStart in a fresh supervised worker, then executes ready hooks only after the application IPC signal. It stops the runtime before awaiting Engine LIFO cleanup on restart or shutdown. Static imports and explicit existing file/directory watches invalidate the next cycle; generated/cache paths are excluded. Dynamic reads require `context.watch`. Dev entry is `--entry` or `src/server.ts`; build convention entry only affects build. Worker startup is limited to 30 seconds and shutdown to 5 seconds; forced termination reports a diagnostic.
