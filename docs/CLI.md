# CLI and local agent development

Use the Rust `lenso` executable from `LioRael/lenso-cli`. The TypeScript
`@lenso/cli` package is an operation library with no executable or npm bin.
Rust starts Bun and imports `@lenso/cli/adapter` from the selected project;
this preserves Engine build/watch/setup/cleanup logic without a second CLI.
For explicit native Go/Python/Rust commands, use `--project lenso.project.json`.

Use `lenso help --json` for current commands, flags, side effects and exit codes. Finite commands support one `schemaVersion: 1` envelope on stdout: `{ok:true,data}` or `{ok:false,error}`. Logs use stderr. Exit 0 means success, 2 arguments/input failure, 3 discovery/assembly failure, 1 runtime/build/output failure. `dev --json` emits lifecycle JSONL through the Rust supervisor; ordinary dev output uses stderr. For custom entry readiness, use `reportDevReady` from `@lenso/engine/dev-ready` after successful startup.

## Select an application

`--root <directory>` remains the starting directory (cwd by default).
`--app <directory>` explicitly selects an application directory inside that root;
it is a directory, not a business registry key. Config, entry, `.lenso`, `dist`
and Engine config are then relative to the selected application root.

All application commands accept `--config <file>` to override the application
config. For example, `lenso inspect --root ./project --app apps/server --config app.ts --json`
reads `project/apps/server/app.ts` directly, without a canonical re-export.
`inspect` and `call` never import `lenso.engine.ts`; when no override is supplied,
they read `lenso.config.ts`. Engine commands use explicit config first, then their
active convention's config. `--entry` overrides `convention.entry` for both build
and dev. Without either, dev retains `src/server.ts`; build uses `.lenso/server.ts`.
Entries must exist inside selected root, using the same lexical containment check
for build and dev. These checks are host policy, not a sandbox or a prohibition
on arbitrary filesystem/process access by trusted application code.

When root has no canonical config and no explicit selection, the CLI may inspect
only directories matched by its explicit `package.json` workspaces declarations
for `lenso.config.ts`. It returns sorted `details.candidates` with
`ambiguous-application-target` (multiple candidates) or `missing-application-selection`
(zero or one). It never picks the first or imports candidate/Engine config.
Select with `--app`, or pass an explicit `--root`/`--config`. Discovery only helps
choose a trusted application; it grants no authorization and starts no resources.
Programmatic callers need no workspace container: Engine and CLI APIs accept the
existing root string or shared `ApplicationTarget` (`{root, config?}`).

### Explicit development workspace

`lenso dev --workspace <trusted TS config file>` imports a default
`defineWorkspace(...)` definition and starts its explicitly declared application
targets or argv processes. Only this explicit flag bypasses application selection;
it never discovers or registers business services. Without `--root`, paths in the
definition resolve from the workspace config's directory. With `--root`, the config
filename and service paths resolve from that explicit root. Callback code's own
relative filesystem reads still follow normal process cwd semantics; use
`import.meta.dir` or explicit absolute paths when needed.

`--workspace` is dev-only and cannot combine with `--app`, `--config`, `--entry` or
`--json`. It is trusted executable configuration, not a sandbox or authorization
boundary. Imports must not acquire resources. Optional `prepare({signal})` performs
bounded local preparation and owns its cleanup. See the
[Engine workspace API](../packages/engine/README.md#explicit-development-workspaces)
for readiness, compatible activation, explicit watches and lifecycle ownership.

Workspace service events are emitted on stderr as `[lenso:workspace] { ... }`
JSON lines alongside the human ready presentation. They include status, service ID,
candidate instance/version, actual PID, elapsed milliseconds, confirmed URLs and
retained instance/version after a failed replacement. Errors use the same safe
diagnostic projection as other CLI commands; unknown callback errors remain opaque.
These are lifecycle logs, not the finite `--json` result envelope. SIGINT/SIGTERM
cancel startup/rebuilds and stop owned builds, runtimes and Engine workers. An initial
partial failure rolls back and exits with failure; an unexpected active exit also
closes the workspace instead of leaving dependents dangling.

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

Calls validate input before setup, start one app, invoke the declared own service method with validated input and the original service as `this`, then stop. Unknown methods fail before setup. Both business and cleanup failures survive in ordered causes. No eval, automatic retry or inferred exposure occurs. Each ordinary CLI call owns a fresh app instance; HTTP normally retains an app. An effect description provides no idempotency or authorization guarantee. Keep authorization inside shared application/service rules or an already authorized public API. CLI does not invent an actor from JSON input.

Operations may also declare `destructive`, `outputDescription`, `retry`
(`safe`, `unsafe`, `unknown`) and `cancellation` (`cooperative`, `request-only`,
`none`, `unknown`). Inspect, generated manifests and optional adapters use the
same Engine description. Omitted destructive/output metadata is `null`; retry
and cancellation default to `unknown`. These are descriptions, not runtime
guarantees: no automatic retry, abort propagation or rollback is introduced.
In particular, cancelling a protocol request does not cancel a durable job.
Application-owned thin methods can adapt multi-argument services and obtain
authentication evidence from a trusted entry without accepting identity in
business input. Declare optional trusted `Operation.mapError(error)` projectors
for known domain Error classes using `instanceof`. Return a safe
`{code, phase: "invoke", message}` diagnostic or `undefined`; no global registry
or arbitrary thrown object's `code` is trusted. Service and binding failures use
this projector, not input validation, entry gates, output or cleanup failures.
Projector failures retain the original error. Existing `CliError`/`EngineError`
extensions remain supported; unknown service errors remain opaque.

Methods needing request evidence or an Auth-produced actor may declare
`context: true`; its type comes from the real method's second parameter.
CLI's named `operationBinding` (or programmatic `invoke` binding) supplies that
context after raw input validation and setup. Use
`OperationBinding<typeof operations[number]>` to retain the contextual type.
MCP uses only its explicit launch `binding`, never the CLI binding. Context is not
input JSON, and credentials are verified by Auth/service rules on every call.
Single-input methods need no binding. Passing a signal does not establish
cooperative cancellation; existing cancellation metadata remains descriptive.

Engine's own signal checks report `aborted` with the original reason retained
only as an in-process cause. A service rejection is cancellation only when it
is the exact object reason of that invocation's aborted signal; equal primitive
values and arbitrary exception names/codes do not establish ownership. Engine's
own checks support primitive reasons too. Engine awaits service settlement rather than
releasing resources on abort. CLI uses its diagnostic envelope, MCP uses its
protocol errors, and Manage maps `aborted` to `CLIENT_CLOSED_REQUEST`/499.
MCP may answer an interrupted request before work settles, but retains the
execution slot and drains actual work before closing. None of these responses
proves that external effects were undone.

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
not elevated authority. Its opt-in HTTP Fetch entry borrows a running app and
requires host-injected verified authentication plus the same service policies.
No listener, scope/audience credential issuance or authorization shortcut is supplied.

## Declared output and error contracts

An Operation may provide `output: sharedOutputSchema` using the same Standard Schema
object as its oRPC `.output(...)` route. The shared executor validates service output
before redaction/JSON serialization; this is validation after invocation, not rollback.
`errors: [{code, message}]` describes safe business failures. Existing `mapError` still
classifies real domain Error instances; declaring an error does not trust arbitrary
thrown objects, create handlers, or bypass service policy. `outputDescription` remains
prose. Inspect adds `outputSchema`, `outputSchemaAvailability` (`not-declared`,
`available`, `runtime-validation-only`) and `errors`. MCP preserves existing tool names,
projects a convertible output schema, and wraps scalar/array results as `{result}` in
structured content. Missing converters never fabricate schemas.

Diagnostics preserve safe `details.paths`, operation/source and phase. `effectState`
records invocation provenance: `not-started` means this operation was not dispatched,
not that configuration/setup had no effects; `unknown` follows dispatch failures,
output failures, cancellation or lost results; `completed` only records a settled
successful invocation. No value proves external durability or rollback.

## Connect to one ready development instance

Availability: this is a coordinated source capability of Engine, CLI operation library
and native CLI. Package version strings have not been bumped; the published CLI v0.2.0
and adapter 0.21.0 alone do not supply this path. Select matching reviewed source builds
and inspect installed exports/help. No framework pack/vendor override or Runtime is
required by ordinary TS business services. Source consumers still rebuild `dist`.

An entry explicitly attaches the same app it already lends to Web/MCP:

```ts
import { serveDevOperations } from "@lenso/cli";
const connection = await serveDevOperations({
  running: app, plugins, operations, binding: operationBinding,
});
reportDevReady({ connection: connection?.connection, urls: [url] });
// Owner shutdown: await connection?.close(), drain other borrowed entries, then app.stop().
```

The helper creates no listener unless the existing dev supervisor opted in. It borrows
exact installed instances and the existing trusted launch binding, validates shared input,
and invokes the shared executor; it neither starts/stops the app nor bypasses business
identity/tenant policy. Only a same-user private Unix socket is used; no TCP listener,
credential issuance, auto-attachment, shell/eval or cross-language Runtime is added.
Current support is Bun on Unix; non-TS native adapters explicitly return `unsupported`.

```sh
lenso dev --root examples/greeting --connect .lenso/dev.json
# Other terminal; use an explicit absolute descriptor path if cwd differs:
lenso inspect --root examples/greeting --connect /absolute/examples/greeting/.lenso/dev.json --json
lenso call greeting greet --root examples/greeting --connect /absolute/examples/greeting/.lenso/dev.json --generation GENERATION_FROM_INSPECT --stdin --json
lenso recent greeting greet --root examples/greeting --connect /absolute/examples/greeting/.lenso/dev.json --generation GENERATION_FROM_INSPECT --limit 16 --json
```

Ordinary calls retain their fresh-app behavior. Connected commands import no target
config and never fall back to starting an app. Calls require the inspected generation;
stale/missing generations refuse dispatch. A descriptor is published only after real
runtime readiness and Engine ready hooks; entries without the explicit host fail with
`dev-connection-unsupported`. An existing unowned descriptor is not replaced. Restart
revokes admission, removes the old descriptor, stops the owned runtime, and replaces the
generation. Shutdown drains cooperative work; a noncooperative runtime may reach the
existing forced-stop timeout, with effects unknown. The supervisor removes its own
socket directory even after forced termination.

Static inspect retains `inspection: static` and reports checked/not-checked scope.
Connected inspect reports `inspection: running`, instance and generation. Installed
versions, watched-input sample digest and generation are distinct: the pre-launch digest
is not loaded-byte attestation, a Git release SHA or proof of matching registry artifacts.
Provider health and business authorization are not inferred from ready status.

`recent` requires the inspected generation and optional plugin/method selection (default limit 16, maximum 64). It is only in-memory metadata for this connection generation: at most 64 records,
5-minute query window, stable callRef, operation, call-time instance/generation/watch-input revision, start/duration/code/phase/effectState/outcome. It stores no
business input, output, context, arbitrary errors or credentials, survives no restart and
is not an Observe backend, durable receipt, retry journal or evidence of an external
commit. One connected request is admitted, no queue/retry; input is capped at 256 KiB,
output at 1 MiB, with a 30s binding signal. The optional fourth binding argument is `{signal, generation}`; existing three-argument `OperationBinding` functions remain compatible. Cancellation reaches the trusted binding;
ordinary methods are not automatically cooperative. The owner drains actual work.

## Short feedback path

1. Choose one consistent resolution path. Default consumers require rebuilt framework `dist`; use package.json for scripts. Same-repository Bun development can instead run `BUN_OPTIONS=--conditions=lenso-source lenso ...` to use current core/Engine/CLI/Web source without rebuilding those packages. Add `"customConditions": ["lenso-source"]` to the consumer tsconfig (`moduleResolution: "Bundler"` or `"NodeNext"` with a compatible `module`) for matching types. Other packages still require built `dist`; this does not change default published JS/declaration exports.
2. Inspect the operation, then change its shared schema or service source.
3. Run the app's typecheck and a focused test. Invoke with `--input-file <path>` or `--stdin` to avoid secrets in shell history; inline JSON remains available for nonsensitive input.
4. If an already owned HTTP server is relevant, verify its typed client too. Use only owned ports/processes.

For example, run `BUN_OPTIONS=--conditions=lenso-source lenso inspect greeting greet --root examples/greeting --json`, then pipe nonsensitive input to `BUN_OPTIONS=--conditions=lenso-source lenso call greeting greet --root examples/greeting --stdin --json`. The same flag works with `dev` and is forwarded to Engine/application subprocesses. Source exports are shipped, but require a TypeScript-capable runtime or bundler; browser clients must still select browser-safe subexports (`@lenso/core/browser`, `@lenso/web/client`, `@lenso/web/openapi-client`).

Engine's bundle helper also inherits the process's custom conditions, including
`lenso-source` when explicitly enabled. A target's `bundle({conditions: [...]})`
replaces that custom list; `conditions: []` requests default publication
resolution even from a source-development process. Bun still supplies the selected
platform's built-in conditions. Custom targets calling Bun directly can use
`BuildContext.conditions`. With `packages: "external"`, package imports remain
external and the process running the artifact must choose its runtime conditions;
bundling conditions cannot change that later resolution.

SDK lifetime is serial setup and synchronous cleanup registration, global LIFO sequential cleanup, startup rollback and aggregated cleanup failures. `onCleanup` returns an async disposer sharing one completion with shutdown, even after early failure. `stop()` caches one Promise even on failure. Original setup/cleanup errors keep their identity; `lifecycleFailure(error)` adds attribution for object errors without mutating them. Primitive thrown values have only the containing phase. Cancellation, request drain and detached task ownership remain the host/application's responsibility. A finalizer must not await its own disposer or its app's stop Promise.

## Diagnostics and output

Assembly codes are `duplicate-id`, `missing-dependency`, `cyclic-dependency`, `invalid-id`, `invalid-source` and `invalid-plugin`. CLI errors include code/phase/message, with optional pluginId/operation/source/details/ordered causes. `help --json` lists the CLI codes. Unknown application error text, stack traces and raw input are omitted because they may contain secrets. Input validation details expose only declared static JSON Schema property paths, not rejected values, dynamic record keys, array indices or runtime-only/unconvertible paths. Paths are limited to 32 issues, eight segments and 128 characters per segment. Diagnostic traversal bounds depth, count and cycles; details are projected only for known codes. Internal causes remain on in-process errors, without repeating raw domain failures in public diagnostics. Config import errors report the config path; build diagnostics report Bun's source positions where available.

`inspect` reports `inspection: "static"`, ordered enabled plugin instances,
their mandatory `requires` instance bindings, existing contributions and explicit
CLI operations. Plugin `source` metadata is used when declared; the real config
path is the fallback. Missing dependencies name the consumer and missing
instance; duplicate IDs identify available declaration locations. Contributions
use the existing redaction policy, not a second configuration renderer.
Assembly `details.path` identifies the declaration field (for example,
`["plugins", 2, "requires", 0]`); duplicate IDs include both declaration
`details.paths`, and cycles include each participating dependency edge. These
indices refer to the declared plugin array, before dependency sorting. Repair
hints use static text and do not include rejected configuration values.
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

`generate` writes `.lenso/manifest.json`, `server.ts` and `client.ts` by default. Build plugins can add owned files; the command reports their paths from `.lenso/.engine-files.json`. Conflicting owners, edited outputs and path/symlink escapes fail before overwriting files. See [Engine authoring](../packages/engine/README.md) for `lenso.engine.ts`, discovery/generation hooks, explicit capability replacement and custom build targets. Manifest schemaVersion is 1, contains plugin/dependency and operation descriptions, and identifies the selected config as source. Keys are sorted; unchanged bytes are not rewritten. Missing router produces an empty client entry, removing stale imports. Metadata must be plain JSON and contain no secrets; sensitive fields are redacted. No timestamps or setup results enter generated output. Determinism depends on deterministic config metadata; import-time randomness, environment-sensitive declarations and module caches remain app-owned constraints. A new CLI process loads current source; repeated in-process discovery follows normal module caching. Edit source/config, then regenerate; do not hand-edit generated files.

Engine setup is build-time trusted code: check/generate/build execute it, while inspect/call read only the selected application config and never load Engine config. Dev runs generation and beforeStart in a fresh supervised worker, passing the explicit config and entry overrides through that worker. The worker's active convention supplies the entry when none is explicit. Ready hooks execute only after the application IPC signal. Dev stops the runtime before awaiting Engine LIFO cleanup on restart or shutdown. Static imports and explicit existing file/directory watches invalidate the next cycle; generated/cache paths are excluded. Dynamic reads require `context.watch`. Worker startup is limited to 30 seconds and shutdown to 5 seconds; forced termination reports a diagnostic.

## Explicit first-run setup (source candidate)

Applications may opt into the separate [initialization contract](INITIALIZATION.md).
`setup status` performs authorized runtime readiness reads; `setup apply --stdin`
(or `--input-file`) validates `{requestId,input}`, commits the app-owned first-run
transaction, and attempts separately authorized activation. `setup activate`
resumes a committed pending revision without another password or owner creation.
No inline credential input is accepted. The app supplies a trusted
`initializationBinding`; JSON actor/owner flags grant no authority.

This entry never starts the incomplete business app or Engine. The same explicit
initializer is a named config export and the default app's optional startup gate.
No bootstrap route, user role, secret backend or permission is enabled merely by
installing the SDK. Missing current authorization, uncertain database state,
conflicting retry input and an already completed first-owner claim fail closed.
CLI errors use the existing safe finite JSON envelope; handled non-ready status
results still require checking `data.state` before treating setup as complete.

The new entry requires matching unreleased TS/native candidates. Released native
v0.2.0 and registry Core 0.3.1 do not contain this setup feature. Native binaries
retain their own publication lifecycle; no release is performed by these docs.
