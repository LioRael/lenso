# App migration history

The `0000_notes.sql` files are unchanged. `0001_owner.sql` upgrades old rows to
the reserved `__legacy_unowned__` owner, which login configuration rejects.
No existing private data is assigned to a configured principal.

Drizzle Kit 0.31.11 generated the owner-column snapshots and journal entries.
The reviewed SQL adds the legacy backfill and copies the Auth package's
`migrations/pg/0000_auth_sessions.sql` and
`migrations/sqlite/0000_auth_sessions.sql` verbatim (with runner breakpoints).
Auth owns that table; the Notes Drizzle snapshots deliberately track only
Notes, not a second application-owned Auth schema. SQLite retains a sentinel
column default to permit a non-null additive upgrade without rebuilding the
old table. Notes always writes its authorized owner explicitly.

Both Notes and sessions run through the existing app Drizzle migrator and its
single migration history. Do not run a separate Auth migration runner.
