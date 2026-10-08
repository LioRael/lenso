# Engine and explicit exposure

Read [CLI development](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/docs/CLI.md) when changing input, exposure, diagnostics or generated files.

## Declare a service operation

- `lenso.config.ts` default-exports `defineApp({plugins})`; its separate named `operations` export is the CLI allowlist. Use `defineOperation` from `@lenso/cli` with the exact installed plugin, own callable service method and shared Standard Schema input.
- Multi-argument/actor-aware services need thin application-owned single-input methods that obtain identity from the trusted entry, then delegate. Follow [Notes companion factories](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/notes/src/operations.ts) and [Tasks declarations](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/tasks/lenso.config.ts). Map known authorization errors to safe `CliError` codes there.
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

Read the [local MCP contract](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/mcp/README.md) before adding tools. `serveStdio` from `@lenso/mcp` owns a dedicated local Bun process with a fixed trusted root and separate explicit allowlist of already declared operations. Require a converted object input schema that also fits the SDK representation; keep stdout protocol-only.

All calls use launch identity, not per-request remote actors. One call is admitted at a time; concurrency returns `adapter-busy`, with no queue. Cancellation is checked before/after the CLI call, never propagated to the service: it is request-only, not durable task cancellation or rollback. Shutdown drains admitted work and cleanup.

No public Manage SDK or CLI command exists in this baseline. If requested, check the installed exports/help for a documented supported entry; absent one, report the gap and ask for the intended supported integration. Do not invent a Manage interface or make consumers implement missing framework APIs.

## Application Engine extensions

Use [Engine authoring](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/engine/README.md#engine-plugins) and its linked standalone plugin examples. Author through `@lenso/engine/authoring` in `lenso.engine.ts`, separate from runtime instance config and browser/runtime imports.

- check/generate/build run trusted **Engine** setup, not application setup; inspect/call use canonical `lenso.config.ts`, never Engine config. Preserve that config or a re-export even with a custom convention.
- Register named capabilities; replacement names the exact current owner and requires appropriate ordering. Return generated paths relative to `.lenso`, respecting `.engine-files.json` ownership, edited-output and path/symlink protections.
- Register dynamic reads with `context.watch`; explicit watches must exist. Dev convention entry differs from build: use `--entry` or `src/server.ts`, and report actual startup via `@lenso/engine/dev-ready`. Runtime stops before Engine cleanup.
