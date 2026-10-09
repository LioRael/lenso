# @lenso/search

PostgreSQL keyword search over a separate, bounded projection. Ordinary async
`upsert(scope, document)`, `delete(scope, {id,type})` and `query(scope, input)`.
No listener, migration, scan, worker, cache or client is started by import,
construction or plugin setup.

## Host assembly

```ts
import { createSearchService } from "@lenso/search";
import { createPostgresSearchProvider, postgresSearchMigration } from "@lenso/search/postgres";
import { bunSqlSearchDatabase } from "@lenso/search/bun-sql";

// db is the host's existing @lenso/db/bun-sql Drizzle resource.
const database = bunSqlSearchDatabase(db.$client);
const provider = createPostgresSearchProvider({ database, language: "simple" });
const search = createSearchService({ provider, cursorSecret: hostSearchCursorSecret });
// Explicit migration workflow only, not application startup:
// await db.$client.unsafe(postgresSearchMigration()).simple();
const scope = { namespace: "notes", tenantId: authorizedTenant, ownerId: authenticatedOwner };
await search.upsert(scope, {
  id: note.id,
  type: "note",
  tenantId: scope.tenantId,
  ownerId: scope.ownerId,
  title: note.title,
  body: note.body,
  metadata: { source: "notes" },
});
const page = await search.query(scope, { text: "meeting", pageSize: 20, includeTotal: true });
await search.delete(scope, { id: note.id, type: "note" });
```

The host owns authorization, DB/client, migrations, listeners, tasks and shutdown.
`SearchScope` is **trusted host output, not client input or an authorization token**.
Search cannot verify that a host grants the correct scope. Authenticate at the
entry and derive it from the existing Auth/authorization policy on every call.
Never expose raw Search methods as unauthenticated CLI/MCP/Manage/HTTP operations.
Only an exact `(namespace, tenantId?, ownerId?)` partition is selected. Missing
optional IDs select the explicitly host-authorized public/single-user partition,
**not** a wildcard. Arrays, wildcard fields, missing namespace and conflicting
document scope fail. Cross-owner/tenant unions and administrator global search
are unsupported. A public scope needs an explicit host policy too.

## Storage, language and capabilities

- PostgreSQL 12+ generated weighted `tsvector`: title A, body B; query via
  `websearch_to_tsquery`, rank via `ts_rank`, GIN text index and scope B-tree.
  Values use parameters; table names match `[a-z_][a-z0-9_]{0,47}`.
- `simple` retains tokens without stemming; `english` uses PostgreSQL English
  stemming/stopwords. Both must match the projection's language. Changing language
  requires explicit scoped reindexing. Chinese segmentation quality is **not
  verified or promised**. No vectors, embedding or model calls.
- Capability object: real full text, relevance and ID sorts, bounded offset
  pagination, plain-text summary, optional exact authorized count. No other backend
  is advertised. Empty/whitespace or tokenless query returns no hits, not browsing.
- Stable source reference is `{type,id}` within the caller's scope. Duplicate IDs
  replace only that scope/type's projection. Same content upserts and missing
  deletes are idempotent. Scope/type changes require explicit old-scope deletion.
- Limits default to 50 results, offset 10,000, query 512 characters, title 1,024,
  body 100,000, document 512 KiB, metadata 4 KiB, summary 240 characters.
  Metadata accepts at most 16 named scalar fields, never nested objects or full
  business records. Owner IDs support the existing Notes 512-character identity
  bound; namespace/tenant are at most 128, type 64, ID 256. Composite key text
  totals at most 2,400 UTF-8 bytes for PostgreSQL's default 8 KiB B-tree pages.
  All payloads are copied/validated before I/O.

Authorization predicates enter the SQL candidate set **before** rank, count and
headline generation. Only a bounded page and optional scalar total leave the DB.
Summaries are bounded **plain text**, not sanitized HTML: render titles, summaries
and metadata as text nodes, never `innerHTML`. No executable highlight HTML is
generated. Errors use fixed structured codes and never retain driver SQL, connection
details or document bodies. `searchErrorDiagnostic` is the safe public projection.

## Pagination and consistency

Relevance order is score descending, then type/ID ascending with PostgreSQL `"C"`
collation; ID order is type then ID. `nextCursor` is a signed bounded offset bound
to the full scope, normalized text, type, sort, page size, count choice, provider
and limits. The host supplies at least 32 random bytes, keeps them secret and uses
the same key across instances. Rotation invalidates cursors. A cursor grants no
authorization. Stop after `nextCursor` is absent; at the configured offset ceiling
further traversal stops even if `total` is larger. Counts and hits share one SQL
statement snapshot. Separate pages do not share a snapshot: concurrent inserts,
updates and deletes can repeat or omit results. There is no version/CAS protection;
hosts must serialize writes per ID or explicitly reconcile latest projections.

An autocommitted successful upsert/delete is immediately visible to new DB
statements; borrowed transaction clients follow their host's commit/isolation.
Business CRUD and projection writes are not atomically committed together.
Use the [Notes adapter](examples/notes.ts) and its [wiring notes](examples/README.md)
for current-user queries, CRUD and bounded existing-Tasks retries/authorized repair.
No global backfill is supplied. Disabling rejects reads/writes but preserves tables
and borrowed resources; hosts choose retention, cleanup and later reconciliation.

## Optional plugin and integration owner checklist

`@lenso/search/plugin` exports `searchConfig` and `createPostgresSearchPlugin`.
Pass the **exact existing DB plugin**, `adapter: db => bunSqlSearchDatabase(db.$client)`,
trusted cursor bytes and config or existing `ConfigSource[]`. It uses core
`bindConfig`, with table/language/enable/text/page limits; it neither migrates nor
closes the borrowed DB. No Manage/CLI/MCP exposure is installed. Any future
configuration diagnostics must use existing Manage permissions and redaction.
Caching is deliberately absent; future cache keys must bind scope and all query
parameters using the existing Cache package.

Integration owner: root `packages/*` already discovers this package; regenerate the
single Bun lockfile, approve cross-package dependencies/release exports, add
`@lenso/search: workspace:^` to Notes, explicitly migrate, assemble Search with the
existing DB and Auth, replace the installed Notes service with the adapter's
`notes`, wire a separately authorized `query` entry, and register the repair task
in existing Tasks if used. Direct Auth-authorized one-ID repair requires no ticket
store. Implement `readLatest` as SQL scoped by owner/tenant/ID; optional durable
retries need the host's existing task ownership records (not provided by Tasks
core). Existing
Notes has no tenant column: a host tenant mapping is not a tenant CRUD policy.
No files outside this package are changed.

## Checks

```sh
bun run --cwd packages/search lint
bun run --cwd packages/search build
bun run --cwd packages/search typecheck
SEARCH_TEST_DATABASE_URL=postgres://... bun run --cwd packages/search test
cd packages/search && bun pm pack --ignore-scripts
SEARCH_TEST_DATABASE_URL=postgres://... bun run test:pack
```

Build framework dependencies first (exports resolve `dist`). PostgreSQL tests
create/drop only random owned tables in a **host-authorized disposable DB**;
without the URL they are explicitly skipped, not replaced by mocks. The suite
includes real CRUD visibility, isolation sentinels, injection, summaries, cursors,
fault/retry and a finite 6,000-row actual-query `EXPLAIN ANALYZE` index check.
