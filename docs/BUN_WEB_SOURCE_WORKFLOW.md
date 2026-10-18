# Bun Web framework-maintainer source workflow

Use the existing `templates/bun-web` when one Bun application needs a service,
CLI and Web. This path installs real archives outside the framework workspace;
it does not depend on a registry release or source aliases. Requirements: Bun
1.4.3, Node for the TypeScript executable, Git and a POSIX shell.

This optional path validates unpublished framework changes. Ordinary application
consumers should use the [registry quickstart](BUN_WEB_QUICKSTART.md), which needs
no Runtime preparation or framework build. Install the official native CLI
separately; the npm adapter has no executable.

## Prepare the framework and copy the application

From a fresh checkout of the reviewed Lenso revision:

```sh
cd lenso
FRAMEWORK="$PWD"
bun run prepare:runtime
bun install --frozen-lockfile
bun node_modules/turbo/bin/turbo run build --filter=@lenso/cli --filter=@lenso/web

# Choose a new directory outside this checkout; mkdir refuses an existing app.
APP="$(dirname "$PWD")/hello-lenso"
mkdir "$APP"
cp -R templates/bun-web/. "$APP/"
# The template defaults to registry releases. Switch this private source-verification
# copy to the four archives below; never commit these overrides as a consumer pin.
bun -e 'const p = await Bun.file(process.argv[1]).json(); for (const [name, file] of Object.entries({core:"core", engine:"engine", cli:"cli", web:"web"})) p.dependencies[`@lenso/${name}`] = `file:./vendor/lenso-${file}.tgz`; p.overrides = {"@lenso/core":"file:./vendor/lenso-core.tgz", "@lenso/engine":"file:./vendor/lenso-engine.tgz"}; await Bun.write(process.argv[1], JSON.stringify(p, null, 2) + "\n");' "$APP/package.json"
rm "$APP/bun.lock"
mkdir "$APP/vendor"
bun pm pack --cwd packages/lenso --filename "$APP/vendor/lenso-core.tgz"
bun pm pack --cwd packages/engine --filename "$APP/vendor/lenso-engine.tgz"
bun pm pack --cwd packages/cli --filename "$APP/vendor/lenso-cli.tgz"
bun pm pack --cwd packages/web --filename "$APP/vendor/lenso-web.tgz"

cd "$APP"
bun install
bun run typecheck
bun run test
bun run inspect
printf '%s\n' '{"name":"Ada"}' | bun run call
bun run build
```

SDK preparation is a requirement of the **framework source workspace**, which
includes Runtime packages. It downloads the checksum-pinned snapshot and builds
only its TS SDK, with no Cargo or sibling checkout. The copied greeting installs
only Core, Engine, CLI and Web plus their declared dependencies. Keep its archives
from one build. Commit the application's newly created `bun.lock` for subsequent
`bun install --frozen-lockfile`; no framework lockfile changes are required.

`inspect` reports a static greeting declaration without starting a listener.
`call` returns `{"data":{"count":1,"message":"Hello, Ada!"},"ok":true,"schemaVersion":1}`.
The config and Web router select the exact same greeting plugin and input schema;
each CLI call owns a fresh service, while HTTP shares one running counter.

## Run Engine dev and the built application

```sh
LENSO_PORT=3000 bun run dev
```

Wait for Engine's `Ready` message and actual URL. From another terminal:

```sh
curl --fail-with-body -H 'content-type: application/json' \
  --data '{"json":{"name":"Ada"}}' http://127.0.0.1:3000/rpc/greet
```

Repeat the request to see count 2. Edit the greeting's business implementation
in `src/greeting.ts`: Engine restarts the app and the in-memory count resets.
Ctrl-C closes the owned listener. To verify the built entry separately:

```sh
LENSO_PORT=3000 bun dist/server.js
```

The server also accepts `LENSO_PORT=0`, which allocates an available port and
prints its actual URL; use that URL for requests. `GREETING_PREFIX=Hi` overrides
the schema's greeting default. Importing `src/server.ts` defines plugins but does
not listen; tests can call `startServer(0)` and must await `app.stop()`.

## Diagnose the boundary that failed

| Symptom                                             | Meaning and next action                                                                                                                                                                                              |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Missing `.runtime-deps/sdk` on framework install    | Run `bun run prepare:runtime` before the frozen install. Do not create a placeholder SDK or edit `bun.lock`.                                                                                                         |
| Preparation download/checksum/build failure         | Preserve the pin and previous prepared output; fix network/toolchain or use the explicitly authorized local-source mode documented in the root README.                                                               |
| Missing `dist` export                               | Build the selected framework packages before packing; repack all four archives and reinstall in the consumer.                                                                                                        |
| `LENSO_PORT=bad`, empty, fractional or out of range | Startup exits 1 with `code: config-invalid`, `phase: config`, cause `pluginId: http` and `details.path: [port]`. Malformed numeric env has source ID `listener-env`. No plugin setup or listener acquisition occurs. |
| Listener setup fails after valid port config        | Check whether the chosen loopback port is occupied; choose `LENSO_PORT=0` or another owned port. Unknown setup details are intentionally omitted from the safe diagnostic.                                           |
| CLI input has a one-character name                  | Input schema rejects it before setup (exit 2). Use the shared schema reported by `inspect`.                                                                                                                          |
| Dev says failed and keeps watching                  | Engine remains available for edits; Ctrl-C ends the supervisor. A failed startup never reports Ready.                                                                                                                |

The listener's port uses Core `bindConfig` plus the explicit numeric `envSource`.
Core validates all installed config before setup. The entry prints the existing
Engine `diagnostic(error)` projection, including safe instance/field attribution,
instead of an unhandled Bun stack containing raw rejected values.

## Enable an existing Engine plugin

The shipped content Engine extension reads Markdown and generates an owned data
module. It uses the public authoring API and adds no runtime service or listener.
In the application terminal (with `FRAMEWORK` and `APP` from above):

```sh
bun pm pack --cwd "$FRAMEWORK/packages/cli/examples/content-plugin" \
  --filename "$APP/vendor/lenso-example-content.tgz"
bun add ./vendor/lenso-example-content.tgz
mkdir content
printf '%s\n' '# Welcome' > content/welcome.md
cat > lenso.engine.ts <<'TS'
import { defineEngineConfig } from "@lenso/engine/authoring";
import { contentPlugin } from "lenso-example-content-engine";

export default defineEngineConfig({ plugins: [contentPlugin()] });
TS
lenso generate --json
cat .lenso/content.ts
bun run build
```

The output contains `welcome` and the Markdown body. Engine defaults still generate
the manifest/client and build the same server. During dev, editing Markdown
invalidates generation through the plugin's explicit `context.watch("content")`.
Do not hand-edit `.lenso/content.ts`. `inspect` and `call` continue to work without
loading Engine plugins. The two shipped extension packages declare a peer range
matching this revision's Engine; keep extension and framework artifacts together.

## Optional runtime plugins and Console

The greeting is a local unauthenticated example. Adding a package does not
install a plugin: create its public plugin instance, declare exact `requires`
references, and include the same objects in the app's explicit plugin list.
Web's listener belongs to `startServer`, so a finite CLI call does not bind a port.

Console needs an authenticated host entry; a running Engine or a CLI `operations`
export is insufficient. Reuse [Auth](../packages/auth/README.md) for verified
current request evidence, select existing contextual service operations with
[Manage](../packages/manage/README.md#running-entries), and mount
`createManageRouter` through the application's owned Web route. Its per-request
`evidence`, `canList` and `binding` callbacks use current Auth; business services
retain their authorization. Do not pass an actor in greeting JSON or add an
always-allow policy to simulate Console access.

The runnable Console host and its frontend belong to
[`lenso-console/examples/ts-console`](https://github.com/LioRael/lenso-console/tree/main/examples/ts-console).
Use that repository's version-matched host instructions for actual Console
attachment; this path verifies the standalone TS application, not Console UI or
cross-repository dependency compatibility.
