# Local Workers applications

From the repository root run `bun install` and `bun run build`. In this directory:

- `bun run dev` serves the in-memory greeting. `GREETING_PREFIX` comes from the binding declared in `wrangler.jsonc`; optionally copy `.dev.vars.example` to `.dev.vars`. Each request gets a fresh app, so count resets to 1.
- `bun run migrate:notes` explicitly applies `examples/notes/migrations/sqlite` to Wrangler's local D1, using its `d1_migrations` table. Then `bun run dev:notes` serves the shared Notes business on D1. Stop the greeting dev server before using the same port.
- `bun run types` regenerates each entry's bindings (`Env` and `NotesEnv`). `bun run typecheck` checks both entries separately. `bun run build` and `bun run build:notes` create local dry-run bundles; they do not deploy.

Use oRPC at `/rpc`: greeting `greet({name})`; Notes `create({title,body?})`, `list()`, `update({id,title,body?})`, and `remove({id})`. For example: `curl -H 'content-type: application/json' --data '{"json":{"title":"First note"}}' http://127.0.0.1:8787/rpc/create`.

Notes imports the real D1 adapter, shared schema and async query factory. The Bun SQLite import in that query module is type-only. D1 bindings are platform-owned and are neither migrated nor closed by plugin setup. Local D1 persists in `.wrangler/state` across dev-server restarts. In `wrangler.notes.jsonc`, the zero-filled UUID ending in `1` is a local placeholder, not a cloud resource; `remote:false` keeps this example local. No deployment, login, remote database, Hyperdrive, or production D1 behavior is covered here.

The Worker entry imports no Bun listener or CLI Engine. Wrangler/workerd executes Fetch directly, with no remote Bun backend. The basic copyable Workers template is in `templates/workers`; Notes intentionally reuses the database example rather than copying its business code.
