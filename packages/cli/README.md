# TypeScript operation API and internal adapter

## Guidance for the installed adapter

From the consuming application's directory, locate this installed Markdown file:

```sh
bun -e 'console.log(import.meta.resolve("@lenso/cli/guide"))'
```

Read the returned file; do not import Markdown as executable code. Its adjacent
`package.json` records the installed adapter version and exports. This guide and
the included `examples/` belong to that artifact, while repository `main` may
describe newer APIs. Equal version strings alone do not identify a local source
candidate: keep its archive hash/source receipt when using one. This new guide
export is a source change, not a claim that registry `@lenso/cli@0.21.0` contains it.
If an older installation has no guide export, inspect its package README and public
declarations without upgrading or assuming missing features.

Other Lenso packages and the native command have independent versions. Verify the
app's installed exports and `lenso help --json` before using their capabilities.
This package provides no Runtime requirement for ordinary local services.

### Minimal Operation recipe

With this adapter, compatible installed Core/Engine, and the application's own
Standard Schema implementation (here Zod), put this in `lenso.config.ts`:

```ts
import { defineApp, definePlugin } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { z } from "zod";

const input = z.object({ name: z.string() });
const output = z.object({ message: z.string() });
const greeting = definePlugin({
  id: "greeting",
  setup() {
    return { greet: ({ name }: { name: string }) => ({ message: `Hello, ${name}!` }) };
  },
});
export default defineApp({ plugins: [greeting] });
export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    description: "Greet one name.",
    input,
    output,
  }),
];
```

Run `lenso inspect greeting greet --root . --json`, then supply `{"name":"Ada"}`
on stdin to `lenso call greeting greet --root . --stdin --json`. Input and output
use the declared schemas; adapt the same service/schema for another entry rather
than copying business logic. A finite call owns app startup/cleanup. Declaration
does not grant permission; authenticated services still enforce their policy.

The packaged `examples/first-run/README.md` describes a separate opt-in setup
recipe and its exact Core/native candidate prerequisites. It is not needed for
this Operation. Notes/private-files/Tasks recipes live in the repository, are not
included here, and require their own provider/dependency verification; a source
recipe's presence does not prove compatibility with installed packages.

## Command and development lifecycle

The only installed command is the Rust `lenso` executable from `LioRael/lenso-cli`.
This package provides `defineOperation`, inspection/invocation APIs and the explicit
`@lenso/cli/adapter` module used through Bun. It has no `bin`, executable entry or
compatibility alias. Importing the adapter does not run a command or modify console.

Install this library in the selected TS project and build framework artifacts before
source-checkout consumers. `lenso inspect/call/build/dev/check/generate --root DIR`
loads that project's installed module directly; Rust never invokes a second CLI.
Keep Bun available. No Rust compiler is needed for ordinary TS application work.

`lenso dev --root <project>` watches the entry and restarts a fresh Bun process.
The human startup view uses stderr. TTY output has modest colors; `NO_COLOR`,
CI, non-TTY and `TERM=dumb` output stays plain.

Readiness is explicit. In the development entry, after application startup and
listener binding succeed, report actual runtime information with the public helper:

```ts
import { reportDevReady } from "@lenso/engine/dev-ready";

reportDevReady({
  urls: [server.url.href],
  capabilities: ["web"],
});
```

The repository greeting entry already calls this helper. It is a no-op outside supervised dev; authors never need to handwrite IPC messages. Custom entries call it after successful startup. Service-only entries can omit `urls`. Use enabled capability names, without
configuration values or secrets. Entries without this signal remain Starting;
spawning a process alone does not prove readiness. Failed starts keep watching
for source changes. The displayed URL is the reported listener origin, with no
credentials, query, path or guessed network interfaces.

`createDevPresentation` in `src/dev-presentation.ts` accepts a project root and
optional `mode: "json"`. JSON mode emits no presentation output; the CLI's
protocol layer owns its results and diagnostics. The module does not wrap
console or intercept application logs.

## Engine plugins

Discovery, generation, targets and dev scheduling live in
the installed `@lenso/engine` package (see its README), with a typed programmatic API and the public
`@lenso/engine/authoring` plugin protocol. No extra configuration is needed for the
default Bun build. Extensions still use `lenso.engine.ts` beside `lenso.config.ts`.

The CLI maps structured Engine diagnostics to its existing JSON envelopes and exit codes.

`inspect` and `call` continue to import only canonical `lenso.config.ts`, never
Engine config/setup/hooks. Trusted config top-level side effects are still
possible; this is not a sandbox. Runtime plugin setup runs only for invocation,
with the existing explicit allowlist and shared schema.
