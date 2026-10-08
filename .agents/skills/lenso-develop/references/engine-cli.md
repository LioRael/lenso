# Engine and explicit exposure

Read [CLI development](https://github.com/LioRael/lenso/blob/main/docs/CLI.md) when changing input, exposure, diagnostics or generated files.

## Declare a service operation

- `lenso.config.ts` default-exports `defineApp({plugins})`; its separate named `operations` export is the CLI allowlist. Use `defineOperation` from `@lenso/cli` with the exact installed plugin, own callable service method and shared Standard Schema input.
- Actor/evidence-aware methods can declare `context: true`; the context type comes from the real second parameter. CLI's named `operationBinding` supplies trusted context after input validation/setup; MCP uses only its explicit launch `binding`. Ordinary single-input wrappers remain supported. Follow [Notes companion factories](https://github.com/LioRael/lenso/blob/main/examples/notes/src/operations.ts) and [Tasks declarations](https://github.com/LioRael/lenso/blob/main/examples/tasks/lenso.config.ts). Map known authorization errors to safe `CliError` codes at that boundary.
- Exposure is opt-in, separate from permission. `effect`, `destructive`, `retry` and `cancellation` describe semantics; they do not authorize, retry, propagate abort or roll back. `outputDescription` is prose, not an output schema.
- CLI input validates before setup; a valid call starts one app, invokes with the service as `this`, and awaits stop. Return finite acyclic plain JSON, including an explicit value instead of `undefined`. Keep secrets out of schema metadata, constraints and output.

For a changed declaration, use the installed application's CLI entry:

```sh
lenso inspect <plugin-id> <method> --root <app> --json
lenso call <plugin-id> <method> --root <app> --stdin --json
```

Supply safe JSON on stdin; `--input-file <file>` or nonsensitive inline JSON are alternatives. `inspect` also accepts no selectors or only a plugin ID. Use `lenso help --json` for installed command/flag support: this baseline has help/check/inspect/generate/build/call/dev, and `dev --json` is unsupported. Inspect imports trusted canonical config and converters without setup; it is not a sandbox.

Pair the call with the app's focused typecheck/test. Verify invalid input and undeclared methods fail before setup; use an authorized disposable environment because the call starts the whole app.

## MCP and Manage

Read [Manage](https://github.com/LioRael/lenso/blob/main/packages/manage/README.md) when declaring instance-bound capabilities or adding its agent/HTTP adapter. `defineManage` binds chosen existing Operations to one exact plugin; `selectManageOperations` returns those same declarations. Select CLI `operations` and MCP `mcpOperations` independently; declaration alone opens no entry. Without `mcpOperations`, MCP falls back to `operations`; an explicit empty list disables that fallback.

`createManageAdapter` borrows an explicitly running app, never starts/stops it. Supply per-call trusted `binding` and current-identity `canList`; service Auth/object policy remains mandatory. `createAgentTools` requires convertible object input. `/orpc` supplies explicitly mounted routes using request evidence, not JSON actors. Catalog keys are adapter-scoped, not durable receipts. Required confirmation/approval needs real trusted callbacks and fails closed without them; input flags and descriptive metadata cannot satisfy those gates.

Read the [local MCP contract](https://github.com/LioRael/lenso/blob/main/packages/mcp/README.md) before adding stdio tools. `serveStdio` owns a dedicated local Bun process with fixed trusted root and additional launch allowlist. Input must convert to an SDK-compatible object schema; keep stdout protocol-only. Calls use launch identity, not per-request remote actors. One call is admitted; concurrency returns `adapter-busy`, with no queue. SDK cancellation is checked before/after invocation, not automatically propagated to the business method; it remains request-only, not durable cancellation or rollback. Shutdown drains work and cleanup.

Verify installed Manage/CLI/MCP exports before using these newer bindings. No generic management CLI, automatic retries, durable receipts, config read-all/write-all or hot restart is supplied.

## Application Engine extensions

Use [Engine authoring](https://github.com/LioRael/lenso/blob/main/packages/engine/README.md#engine-plugins) and its linked standalone plugin examples. Author through `@lenso/engine/authoring` in `lenso.engine.ts`, separate from runtime instance config and browser/runtime imports.

- check/generate/build run trusted **Engine** setup, not application setup; inspect/call use canonical `lenso.config.ts`, never Engine config. Preserve that config or a re-export even with a custom convention.
- Register named capabilities; replacement names the exact current owner and requires appropriate ordering. Return generated paths relative to `.lenso`, respecting `.engine-files.json` ownership, edited-output and path/symlink protections.
- Register dynamic reads with `context.watch`; explicit watches must exist. Dev convention entry differs from build: use `--entry` or `src/server.ts`, and report actual startup via `@lenso/engine/dev-ready`. Runtime stops before Engine cleanup.
