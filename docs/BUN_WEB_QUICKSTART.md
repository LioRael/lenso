# Bun Web with published packages

Use `templates/bun-web` for one ordinary async service exposed through HTTP and
native CLI. Copy just that directory (including `bun.lock`) from a reviewed source
archive into a new application directory. No framework checkout, sibling repo,
Runtime SDK, framework build, pack step, vendor archive or override is required.

## Install

Requirements: Bun **1.4.3**, Node for `tsc`, and the official
[native Lenso CLI v0.2.0](https://github.com/LioRael/lenso-cli/releases/tag/v0.2.0).
Download the release asset for your platform, verify it against the release's
`SHA256SUMS`, and place the executable named `lenso` in a directory on `PATH`.
For example, on macOS arm64 (use a new directory you own):

```sh
mkdir lenso-tools
curl --fail --location --output lenso-tools/lenso \
  https://github.com/LioRael/lenso-cli/releases/download/v0.2.0/lenso-v0.2.0-aarch64-apple-darwin
printf '%s\n' '0d6addbfc2d213054becda79fba0267f7104064f4829c85995c04388d1af3cce  lenso-tools/lenso' | shasum -a 256 -c -
chmod +x lenso-tools/lenso
export PATH="$PWD/lenso-tools:$PATH"
```

In the copied application:

```sh
bun install --frozen-lockfile --registry https://registry.npmjs.org
bun run typecheck
lenso help --json
bun run inspect
printf '%s\n' '{"name":"Ada"}' | bun run call
```

The manifest fixes `@lenso/core@0.3.1`, `@lenso/engine@0.7.0`,
`@lenso/cli@0.21.0` and `@lenso/web@0.3.2`, all from the official npm registry.
`bun.lock` fixes the dependency closure and integrity hashes. Web uses the pinned
prerelease `@orpc/server@2.0.0-beta.42`; do not replace it with registry `latest`.
The npm `@lenso/cli` package supplies operations and `./adapter`, **no bin**;
`bunx @lenso/cli` is not the native CLI installation step.

`inspect` imports trusted config and reports the shared input schema without
starting the app. `call` validates input, starts a fresh app, invokes the service
and awaits cleanup. The initial result's data is
`{"message":"Hello, Ada!","count":1}`.

## Run, edit and restart

```sh
LENSO_PORT=3000 bun run dev
```

The listener binds only `127.0.0.1`. Wait for its actual URL, then in another
terminal:

```sh
curl --fail-with-body -H 'content-type: application/json' \
  --data '{"json":{"name":"Ada"}}' http://127.0.0.1:3000/rpc/greet
```

Change the single business expression in `src/greeting.ts`, for example
`` `${prefix}, ${name.toUpperCase()}!` ``. Engine restarts the HTTP app after the
edit. Call both entries again: both now return `Hello, ADA!`. No router or CLI
handler change is needed; both use `greetingInput` and the same ordinary service.
HTTP retains its in-memory counter; each finite CLI call starts at 1. Stop with
Ctrl-C and run `bun run dev` again: the business edit remains, the counter resets.
This sample has no durable database or resume journal.

`LENSO_PORT=0` chooses an available port and prints its URL. No `.env` edit is
required. `GREETING_PREFIX=Hi` optionally overrides the greeting default.

For a built application:

```sh
bun run build
LENSO_PORT=3000 bun dist/server.js
```

Stop owned processes before removing the copied application, `.lenso`, `dist` or
`node_modules`. Dependencies can be restored with the same frozen install.

## Diagnose the actual boundary

- Missing native executable: install the official v0.2.0 asset on `PATH`.
- Missing project adapter: published native v0.2.0 writes an installation
  suggestion to stderr but its JSON error is `invalid-output` (exit 3). This is a
  release diagnostic limitation, not malformed business output. Restore this
  application's fixed `@lenso/cli` using `bun install --frozen-lockfile`.
  CLI main has `typescript-adapter-missing` in `error.adapterError.code`, but
  that precise diagnostic is not in the v0.2.0 release.
  Installing a global npm CLI or cloning the framework does not repair the app.
- Invalid name: inspect the shared schema; a one-character name fails before setup.
- Invalid `LENSO_PORT`: startup reports `config-invalid`, phase `config`, and
  attributes the port to `http`. Choose an integer from 0 through 65535.
- Occupied port: use an owned free port or `LENSO_PORT=0`.

For coding-agent work, use the repository's canonical
[`lenso-develop`](../.agents/skills/lenso-develop/SKILL.md) and
[`lenso-diagnose`](../.agents/skills/lenso-diagnose/SKILL.md) skills. Links follow the
reviewed source revision; installed exports and lockfile remain authoritative.
No additional prompt system is shipped in the template.

Auth, Manage, MCP, Console, Runtime and Agent are optional separate integrations.
The registry `@lenso/agent@1.20.0` is the old launcher, not the new embedded Agent
API; this ordinary TS path does not install it. The greeting has no authentication
and is a local sample. Add real entry identity and shared business policy before
using it as a remote service.

Framework maintainers retain the separate
[source pack workflow](BUN_WEB_SOURCE_WORKFLOW.md). Its Runtime preparation is a
source-workspace requirement, not an ordinary application dependency.
