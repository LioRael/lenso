# Local Workers applications

From the repository root run `bun install` and `bun run build`. In this directory:

- `bun run dev` serves the in-memory greeting. `GREETING_PREFIX` comes from the binding declared in `wrangler.jsonc`; optionally copy `.dev.vars.example` to `.dev.vars`. Each request gets a fresh app, so count resets to 1.
- `bun run migrate:notes` explicitly applies `examples/notes/migrations/sqlite` to Wrangler's local D1, using its `d1_migrations` table. This includes Notes ownership and Auth sessions. Supply the `NOTES_LOGIN_KEYS` secret binding before `bun run dev:notes`; stop the greeting dev server before using the same port.
- `bun run types` regenerates each entry's bindings (`Env` and `NotesEnv`). `bun run typecheck` checks both entries separately. `bun run build` and `bun run build:notes` create local dry-run bundles; they do not deploy.

Use oRPC at `/rpc`: greeting `greet({name})`; Notes `create({title,body?})`,
`list()`, `read({id})`, `update({id,title,body?})`, and `remove({id})`. Notes also
serves raw Fetch REST at `/notes` and `/notes/:id`; all call the same service
authorization, including current session, operation audience and object owner.
Anonymous access is rejected and each user sees only their own notes.

`NOTES_LOGIN_KEYS` is a JSON array of unique `{subjectId,key}` records, where each
key is independently generated random 32 bytes encoded as 64 hex characters.
Use the generation example in `docs/DATABASE.md`, and supply the result via
Wrangler's ignored local secret/env configuration or your secret authority.
Do not place real keys in `wrangler.notes.jsonc` or any tracked file. This is a
Notes-owned API-key source, not a User schema required by Auth.

`POST /session` with JSON `{"key":"<configured login key>"}` returns a session
credential. Send it in the Bearer header, including with oRPC. For example,
with `NOTES_SESSION` already set:

```sh
curl -H "Authorization: Bearer $NOTES_SESSION" \
  -H 'content-type: application/json' \
  --data '{"json":{"title":"First private note"}}' \
  http://127.0.0.1:8787/rpc/create
```

`POST /session/renew` rotates the Bearer credential, and `POST /session/revoke`
invalidates it. `NOTES_RENEW_AFTER_MS` is an optional trusted lifetime setting;
it defaults to 60000 milliseconds. There is no cookie credential fallback.

Notes imports the real D1 adapter, shared schema and async query factory. The Bun SQLite import in that query module is type-only. D1 bindings are platform-owned and are neither migrated nor closed by plugin setup. Local D1 persists in `.wrangler/state` across dev-server restarts. In `wrangler.notes.jsonc`, the zero-filled UUID ending in `1` is a local placeholder, not a cloud resource; `remote:false` keeps this example local. No deployment, remote database, Hyperdrive, or production D1 behavior is covered here.

## Local workerd checks

`bun run test:notes` bundles this existing entry and uses Node's test runner to
drive Miniflare's actual workerd process and D1 database. It does not substitute
a self-made SQLite binding. It verifies private REST/RPC CRUD, ownership/realm
denials, revoked credentials, competing renewals and revoke/renew races.
Bindings use generated disposable test keys; databases are isolated and disposed.
The local fixture uses compatibility date 2026-10-06 for the pinned runtime.
It does not prove production region/replication behavior.

The Worker entry imports no Bun listener or CLI Engine. Wrangler/workerd executes Fetch directly, with no remote Bun backend. The basic copyable Workers template is in `templates/workers`; Notes intentionally reuses the database example rather than copying its business code.

Both configs enable `enable_request_signal` so network disconnects reach the Web source and the adapter can keep asynchronous app cleanup alive with the platform's `waitUntil`. Runtime termination can still interrupt cleanup; resources and non-cancellable producers must respect the platform lifetime.

## Optional R2 objects

`src/storage.ts` shows two explicitly referenced native R2 instances. Call `createStorageInstances(env)` inside the existing Worker app factory after supplying `PUBLIC_ASSETS` and `PRIVATE_FILES` bindings in your own Wrangler configuration, and include its returned plugins. The example does not provision buckets, enable public access or install private HTTP routes. Binding uploads require known byte length and do not support presigned URLs. For authorization, file records/D1 and raw Fetch or S3-based direct upload, see [`@lenso/storage`](../../packages/storage/README.md).
