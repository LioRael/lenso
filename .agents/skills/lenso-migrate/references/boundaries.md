# Storage, authorization and Runtime boundaries

## Persistence is not a type translation

Inventory what is durable, its owner, schema/version, migration command, transactions and recovery states. TS Drizzle/Bun SQL/D1/R2 adapters do not translate to a named Rust package. The native JSON-lines example is a source reader; Rust Cache is process-local, not Storage. Until the Storage task is integrated, use a chosen native library explicitly or report the missing adapter; don't publish proposed branch APIs in examples.

Preserve DB constraints and transaction boundaries, object ownership, conditional writes, streaming/ranges and provider-specific signing support. A signed URL is a bearer credential, not safe log output. Bytes should remain streamed through the right entry; do not base64-wrap every file into finite CLI/MCP JSON. Object upload and metadata DB commit are separate operations; retain compensation/reconciliation and don't report partial failure as success.

Migrations are explicit authorized operations, not hidden startup behavior. Durable tasks need real leases, retries, cancellation state and idempotency/fencing at the effect boundary; a native async function/stream is not a queue. Use targeted persistence/permission tests for a changed invariant. A memory fixture proves neither durability nor provider parity.

## Native calls versus transport

Inside one service, keep TS objects and Rust native values. At a real cross-service boundary, the Runtime-owned generator uses a restricted contract and typed TS/Rust clients. Map inputs/outputs/business errors deliberately: optional versus nullable, safe JSON integers, time/byte formats, stream bounds and deadlines. See Runtime `PROTOCOL.md` and the `lenso-runtime` skill for the actual subset.

Source-first authoring can generate JSON contracts; the verified baseline still has a JSON-input generator. Treat source-first integration as pending/candidate until its export and running example are checked. Do not force contracts onto all plugins, or claim arbitrary functions/classes/Arc/database handles can cross automatically.

Connection registered/unready → native setup → explicit handler binding → activation/ready. Native adapters differ from SDK-only hosts. Bind the **exact installed** connection and service instances; exported method names are not dependency resolution. In the TS adapter use post-setup `exports` and serving `startRuntimeApp`; Rust has `RuntimeExport` and `start_runtime_app`. Older Rust adapter exposes no transport selector even though current Runtime SDK supports TCP/stdio/WebSocket.

Forward trace/cancellation explicitly. Await stream send/iteration backpressure; retain execution lifetime through cleanup. Cancellation/deadline/connection loss are not write rollback or safe replay; no automatic retry/exactly-once/distributed transaction is supplied.

At each trusted entry, authenticate with its actual provider. Enforce owner/tenant/object permission in the shared resource service, including when reused by CLI/MCP/Console. A serialized TS Auth Actor, Rust struct field, Go context value or Runtime service name is not automatically an authenticated user. Keep unsafe errors internal and map only approved public codes/data.
