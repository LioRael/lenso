# Development output

`lenso dev --root <project>` watches the entry and restarts a fresh Bun process.
The human startup view uses stderr. TTY output has modest colors; `NO_COLOR`,
CI, non-TTY and `TERM=dumb` output stays plain.

Readiness is explicit. In the development entry, after application startup and
listener binding succeed, report actual runtime information with the public helper:

```ts
import { reportDevReady } from "lenso-cli/dev";

reportDevReady({
  urls: [server.url.href],
  capabilities: ["web"],
});
```

The included greeting entry already calls this helper. It is a no-op outside supervised dev; authors never need to handwrite IPC messages. Custom entries call it after successful startup. Service-only entries can omit `urls`. Use enabled capability names, without
configuration values or secrets. Entries without this signal remain Starting;
spawning a process alone does not prove readiness. Failed starts keep watching
for source changes. The displayed URL is the reported listener origin, with no
credentials, query, path or guessed network interfaces.

`createDevPresentation` in `src/dev-presentation.ts` accepts a project root and
optional `mode: "json"`. JSON mode emits no presentation output; the CLI's
protocol layer owns its results and diagnostics. The module does not wrap
console or intercept application logs.

## Engine plugins

No extra configuration is needed for the default Bun build. To extend the build host,
add an optional **lenso.engine.ts** beside lenso.config.ts. Keep Engine imports out of
runtime assembly and browser entries. The `lenso-cli/engine` entry contains only authoring
helpers and types; it imports neither the CLI host nor the runtime SDK.

```ts
import { defineEngineConfig, defineEnginePlugin } from "lenso-cli/engine";

const assets = defineEnginePlugin({
  name: "app/assets",
  source: { file: "lenso.engine.ts", export: "assets" },
  setup(context) {
    context.watch("assets"); // Directory additions/removals also invalidate generation.
    context.discover("assets", async () => ["assets/message.txt"]);
    context.generate("asset-index", ({ sources }) => [
      {
        path: "assets.ts", // Relative to .lenso, never an arbitrary filesystem path.
        content: `export const paths = ${JSON.stringify(sources)};`,
      },
    ]);
  },
});
export default defineEngineConfig({ plugins: [assets] });
```

Setup registers `convention`, `discover`, `generate`, `target`, `dev`, `watch` and
`onCleanup`. Snapshots contain immutable paths and source lists. Discovery hooks append
existing application sources; generators return static files. Sources live inside the
application root, outside .lenso/dist. A convention returns `{config, entry?, router?}`;
router is optional, so Web/oRPC is never required. A target receives `entry` and `bundle`
for the same Bun build implementation used by the default target. Select a registered
target with `defineEngineConfig({target: "workers", plugins})`.

Plugins have unique names. `before`/`after` specify named ordering constraints; unknown
names and cycles fail. Otherwise configuration order is stable. Capabilities have unique
names within their stage. Replace an existing registration with the exact owner, for
example `context.convention(() => ({config:"app.ts", entry:"src/index.ts"}),
{replace:"lenso/defaults"})`. The official owner **lenso/defaults** registers the app
convention, generators **manifest**, **server**, **client**, and target **bun** through
this same API. Replacement retains the capability's stage position. To replace another
custom plugin, declare `after` that plugin and use its name as `replace`. Merely registering
a conflicting name or output never silently overwrites it.

The packaged [content plugin](examples/content-plugin/index.ts) and
[module target plugin](examples/module-target-plugin/index.ts) are standalone application
packages. Install them as dev dependencies (`bun add --dev ./path/to/plugin`) and import
`contentPlugin`/`moduleTarget` in lenso.engine.ts. Content discovery produces .lenso/content.ts.
`moduleTarget({name:"workers", entry:"src/index.ts"})` bundles a browser-compatible module
inside dist/workers using the shared bundler. A Workers entry can use the existing
@lenso/workers adapter; this build plugin adds no deployment or credential workflow.

Generated ownership is recorded in .lenso/.engine-files.json. Duplicate output owners,
path traversal, symlink output paths and edited generated content fail before writing.
Unchanged bytes retain their mtime. Removing a generator removes only its tracked,
unmodified files. The three known outputs from the previous Engine are migrated once.
Generation validates its complete output set first, but filesystem writes are not transactional.
Build plugins remain trusted local code and can themselves access the filesystem/process;
the host's path checks are not a sandbox.

Register resource cleanup immediately during setup. Hooks execute sequentially; cleanup
executes in reverse registration order on success, failure and dev shutdown, preserving
both execution and cleanup diagnostics. Capability registration closes when setup returns; captured `context.watch`/`context.onCleanup` remain available to hooks until cleanup begins. The dev
host helper `startEngineDevCycle(root)` creates a fresh build worker, generates files and
runs `dev("beforeStart", snapshot)`. Only application readiness triggers `cycle.ready()`
and `dev("ready", snapshot)`. `cycle.close()` is idempotent and waits for worker cleanup;
shutdown exceeding 5 seconds terminates the owned worker and reports failure. Startup is
bounded to 30 seconds. The supervisor must close a cycle after stopping its runtime child.

Static imports from Engine/config/discovered sources are watched; register dynamic reads
with `watch(path)`. Each dev cycle loads a fresh module graph. Finite in-process calls use
normal Bun module caching; use a fresh CLI process after edits. No generation result cache
is assumed. Generated and dist inputs are excluded from watches to avoid restart loops.

`inspect` and `call` use lenso.config.ts directly and never import Engine plugins or run
Engine setup/hooks. A dynamically chosen build convention cannot be inferred by those
commands: retain a canonical lenso.config.ts re-export for CLI operations. Runtime plugin
setup runs only during invocation, with the existing explicit allowlist and shared schema.
Engine errors use the existing CLI JSON diagnostics with plugin name and supplied source
(or the exact engine config path); no line numbers are guessed.
