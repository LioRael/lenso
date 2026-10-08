# CLI and local agent development

Use `lenso help --json` for current commands, flags, side effects and exit codes. Finite commands support one `schemaVersion: 1` envelope on stdout: `{ok:true,data}` or `{ok:false,error}`. Logs use stderr. Exit 0 means success, 2 arguments/input failure, 3 discovery/assembly failure, 1 runtime/build/output failure. `dev --json` is unsupported; its human lifecycle is managed separately.

## Declare existing service operations

The trusted `lenso.config.ts` default export remains `defineApp({plugins})`. An optional named export `operations` is the CLI allowlist. Reuse the application's existing schema and service; do not implement another business handler. For example:

```ts
import { defineApp } from 'lenso';
import { defineOperation } from 'lenso-cli';
import { greeting } from './src/greeting';
import { greetingInput } from './src/contracts'; // also used by Web input validation

export const operations = [defineOperation({
  plugin: greeting,
  method: 'greet',
  input: greetingInput,
  description: 'Greet a name using the ordinary service.',
  effect: 'write', // greeting increments an in-memory count
  source: { file: 'src/greeting.ts', export: 'greeting' },
})];
export default defineApp({ plugins: [greeting] });
```

`defineOperation` checks the service method's input type. The schema uses Standard Schema v1, including Zod 4. `inspect [plugin-id [method]]` derives JSON Schema from that same object's Standard JSON Schema converter. If conversion is absent or unsupported, it reports `runtime-validation-only` and `inputSchema:null`; it never starts resources to infer methods. Source locations are explicit declaration metadata, otherwise the exact config file; line/column are omitted unless supplied or reported by Bun build diagnostics.

Calls validate input before setup, start one app, invoke the declared own service method with validated input and the original service as `this`, then stop. Unknown methods fail before setup. Both business and cleanup failures survive in ordered causes. No eval, automatic retry or inferred exposure occurs. Each CLI call owns a fresh app instance; HTTP normally retains an app. An effect description provides no idempotency or authorization guarantee. Keep authorization inside shared application/service rules or an already authorized public API. CLI does not invent an actor from JSON input.

## Short feedback path

1. Build changed framework packages so exports resolve current dist; use package.json for scripts.
2. Inspect the operation, then change its shared schema or service source.
3. Run the app's typecheck and a focused test. Invoke with `--input-file <path>` or `--stdin` to avoid secrets in shell history; inline JSON remains available for nonsensitive input.
4. If an already owned HTTP server is relevant, verify its typed client too. Use only owned ports/processes.

SDK lifetime is serial setup and synchronous cleanup registration, global LIFO sequential cleanup, startup rollback and aggregated cleanup failures. `stop()` caches one Promise even on failure. Original setup/cleanup errors keep their identity; `lifecycleFailure(error)` adds attribution for object errors without mutating them. Primitive thrown values have only the containing phase. Cancellation, request drain and detached task ownership remain the host/application's responsibility. A finalizer must not await its own app's stop Promise.

## Diagnostics and output

Existing assembly codes remain `duplicate-id`, `missing-dependency`, `cyclic-dependency`, `invalid-id`. CLI errors include code/phase/message, with optional pluginId/operation/source/details/ordered causes. `help --json` lists the CLI codes. Unknown application error text, stack traces and raw input are omitted because they may contain secrets; validation details expose paths, not rejected values. Config import errors report the config path; build diagnostics report Bun's source positions where available. In-process engine APIs retain causes where possible; the JSON boundary emits safe diagnostic data.

The CLI routes trusted application console methods to stderr, redacts sensitive keys, credential-bearing URLs, authorization strings and known secret environment values, and redacts results. Applications must still avoid printing sensitive values; arbitrary direct stdout writes/native logs and secrets under innocuous keys cannot be reliably isolated in this trusted in-process model. This is not a sandbox. Returned data must be acyclic plain finite JSON; unsupported values produce `serialization-failed`, including an undefined result.

## Generated ownership

`generate` writes only `.lenso/manifest.json`, `server.ts` and `client.ts`. Manifest schemaVersion is 1, contains plugin/dependency and operation descriptions, and identifies `lenso.config.ts` as source. Keys are sorted; unchanged bytes are not rewritten. Missing router produces an empty client entry, removing stale imports. Metadata must be plain JSON and contain no secrets; sensitive fields are redacted. No timestamps or setup results enter generated output. Determinism depends on deterministic config metadata; import-time randomness, environment-sensitive declarations and module caches remain app-owned constraints. A new CLI process loads current source; repeated in-process discovery follows normal module caching. Edit source/config, then regenerate; do not hand-edit generated files.
