# Drizzle role stores

`postgresRoleStore(db, namespace, actions)`, `d1RoleStore(db, namespace, actions)`
and `sqliteRoleStore(db, namespace, actions)` borrow the supplied native Drizzle
database. Reads and writes use the same core role-graph validation, including
known actions, inheritance and scoped bindings; returned snapshots are immutable.

Apply `migrations/0001-role-graphs-pg.sql` or
`migrations/0001-role-graphs-sqlite.sql` through your explicit migration owner,
then call `store.initialize(snapshot)` once using trusted bootstrap configuration.
This is an internal seed API, not an unauthenticated management operation.
An existing namespace is preserved. Construction and setup run no migration or
initialization. PostgreSQL JSON is explicitly bound as text and cast to JSONB to
match the repository's Bun SQL/Drizzle serialization pattern.

Each namespace is stored as one graph document. Compare-and-swap is a single
conditional `UPDATE ... WHERE namespace = ? AND revision = ? RETURNING ...`.
It does not wrap authorization data and resource facts in a transaction.

All writers must use a new, never-reused revision token; management generates a
UUID for every mutation. Reusing the current revision is rejected. Raw store
access is trusted infrastructure, not an API for clients to choose revisions.
One graph document makes a role/binding update indivisible within that row,
but serializes writes in the namespace and is bounded to the core's graph limits.
It is not a scalable relationship database.

PostgreSQL supports transactions, but this adapter needs only one conditional
statement. D1 uses the same SQLite statement and no interactive transaction.
SQLite and local Miniflare exercise `UPDATE RETURNING` with the actual driver.
Cancellation is checked before and after read/CAS. Cancellation or connection
loss after commit can leave a successful mutation with an uncertain caller
outcome: no rollback or automatic retry is promised.

The borrowed database controls consistency. PostgreSQL repeatable-read
transactions/replicas can retain older snapshots. D1's direct binding queries
use primary routing per the current docs; an injected D1 Sessions API/replica
client must be configured for the application's revocation requirement.
`first-unconstrained` can begin stale; use a fresh `first-primary` session where
latest primary data is required. The adapter does not infer or override routing.

The update/returning pattern follows Drizzle's
[PostgreSQL](https://orm.drizzle.team/docs/update) and
[SQLite](https://orm.drizzle.team/docs/sqlite/update) update APIs. D1 uses
SQLite-compatible SQL through Drizzle's D1 driver; its prepared statement API
documents SQLite binding semantics and result retrieval at
[Cloudflare D1](https://developers.cloudflare.com/d1/worker-api/prepared-statements/).
Replication/consistency reference:
[D1 global read replication](https://developers.cloudflare.com/d1/best-practices/read-replication/).
