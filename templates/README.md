# Minimal applications

Copy one directory (`cli`, `bun-web`, or `workers`) into a new directory outside this workspace. These are template contents, not a scaffold command. Use Bun 1.4.2. They consume packed packages; package names here do not imply that an npm release exists.

Build the framework first (`bun install`, `bun run build`). Create the application's `vendor` directory, then in each needed package directory run `bun pm pack --filename /absolute/path/to/application/vendor/<file>`:

| Package directory  | Filename            | Templates        |
| ------------------ | ------------------- | ---------------- |
| `packages/lenso`   | `lenso.tgz`         | all              |
| `packages/engine`  | `lenso-engine.tgz`  | CLI, Bun Web     |
| `packages/cli`     | `lenso-cli.tgz`     | CLI, Bun Web     |
| `packages/web`     | `lenso-web.tgz`     | Bun Web, Workers |
| `packages/workers` | `lenso-workers.tgz` | Workers          |

The templates declare `file:./vendor/*.tgz` dependencies. Create `vendor` and put the real archives there before `bun install`. Root overrides resolve transitive `lenso` and (for CLI/Bun Web) `@lenso/engine` dependencies to local archives, avoiding registry lookups for unpublished packages. Workers do not install Engine or CLI. All packages come from the same build. You may replace these explicit paths and overrides with other verified package locations; do not point at framework source or copy the framework into the application. Each consumer uses its own single `bun.lock`.

- CLI: `bun run call -- '{"name":"Ada"}'`, `bun run generate`, `bun run build`.
- Bun Web: optionally copy `.env.example` to `.env`, then `bun run dev`. Call `/rpc/greet` with an oRPC client, or `curl -H 'content-type: application/json' --data '{"json":{"name":"Ada"}}' http://127.0.0.1:3000/rpc/greet`. `bun run build` emits the Bun server.
- Workers: `bun run types`, `bun run typecheck`, then `bun run dev`. The same HTTP example uses port 8787. `bun run build` performs a local Wrangler dry-run bundle. No deploy script, cloud resources, or login is required. Optional `.dev.vars.example` contains a non-secret local binding override.

The Workers greeting has per-request memory only (count resets to 1). For durable Notes storage use the D1 example, not this counter. `src/index.ts` is the Worker entry; never use the Bun CLI build/dev engine for a Worker bundle. Generated Worker types derive from `wrangler.jsonc`.

Keep the Workers template's `enable_request_signal` flag: it enables client-disconnect propagation and app cleanup through the adapter. Platform termination can still interrupt finalizers.

CLI operations are explicit declarations of the existing service and shared input schema. Bun Web uses that same schema for its service, Web procedure, and CLI operation. Its custom server reports readiness with the public `lenso-cli/dev` helper after listening; use framework archives that include that export.

Future scaffold integration needs a template ID, target directory, application name, and an explicit dependency map for `lenso`, `@lenso/engine`, `lenso-cli`, `@lenso/web`, and `@lenso/workers` as applicable. Copy only the selected directory, replace its package name and dependency locations, then install with Bun. Template metadata or this README must not be emitted as application runtime code.
