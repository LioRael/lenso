# Bun Web consumer

This ordinary application uses fixed published dependencies and the included
`bun.lock`. Requirements: Bun 1.4.3, Node for `tsc`, and the official
[native CLI v0.2.0](https://github.com/LioRael/lenso-cli/releases/tag/v0.2.0)
on `PATH` as `lenso`. Verify the downloaded asset with the release's SHA256SUMS.
The npm `@lenso/cli@0.21.0` is a project adapter with no executable.

```sh
bun install --frozen-lockfile --registry https://registry.npmjs.org
bun run typecheck
bun run inspect
printf '%s\n' '{"name":"Ada"}' | bun run call
LENSO_PORT=3000 bun run dev
```

In another terminal:

```sh
curl --fail-with-body -H 'content-type: application/json' \
  --data '{"json":{"name":"Ada"}}' http://127.0.0.1:3000/rpc/greet
```

Edit only `src/greeting.ts`: change `${name}` in the returned message to
`${name.toUpperCase()}`. Dev restarts the HTTP app. CLI and HTTP then both return
`Hello, ADA!`; they share the service and `greetingInput` schema. HTTP retains an
in-memory counter; each CLI call and server restart starts it at 1. Ctrl-C closes
the listener; `bun run dev` starts it again with your saved business edit.
`LENSO_PORT=0` chooses a free loopback port and prints its URL.

No `.env` edit, framework checkout/build, Runtime, Agent, vendor directory or
package override is needed. Sources are the official npm registry and native
GitHub release. For a build: `bun run build`, then `bun dist/server.js`.

If the adapter is missing, released native v0.2.0 prints an installation suggestion
to stderr but wraps it as JSON `invalid-output` (exit 3). Restore local dependencies
with `bun install --frozen-lockfile`. CLI main has the precise
`typescript-adapter-missing` diagnostic; it is not yet in that release. Do not
install a global npm CLI to repair the project.

The sample is unauthenticated and binds `127.0.0.1`. It has no durable state.
See the canonical [quickstart](https://github.com/LioRael/lenso/blob/main/docs/BUN_WEB_QUICKSTART.md)
for native installation/checksum instructions and the separate source-maintainer
pack workflow. Coding agents should reuse
[lenso-develop](https://github.com/LioRael/lenso/blob/main/.agents/skills/lenso-develop/SKILL.md)
and [lenso-diagnose](https://github.com/LioRael/lenso/blob/main/.agents/skills/lenso-diagnose/SKILL.md),
checking installed exports against the lockfile.
