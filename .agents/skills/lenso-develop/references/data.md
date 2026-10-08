# DB, Files and Tasks

Read only the changed resource branch. A new projection/operation over an unchanged authorized service needs that service's contract, not database or migration manuals.

## Native database resource

Follow [Database resources](https://github.com/LioRael/lenso/blob/main/docs/DATABASE.md). Choose `@lenso/db/bun-sql` for Bun PostgreSQL, `/bun-sqlite` for Bun SQLite or `/d1` for Workers D1. Preserve native Drizzle types and exact resource dependencies. New clients belong to their resource plugin; supplied clients/bindings stay borrowed. Startup does not create tables or apply migrations: use the application's explicit migration workflow against an authorized local/test database.

## Private objects versus authorized files

Read [Storage and Files](https://github.com/LioRael/lenso/blob/main/packages/storage/README.md) and [Notes Files integration](https://github.com/LioRael/lenso/blob/main/examples/notes/src/files.ts).

- Object services are trusted internal APIs; a key or storage ID grants no caller permission. Authorized file records require the application's authenticated access and explicit action/owner/tenant policy.
- Keep bytes streamed through raw Fetch handlers, not JSON CLI/MCP output. Metadata/initiation/completion can reuse typed services. Local roots are dedicated app-controlled paths; native R2 bindings use `/r2`, not Bun `/local` or `/s3`.
- Check provider capabilities before selecting signing/ranges/conditions. Signed links are bearer credentials; send required headers and keep links out of logs. Upload completion validates real object metadata; signed-upload size limits are not provider traffic limits.
- Object writes and DB commits are separate, not a cross-resource transaction. Preserve publication/deletion states, compensation and reconciliation behavior rather than turn partial failure into success.

## Durable tasks

Use [Tasks contracts](https://github.com/LioRael/lenso/blob/main/packages/tasks/README.md), [Tasks plugin](https://github.com/LioRael/lenso/blob/main/examples/tasks/src/plugin.ts) and [authorized task service](https://github.com/LioRael/lenso/blob/main/examples/tasks/src/authorized-service.ts).

- Queue payloads contain serializable business data, not actors, credentials, resource handles or an authorization grant. Persist and authorize durable ownership before exposing private query/cancel/retry.
- Handlers may overlap after lease loss/retry; make external effects idempotent or fenced at their real boundary. Deduplication does not make every effect exactly-once.
- Running cancellation is a durable **request**, cooperatively signalled to a handler; it does not prove stopped work or roll back effects. Keep this separate from CLI/MCP request cancellation.
- Stop claiming and await handlers before closing owned queue/database resources. Provider schema initialization/migrations are explicit, not consumer startup work. PostgreSQL integration tests require a disposable test database; report skips rather than imply persistence was tested.
