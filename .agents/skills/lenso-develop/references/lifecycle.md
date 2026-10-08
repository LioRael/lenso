# Lifecycle and instance configuration

Read [Core API and instance configuration](https://github.com/LioRael/lenso/blob/main/README.md#core-api) for the changed contract; [Notes service/plugin](https://github.com/LioRael/lenso/blob/main/examples/notes/src/notes.ts) shows structural `Plugin<Service>` dependencies and separate business functions.

- Declare mandatory edges only for supplied providers. An optional provider needs an explicit absent branch, not an undeclared `get` or implicit provider selection. Preserve exact instances through factories and assembly.
- Setup is serial; registered cleanup is sequential global LIFO, including startup rollback. Every finalizer is attempted and failures aggregate. `onCleanup` returns an async disposer: use that for early release so explicit release and shutdown share one completion. Repeated `stop()` shares one Promise, including failures.
- Registration is setup-only. A finalizer must not await its own disposer or its application's `stop()`. Core does not infer ownership of listeners, detached work, request drain or cancellation from a returned service.
- For lifecycle changes, exercise failed setup and cleanup as well as normal stop, checking that owned resources release once and borrowed resources remain usable.

## Opt into typed config when needed

Ordinary factory options remain valid. For validated, multi-source instance configuration, use `definePluginConfig`/`bindConfig` from `@lenso/core/config`; explicit environment/file adapters live at `/config/env` and `/config/file`. Follow [Notes listener configuration](https://github.com/LioRael/lenso/blob/main/examples/notes/src/server.ts) or [Tasks configuration](https://github.com/LioRael/lenso/blob/main/examples/tasks/src/config.ts).

- External use requires a verified packed/released build exposing these entries. If installed exports lack them, retain supported ordinary factory options or report the artifact gap; do not recreate the config API.
- Startup resolves all installed configured instances before any business setup. Sources compose in declaration order: later present top-level fields replace earlier ones; nested objects/arrays are replaced whole, not deep-merged. Validate once after composition.
- Config is copied/frozen finite plain data, not resource handles. Bind env keys explicitly and mark sensitive fields/bindings explicitly. File sources require an absolute root and relative in-root path; keep filesystem adapters out of Workers.
- Source failures fail closed. Custom preflight readers may do I/O and must release temporary resources in `finally`. Inspection describes declarations without reading sources; trusted top-level code and converters still execute.
- Config schema discovery needs its explicit converter. Keep values, secrets and revision tokens out of metadata. This is startup configuration, not a config center, subscription system or hot reload.
