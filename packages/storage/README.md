# @lenso/storage

Private object storage with explicit Lenso instance references. Business methods are ordinary async functions. The package root has no filesystem, AWS SDK, database or Web imports.

| Import | Runtime / optional dependency |
| --- | --- |
| `@lenso/storage` | Object types, errors and plugin factory |
| `@lenso/storage/local` | Bun, dedicated local directory |
| `@lenso/storage/s3` | Bun, AWS SDK v3 `client-s3`, `lib-storage`, `s3-request-presigner` |
| `@lenso/storage/r2` | Workers native R2 binding, no AWS SDK |
| `@lenso/storage/files` | Optional authorized file records, database supplied by application |
| `@lenso/storage/sqlite` | Drizzle schema/queries for Bun SQLite or D1 |
| `@lenso/storage/postgres` | Drizzle schema/queries for Bun SQL PostgreSQL |
| `@lenso/storage/fetch` | Optional raw Fetch helpers, no listener or implicit routes |

Install `lenso` and this package. Install the three AWS SDK peers for `/s3`, or `drizzle-orm@0.45.3` for the database subpaths. Do not import `/local` or `/s3` in a Workers entry. Inspect `storage.capabilities` before selecting signing, conditional or range operations; unsupported operations throw `StorageError` with `code: "unsupported"`.

## Multiple instances and object streams

```ts
import { defineApp, startApp } from "lenso";
import { createLocalStoragePlugin } from "@lenso/storage/local";

const publicAssets = createLocalStoragePlugin({ id: "publicAssets", root: "./data/assets" });
const privateFiles = createLocalStoragePlugin({ id: "privateFiles", root: "./data/private" });
const app = await startApp(defineApp({ plugins: [publicAssets, privateFiles] }));
try {
  const storage = app.get(privateFiles); // No global default.
  await storage.put({
    key: "contracts/unique-id.pdf", body: request.body!,
    contentType: "application/pdf", maxBytes: 20 * 1024 * 1024, signal: request.signal,
  });
  const download = await storage.get("contracts/unique-id.pdf");
  const response = new Response(download.body, {
    headers: { "content-type": download.metadata.contentType },
  });
  // Return/consume the response before stopping the app.
  await response.body!.pipeTo(destination);
  const metadata = await storage.head("contracts/unique-id.pdf"); // null only for missing
  const page = await storage.list({ prefix: "contracts/", limit: 100 });
  if (page.cursor) await storage.list({ prefix: "contracts/", cursor: page.cursor, limit: 100 });
  await storage.delete("contracts/unique-id.pdf"); // safe retry; provider errors still throw
} finally {
  await app.stop();
}
```

Object services are **trusted internal APIs**, not public endpoints. Knowing a key grants no application permission. Even `publicAssets` stays private: names never enable public ACLs. All uploads create a new key; an existing key causes `conflict`, not overwrite. Keys are not original filenames. Metadata includes provider size, content type, ETag, timestamp, version and custom metadata where available. Listing cursors are backend-specific, not portable or snapshot guarantees.

## S3-compatible and R2

```ts
import { createS3StoragePlugin } from "@lenso/storage/s3";
const privateFiles = createS3StoragePlugin({
  id: "privateFiles", bucket: "private-files",
  clientConfig: { region: "us-east-1" }, // SDK credential chain; no credentials in source
});
// R2 S3 API uses the same adapter, separately from the binding:
const r2S3 = createS3StoragePlugin({
  id: "r2S3", bucket: "files",
  clientConfig: { region: "auto", endpoint: process.env.R2_S3_ENDPOINT },
  multipart: false,
});
```

S3-compatible providers must honor conditional create (`If-None-Match: *`). Multipart mode additionally requires conditional `CompleteMultipartUpload`. AWS supports it; R2 does not document that condition on completion. Known R2 endpoints select single-PUT mode, which requires `size`; use `multipart: false` explicitly for other providers without conditional multipart completion. The SDK bounds multipart buffering rather than loading the whole object; `maxUploadBytes` exposes this adapter's limit (5 GiB single PUT; 10,000 × 5 MiB multipart parts). Supplied clients remain application-owned; single-PUT clients need `requestChecksumCalculation: "WHEN_REQUIRED"`. Never give an SDK client a payload-logging logger.

```ts
import { createR2StoragePlugin } from "@lenso/storage/r2";
const privateFiles = createR2StoragePlugin({ id: "privateFiles", binding: env.PRIVATE_FILES });
// Native binding upload needs known size and Workers FixedLengthStream.
await app.get(privateFiles).put({
  key: crypto.randomUUID(), body: request.body!, size: trustedExpectedSize, maxBytes: serverLimit,
});
```

R2 binding has no presigner and no provider request-abort API. Cancellation interrupts the source stream; an already-running platform operation must settle. For R2/local downloads use an application-authorized Fetch endpoint, or separately configure the S3 adapter for R2 presigned URLs. No binding or externally supplied client is closed.

`examples/workers/src/storage.ts` also demonstrates both native binding instances and checks against real Workers `R2Bucket` types. It adds no bucket configuration or routes.

## Optional file records and authorization

Reuse the existing Notes example, not a second application:

```sh
bun install
bun run build
mkdir -p output
cd examples/notes
SQLITE_PATH=../../output/notes-files.sqlite bun run migrate:sqlite
SQLITE_PATH=../../output/notes-files.sqlite bun run files:migrate
SQLITE_PATH=../../output/notes-files.sqlite bun run files:demo
```

`examples/notes/src/files.ts` binds two local instances, the existing Notes database resource and an owner/tenant policy. The demo streams a private upload, gets a stable `fileId`, downloads it and retries deletion. It supplies a local demo actor, not production authentication.

Import `fileSchema` and `createSqliteFileQueries` (or the PostgreSQL equivalents), include the table in your existing database schema, then use `createFilesPlugin({ id, storages, database, queries, authorize })`. `authorize({ access, action, file })` is required for every file operation; omitting it denies access. Owner/tenant fields are application associations, not a user system. Applications authenticate `access` and validate assignments and quota/type policies themselves. The plugin neither creates identities nor assumes future Auth APIs.

Migrations belong to this package at `migrations/sqlite/0001_files.sql` and `migrations/postgres/0001_files.sql`. Execute them explicitly using your database migration process; D1 can use `wrangler d1 migrations apply --local`. Starting the plugin never changes the schema. Object-only use requires no database.

## Direct upload and signed download

```ts
const files = app.get(filePlugin);
const { file, link } = await files.beginUpload(access, {
  storageId: privateFiles.id, filename: "contract.pdf", contentType: "application/pdf",
  maxBytes: 20 * 1024 * 1024, expiresIn: 300,
  ownerId: access.ownerId, tenantId: access.tenantId,
});
// Send link only to the authorized client, never a logger.
await fetch(link.url, { method: link.method, headers: link.headers, body: browserFile });
const ready = await files.completeUpload(access, file.fileId);
const download = await files.signDownload(access, ready.fileId, 60);
await fetch(download.url, { headers: download.headers });
```

Send **all returned headers**. Signed PUT links bind the key, expiry, content type and create-only condition. `maxBytes` is a completion check, **not a provider-enforced upper bound on uploaded traffic**. Completion reads real object metadata and checks existence, size and content type before publishing a ready record; it can verify an already uploaded object after the PUT credential expires. Reads bind the saved ETag where supported. Temporary credentials can expire earlier than the requested TTL. Browser direct access needs application-managed bucket CORS.

URLs are temporary bearer credentials and are not logged by this plugin. Do not put them in analytics, request logs, ordinary RPC/CLI diagnostics or durable file records. A signed link cannot be revoked by removing an application permission. File deletion refuses while its PUT link is live, even after publication: retry after expiry. Failed completion may compensate an invalid object earlier; a still-live PUT can recreate an orphan, but its failed file record cannot become ready. Wait for expiry before reconciling such orphaned keys.

## Raw Fetch, failures and boundaries

`createFileUploadHandler({ files, authenticate, input })` and `createFileDownloadHandler({ files, authenticate, fileId })` return raw Fetch-context handlers usable through `createWebPlugin({ fetch: context => handler, ... })`. Applications choose routes, supply authentication and server-controlled upload limits/owner/tenant inputs. Downloads default to attachment, `nosniff` and `private, no-store`; custom inline serving needs application policy. File bytes stay `ReadableStream`s; metadata, direct-upload initiation/completion and business commands can use the existing typed RPC router. Nothing starts an HTTP server or automatically exposes a private route.

Object write and database commit are separate operations. File states and conditional transitions keep incomplete uploads unpublished; compensation first claims an unpublished failed state, so an ambiguous DB response cannot delete an already committed ready object. Failed deletions remain retryable, including partial local removal. Cleanup/database/provider failures are retained rather than converted to success. Process termination, ambiguous provider writes or an unavailable DB can leave incomplete work for application reconciliation; there is no implicit background worker or cross-resource transaction.

Local roots must be dedicated, application-controlled directories, not upload-selected paths. Traversal and symlinks are rejected, but this is not a sandbox against a hostile process concurrently modifying the filesystem. Do not modify the adapter's on-disk object files manually.

No public bucket provisioning, virus scanning, image processing, thumbnails, deduplication, CDN management, cross-cloud copying, resumable client multipart sessions or Console UI is included.

Official references: [AWS Upload](https://docs.aws.amazon.com/AWSJavaScriptSDK/v3/latest/Package/-aws-sdk-lib-storage/), [Bun Node compatibility](https://bun.com/docs/runtime/nodejs-compat), [R2 binding API](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/), [R2 S3 compatibility](https://developers.cloudflare.com/r2/api/s3/api/), [R2 presigned URLs](https://developers.cloudflare.com/r2/api/s3/presigned-urls/), [Workers FixedLengthStream](https://developers.cloudflare.com/workers/runtime-apis/streams/transformstream/).
