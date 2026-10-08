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
bun run cli create "First note" "Stored in PostgreSQL"
bun run cli list
bun run cli update <id> "Updated note" "New body"
bun run cli remove <id>
bun run serve
```

Create the database beforehand. `serve` exposes oRPC at
`http://127.0.0.1:3001/rpc` (`LENSO_PORT` overrides the port). CLI and Web call
the same ordinary async `NotesService`: `create`, `list`, `update`, and
`remove`. The service validates titles and bodies, generates UUIDs, and returns
ISO timestamps. Missing updates return `null`; missing removals return `false`.
The example is a local development server.

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

A Worker composes the same service using its explicit binding:

```ts
import { createD1Plugin } from "@lenso/db/d1";
import { startApp } from "lenso";
import { createNotesPlugin } from "./src/notes";
import { createSqliteNotesQueries } from "./src/queries-sqlite";
import * as schema from "./src/schema-sqlite";

const database = createD1Plugin({ id: "notes-db", binding: env.DB, schema });
const notes = createNotesPlugin({ id: "notes", database, queries: createSqliteNotesQueries });
const app = await startApp({ plugins: [database, notes] });
try {
  return Response.json(await app.get(notes).list());
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

Driver references: [Bun SQL](https://orm.drizzle.team/docs/connect-bun-sql),
[Bun SQLite](https://orm.drizzle.team/docs/connect-bun-sqlite),
[D1](https://orm.drizzle.team/docs/connect-cloudflare-d1),
[D1 migrations](https://developers.cloudflare.com/d1/reference/migrations/).
The package pins stable Drizzle ORM 0.45.3 and Kit 0.31.11.
