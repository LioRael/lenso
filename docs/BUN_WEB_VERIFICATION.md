# Bun Web onboarding verification — 2026-10-15

Base: `c6d2820` (`origin/main`). Scope: the existing Bun Web template, shipped
Engine extension examples and their documentation. No Relay, Console, Rust,
root dependency lockfile, production credentials or deployed resources changed.

## Observed breaks and fixes

- The current root README already documents SDK preparation. The templates
  installation section still started with `bun install`, which fails on a clean
  workspace because `.runtime-deps/sdk` is absent. Its instructions now match
  the existing pinned preparation flow; no replacement bootstrap was added.
- The Bun Web entry converted `LENSO_PORT` with `Number()`. A malformed value
  reached `Bun.serve` as NaN and printed an unhandled `RangeError` stack. The
  listener now uses existing Core config/env validation before any plugin setup,
  and the entry prints Engine's existing safe diagnostic projection. Empty,
  non-finite, fractional, negative and above-65535 ports are covered.
- Both shipped CLI Engine extension example manifests declared `^0.1.0`, which
  excludes this revision's Engine `0.6.0`. They now select `^0.6.0`; the existing
  packed-consumer integration test checks that the installed Engine satisfies
  each extension's declared peer range.

## Real external consumer evidence

Executed the [quickstart](BUN_WEB_QUICKSTART.md) in a fresh directory outside the
repository, with no pre-existing `node_modules`, copied workspace links or source
aliases. Core, Engine, CLI, Web and the content extension were installed from
real `bun pm pack` archives. Public dependencies were resolved normally.

Environment: macOS arm64, Bun 1.4.3, Node 26.10.0. These are one-run elapsed
measurements with the machine's existing package cache, not cold-network or
production performance claims. Framework prerequisite build succeeded in
880 ms (7 tasks, no Turbo cache hits); archive packing took 96 ms in total.

| External app check                              |   Observed elapsed |
| ----------------------------------------------- | -----------------: |
| Initial `bun install` (34 packages)             |            1168 ms |
| `bun install --frozen-lockfile`                 |              17 ms |
| TypeScript check                                |             198 ms |
| 8 focused tests, 54 assertions, no failures     |             281 ms |
| Static greeting inspection                      |              52 ms |
| Valid CLI greeting, count 1                     |              53 ms |
| Invalid name, JSON error and exit 2             |              61 ms |
| Engine build                                    |             148 ms |
| Built server ready on OS-assigned loopback port |              43 ms |
| Engine dev ready (including IPC readiness)      |             209 ms |
| Business source edit → restarted app ready      |             291 ms |
| Add packed content extension                    |              14 ms |
| Plugin generation / build / typecheck           | 113 / 147 / 111 ms |
| Engine dev with content extension               |             210 ms |

Both built and dev listeners served real `/rpc/greet` requests with counts 1
then 2. The dev source edit changed the response and reset the service count to
1, proving a fresh application instance. SIGTERM closed each owned server and
supervisor with exit 0; each last listener port could immediately be rebound.
Content generation produced `.lenso/content.ts` containing the supplied Markdown.
The template tests spawn the actual entry for invalid env cases and parse its
safe config diagnostic; they also exercise Web calls and listener release.

The existing repository integration command
`bun test packages/cli/test/packaging.test.ts` now also copies this exact template,
installs the four packed artifacts using its own manifest, and runs its typecheck,
8 tests, build and inspection. The final run passed (1 integration test,
56 outer assertions, 2.74 seconds, plus the nested template assertions). CLI
typecheck, scoped oxlint/oxfmt, changed-document link/fence validation and
`git diff --check` also passed.

The copied runnable app used for this pass was
`<temporary-consumer-directory>`; temporary paths are local evidence,
not a dependency of the committed example. The quickstart contains all commands
to create another copy. The original source/review workflow needs no global links
and changes no framework lockfile.

## Console boundary

No Console UI or Relay/MCP upgrade was executed here. The quickstart links the
existing Console host and identifies the required verified Auth evidence,
selected Manage operations and explicit route/binding seam. The public greeting
does not invent identity or provide an always-allow management route. Actual
Console attachment remains owned by the parallel Console/Relay task; these
results do not assert cross-repository dependency or frontend compatibility.

## Published-package consumer follow-up (2026-10-18)

The earlier source-pack evidence above remains historical. `templates/bun-web`
now pins npm Core 0.3.1, Engine 0.7.0, CLI 0.21.0 and Web 0.3.2 and includes its
own lockfile. A new `/tmp` consumer with an empty package cache, controlled PATH,
and official native v0.2.0 completed frozen installation, typecheck, the focused
listener/config checks, inspect, CLI/HTTP calls, one business edit, dev reload,
stop/restart and build without a framework checkout, vendor or Runtime SDK.
Measured acceptance sequence: 30.118 s, including 28.282 s installation; zero
application configuration edits, one business edit, no help requests. This is one
macOS arm64 observation, not a cross-platform or speedup claim.

One requested diagnostic criterion remains release-blocked: missing adapter writes
a useful installation hint to stderr, but released native v0.2.0 returns JSON
`invalid-output` (exit 3). CLI main `7964c1c8b80e8d75b9bb6ce1fc62d9eae9109b40`
already distinguishes `typescript-adapter-missing` in its embedded bootstrap;
release v0.2.0 is from commit `5468c60f1c359a249ebed501dabb30c037bd7b44`.
No replacement binary or unpublished API was used for consumer acceptance.
A later native release is needed for the precise structured diagnostic criterion;
ordinary installed CLI/HTTP use works with the existing release.
