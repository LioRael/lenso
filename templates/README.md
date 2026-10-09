# Application starting points

Copy a template directory outside this repository. These are application contents,
not a scaffold generator. Use Bun 1.4.2 and packed packages from the same framework
build; the package names do not imply an npm release exists.

## Choose a size

- `cli`: one-file service, plugin binding and explicit CLI operation. No Web.
- `bun-web`: a small application with an ordinary async greeting service, startup
  configuration binding, router-first Web types and CLI exposure of the same service.
- `workspace`: `apps/server`, `apps/web` and one actual shared capability,
  `plugins/greeting`. Add packages when code is shared, not one package per business
  service. The Web app is a browser client module, not a second Lenso runtime or UI.
- `workers`: the existing Worker entry and local Wrangler workflow.

For the smallest ordinary-service use, import `greet` from `cli/lenso.config.ts`,
or use the Bun Web service without starting an app:

```ts
import { createGreetingService } from "./src/greeting";

const service = createGreetingService("Hello");
console.log(await service.greet({ name: "Ada" }));
```

`src/server.ts` remains the Bun Web entry and retains its greeting/schema exports.
Importing it does not listen; only `startServer()` or execution as the main entry
starts a listener. Business calls do not read environment variables. The app
explicitly supplies `envSource` to the thin plugin; configuration resolves once
before setup. Factories can instead take sources backed by another environment,
or no sources for schema defaults.

## Install real artifacts

Build the framework first (`bun install`, `bun run build`). Create the copied
application's `vendor` directory. From each required framework package directory,
run `bun pm pack --filename /absolute/path/to/application/vendor/<filename>`:

| Package directory  | Filename            | Templates                   |
| ------------------ | ------------------- | --------------------------- |
| `packages/lenso`   | `lenso-core.tgz`    | all                         |
| `packages/engine`  | `lenso-engine.tgz`  | CLI, Bun Web, workspace     |
| `packages/cli`     | `lenso-cli.tgz`     | CLI, Bun Web, workspace     |
| `packages/web`     | `lenso-web.tgz`     | Bun Web, workspace, Workers |
| `packages/workers` | `lenso-workers.tgz` | Workers                     |

Then run `bun install` in the copied application root. Manifests use reviewable
`file:.../vendor/*.tgz` dependencies; root overrides keep transitive core/Engine
dependencies on the same archives. Workers installs neither Engine nor CLI.
Do not point consumers at framework source or invent registry versions.
Each copied application has one root `bun.lock`, including the workspace.

## Run

- CLI: `bun run call -- '{"name":"Ada"}'`, `bun run generate`, `bun run build`.
- Bun Web: optionally copy `.env.example` to `.env`; `bun run typecheck`,
  `bun run dev`, `bun run build`.
- Workspace: run `bun run typecheck`, `bun run call -- '{"name":"Ada"}'`,
  `bun run dev`, `bun run build` at its root. Server scripts select
  `--app apps/server` explicitly; browser build is independent.
- Workers: `bun run types`, `bun run typecheck`, `bun run dev`, `bun run build`.
  Build is a local Wrangler dry-run, not deployment. Optional `.dev.vars.example`
  contains a non-secret local binding override.

The Bun Web and workspace server bind loopback, with no authentication; they are
local development examples, not public ingress policies. Call either with:

```sh
curl -H 'content-type: application/json' \
  --data '{"json":{"name":"Ada"}}' http://127.0.0.1:3000/rpc/greet
```

For a typed client, import `AppRouter` and `AppClient` using **`import type`**.
Bun Web exports them from `src/router.ts` (and the old server path):

```ts
import type { AppClient, AppRouter } from "./src/router";
import { createClient } from "@lenso/web/client";

const client: AppClient = createClient<AppRouter>("http://127.0.0.1:3000/rpc");
console.log(await client.greet({ name: "Ada" }));
```

Workspace Web imports the server's public `@app/server/client` type-only export.
There is deliberately no runtime export for that path. Its browser bundle imports
only `@lenso/web/client`, not configuration, Engine or server initialization.
The shared greeting dependency is explicitly `workspace:*`; package identity,
not `plugins/`, determines composition. Move it anywhere by updating workspace
globs, relative vendor dependency references and TypeScript includes as needed,
without changing service assembly.

`apps/server/lenso.engine.ts` replaces only the default convention. Default
generators and the Bun target remain installed. Its `runtime/main.ts` entry is
relative to the selected application root, not the workspace root. Dev uses
that convention entry; an explicit `--entry` overrides it with the same
root-relative rule. Custom entries report readiness through
`@lenso/engine/dev-ready` after listening.

CLI operations are explicit selections of the existing service and shared schema.
HTTP retains its counter for the running app; each CLI call starts a fresh app.
The Workers greeting uses per-request memory (count resets to 1); durable storage
belongs in a real storage service. Keep its `enable_request_signal` flag for
disconnect propagation and cleanup, though platform termination can interrupt
finalizers. Worker types derive from `wrangler.jsonc`; never use Bun's Engine
build/dev for its entry.
