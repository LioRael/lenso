# Database resources and persistent notes

`@lenso/db` binds a native Drizzle database to a Lenso plugin instance. Each
resource requires an explicit `id` and schema. Consumers declare the exact
resource in `requires` and obtain it with `context.get(resource)`. Different
IDs and references support multiple databases and multiple business instances.

| Import                 | Factory                                           | Native database    | Ownership                 |
| ---------------------- | ------------------------------------------------- | ------------------ | ------------------------- |
| `@lenso/db/bun-sql`    | `createBunSqlPlugin({ id, schema, connection })`  | Bun SQL PostgreSQL | Closes its new pool       |
| `@lenso/db/bun-sqlite` | `createBunSqlitePlugin({ id, schema, filename })` | Bun SQLite         | Closes its new connection |
| `@lenso/db/d1`         | `createD1Plugin({ id, schema, binding })`         | Cloudflare D1      | Platform owns binding     |

The Bun factories also accept an existing `client` instead of a connection or
filename; its caller retains ownership. SQLite accepts native constructor
`options`; PostgreSQL accepts a URL or native PostgreSQL connection options.
Cleanup runs on application stop and setup rollback. The factories neither
create tables nor apply migrations. D1's entrypoint imports no Bun runtime code.

For another Drizzle driver, `createDrizzlePlugin({ id, requires?, connect(context) })`
from `@lenso/db` retains the factory's exact return type. Register cleanup with
`context.onCleanup` immediately after acquiring a resource you own. Do not
register cleanup for borrowed clients or platform bindings.

## Run the PostgreSQL notes example

From the repository root, install with Bun and build dependencies:

```sh
bun install
bun run build
cd examples/notes
export DATABASE_URL='postgres://localhost/lenso_notes'
bun run migrate:pg
export NOTES_LOGIN_KEYS="$(bun -e 'console.log(JSON.stringify(["alice","bob"].map(subjectId => ({subjectId,key:Array.from(crypto.getRandomValues(new Uint8Array(32)),b=>b.toString(16).padStart(2,"0")).join("")}))))')"
export NOTES_LOGIN_KEY="$(bun -e 'console.log(JSON.parse(process.env.NOTES_LOGIN_KEYS)[0].key)')"
export NOTES_SESSION="$(bun run cli login | bun -e 'console.log((await Bun.stdin.json()).credential)')"
printf '%s' '{"title":"First note","body":"Stored in PostgreSQL"}' | bun run cli create
bun run cli list '{}'
bun run cli read '{"id":"<note UUID>"}'
bun run cli update '{"id":"<note UUID>","title":"Updated note","body":"New body"}'
bun run cli remove '{"id":"<note UUID>"}'
bun run serve
```

Create the database beforehand. `serve` exposes oRPC at
`http://127.0.0.1:3001/rpc` (`LENSO_PORT` overrides the port), plus raw Fetch REST
at `/notes` and `/notes/:id`. CLI, Fetch and oRPC call the same ordinary async
`NotesService`: `create`, `list`, `read`, `update`, and `remove`. Each operation
checks a trusted actor for its exact audience, revalidates the session and
checks the actual owner. Lists contain only the caller's records; update/delete
also include the authorized owner in the SQL condition.

The login source belongs to Notes, not Auth: it verifies explicitly configured
random 32-byte hex keys and does not add a User table. The commands above generate
keys into the environment, not a committable file. Keep those keys stable across
restarts and supply them through your deployment's secret authority. They are
API login keys, not passwords, OAuth or a full account-management example.

`NOTES_SESSION` supplies CLI credentials without putting them in argv. HTTP
clients send `Authorization: Bearer <credential>`. `POST /session` accepts
`{"key": "<configured login key>"}`; `POST /session/renew` rotates the Bearer
credential; `POST /session/revoke` revokes it. CLI `renew`/`revoke` use the same
session owner. After revocation, the old credential cannot access any private
operation. Login and renewal deliberately return a credential; never log it or
put it in a URL. The Bun listener is a loopback development server, not a TLS
production ingress.

The service validates titles and bodies, generates UUIDs, and returns ISO
timestamps. An authenticated caller gets null/false for missing records;
anonymous callers are rejected even for missing IDs. Owner, ID and creation
time cannot be assigned through transport input.

### Explicit business operations

`examples/notes/lenso.config.ts` exports an explicit operation allowlist.
`notes-operations` exposes `create`, `list`, `read`, `update`, and `remove`,
each with one JSON business input. `src/contracts.ts` owns the strict schemas
used by Notes service validation, Web and CLI operations: trimmed titles of
1 to 200 characters, bodies up to 20,000 characters, UUID lookups, and no extra
fields. Direct service input with extra owner/actor fields is rejected too.
The thin application methods obtain an audience-specific actor from trusted
`NOTES_SESSION`, then delegate to the existing service. JSON never supplies
an actor. `login`, `renew`, and `revoke` remain trusted session entry commands,
not exposed business operations.

From the repository root, use the standard CLI for discovery and invocation:

```sh
bun packages/cli/src/bin.ts inspect notes-operations create --root examples/notes --json
printf '%s' '{"title":"From the operation registry"}' |
  bun packages/cli/src/bin.ts call notes-operations create --root examples/notes --stdin --json
printf '%s' '{}' |
  bun packages/cli/src/bin.ts call notes-operations list --root examples/notes --stdin --json
```

Static inspection does not open databases, initialize storage, verify a session
or require login keys. Each valid call starts and stops one app. Invalid input
and undeclared methods fail before setup; safe Auth codes survive the CLI
boundary without arbitrary error text.

Set `DATABASE_URL` without `SQLITE_PATH` to use PostgreSQL Notes. Set
`SQLITE_PATH` for local SQLite Notes and attachments (it takes precedence);
without either, the config describes local defaults under `output/`.
Config paths are resolved against the Notes application root, independently
of the invoking process's working directory. Migration scripts retain their
normal cwd semantics. Migrate explicitly with `files:migrate` before local
calls; use the same absolute
`SQLITE_PATH`, `STORAGE_ROOT`, and login configuration for login and business calls.
Local configuration also exposes `notes-file-operations.metadata` and
`notes-file-operations.delete`, each accepting only `{"fileId":"<file UUID>"}`:

```sh
printf '%s' '{"fileId":"<file UUID>"}' |
  bun packages/cli/src/bin.ts call notes-file-operations metadata --root examples/notes --stdin --json
```

File authorization revalidates the actual Auth actor for the requested
audience, requires a user, checks the record's owner, and requires the fixed
application tenant `local-notes`. A copied/forged actor, another owner, and
another tenant are denied. The existing Files service still owns metadata
and deletion/state transitions. No binary upload/download operation is exposed;
local storage does not support signed links, so metadata is the discoverable
read operation. `files:demo` remains a trusted streaming demonstration.

For an MCP host, launch `bun /absolute/path/to/examples/notes/src/mcp.ts`
with the same trusted environment. Its fixed application root and explicit
allowlist expose the five Notes operations, not session issuance or Files.
No server starts unless this optional stdio entry is invoked. The host can
select a narrower allowlist in its own trusted entry using
[`serveStdio`](../packages/mcp/README.md); never accept that selection from
tool arguments. Tool discovery retains the canonical operation description
and source in `_meta["lenso/operation"]`. Files metadata/delete can be added
explicitly only for the local configuration that declares them.

`src/schema-pg.ts` uses PostgreSQL UUID and timestamp-with-time-zone columns.
`src/schema-sqlite.ts` uses SQLite text IDs and integer millisecond timestamps.
Dialect-specific queries bind those schemas to the native drivers. The shared
`NotesQueries` interface contains only these business operations; it is not a
generic repository or a replacement ORM.

## Explicit migrations and D1 composition

Generate future changes with `bun run generate:pg` or
`bun run generate:sqlite`, then review the generated SQL. PostgreSQL migrations
live in `migrations/pg`; SQLite/D1 migrations live in `migrations/sqlite`.
`bun run migrate:pg` uses Drizzle's Bun SQL migrator. For a Bun SQLite file,
run `SQLITE_PATH=/absolute/path/notes.sqlite bun run migrate:sqlite`.
Installation and plugin startup never run either command.

The `0001_owner` application migration adds immutable ownership, leaves older
records at a reserved non-login owner, and includes the Auth-owned session
baseline. Both tables use one application migration history: do not also run
a separate Auth migrator. Drizzle snapshots track Notes; Auth owns its session
schema. See `examples/notes/migrations/OWNERSHIP.md`.

A Worker composes the same service using its explicit binding:

```ts
import { createD1Plugin } from "@lenso/db/d1";
import { d1SessionStore } from "@lenso/auth/drizzle/d1";
import { bearerEvidence } from "@lenso/auth/fetch";
import { startApp } from "@lenso/core";
import { createNotesAuthPlugin, parseNotesPrincipals } from "./src/auth";
import { createNotesPlugin, notesAudiences } from "./src/notes";
import { createSqliteNotesQueries } from "./src/queries-sqlite";
import * as schema from "./src/schema-sqlite";

const database = createD1Plugin({ id: "notes-db", binding: env.DB, schema });
const authentication = createNotesAuthPlugin({
  database,
  store: d1SessionStore,
  principals: parseNotesPrincipals(env.NOTES_LOGIN_KEYS),
});
const notes = createNotesPlugin({
  id: "notes",
  database,
  authentication,
  queries: createSqliteNotesQueries,
});
const app = await startApp({ plugins: [database, authentication, notes] });
try {
  const input = bearerEvidence({ request });
  const actor = await app
    .get(authentication)
    .for(notesAudiences.list)
    .required(input.evidence, input);
  return Response.json(await app.get(notes).list(actor));
} finally {
  await app.stop(); // releases app services; does not close env.DB
}
```

The Worker owner configures its D1 binding with `migrations_dir` pointing to
this example's `migrations/sqlite`, and explicitly runs
`wrangler d1 migrations apply <database-name> --local` for local development.
Wrangler owns D1 migration tracking in `d1_migrations`; the Bun SQLite migrator
uses Drizzle's journal. Do not apply both migration runners to the same store.
These queries use single-statement mutations and `RETURNING`; they do not
assume PostgreSQL transactions or synchronous SQLite connection APIs on D1.

## Actual database checks

`LENSO_REQUIRE_POSTGRES=1 bun test packages/auth/test/postgres.test.ts` starts
private loopback PostgreSQL clusters using installed `postgres`, `initdb` and
`pg_ctl`. Separate connections are held in real lock queues before testing
exactly one renewal winner, both revoke/renew orderings and expiry while waiting.
The required flag fails rather than skips if those binaries are absent.

`LENSO_TEST_DATABASE_URL=<isolated test database> bun test examples/notes/test/postgres.test.ts`
exercises Notes through real CLI subprocesses and HTTP, including anonymous,
cross-owner and revoked-session rejection. Use an isolated database, never a
shared application database.

`bun run --cwd examples/workers test:notes` executes the existing Notes Worker
in local workerd with its actual D1 binding. It covers the same private REST/RPC
operations and concurrent session renewal/revocation, not a simulated SQLite
binding. It does not validate production Cloudflare replication or other PG drivers.

Driver references: [Bun SQL](https://orm.drizzle.team/docs/connect-bun-sql),
[Bun SQLite](https://orm.drizzle.team/docs/connect-bun-sqlite),
[D1](https://orm.drizzle.team/docs/connect-cloudflare-d1),
[D1 migrations](https://developers.cloudflare.com/d1/reference/migrations/).
The package pins stable Drizzle ORM 0.45.3 and Kit 0.31.11.
