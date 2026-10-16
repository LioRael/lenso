# Login and private-file recipes

These are editable application code using existing Auth, DB, Storage and Web APIs.
There is no recipe runtime or generated owner of your source. The two HTTP entries
can be removed independently. No Console, MCP or Manage entry is required.

## Run from a clean repository

Use the repository's pinned Bun version (currently 1.4.3). From the checkout root:

```sh
bun run prepare:runtime
bun install --frozen-lockfile
bunx turbo run build --filter=@lenso/example-notes --filter=@lenso/example-tasks
bun run --cwd examples/notes recipes:inspect
bun run --cwd examples/notes recipes:demo
bun test examples/notes/test/recipes.test.ts examples/notes/test/files.test.ts
```

`prepare:runtime` downloads the pinned SDK workspace needed by the root install;
these recipes do not use that SDK at runtime. Build framework dependencies first:
the default package exports point at `dist`.

The demo explicitly migrates a disposable SQLite database, generates two temporary
login keys, starts a loopback HTTP listener on an assigned port, and verifies login,
private upload/download, cross-user refusal, renewal-before-due, deletion and
revocation. It prints no credential and removes its files/listener on exit. The
local storage root is resolved with `realpath` because this adapter rejects symlink
ancestors, including macOS `/tmp` and `/var` aliases.

For a persistent local app, create an owned directory first, set absolute
`SQLITE_PATH` and a non-symlink `STORAGE_ROOT`, and configure `NOTES_LOGIN_KEYS` as
JSON containing unique `{ "subjectId": "...", "key": "<64 hex characters>" }`
records. Keep those keys in the environment, never source or command arguments.
Then run:

```sh
bun run --cwd examples/notes files:migrate
bun run --cwd examples/notes recipes:serve
```

`PORT` defaults to 3000; `0` requests an assigned port. To use Engine's existing
supervisor instead, from the repository root:

```sh
lenso dev --root examples/notes \
  --config recipes.config.ts --entry src/recipes/server.ts
```

The server reports Engine readiness and closes its own listener on SIGINT/SIGTERM.
It binds only to loopback and checks browser origins. The demo's key login is the
existing Notes authentication flow, not password signup, OAuth or a user directory.

## Reuse existing instances

[`createRecipeApplication`](src/recipes/application.ts) is the standalone owner:
it creates exactly one database and one Auth. In an existing application, use the
smaller factories with your existing plugin objects instead:

```ts
import { createPrivateNotesFiles } from "./src/private-files";
import { createRecipeWeb } from "./src/recipes/web";
import { createSqliteFileQueries } from "@lenso/storage/sqlite";

const files = createPrivateNotesFiles({
  database, // existing Plugin<BunSQLiteDatabase<...>>
  authentication, // existing Plugin<NotesAuthentication>
  storages: [privateStorage], // existing Plugin<ObjectStorage>
  queries: createSqliteFileQueries,
});
const web = createRecipeWeb({
  authentication,
  files: { plugin: files, storageId: privateStorage.id },
});
// Add files and web to the host's plugins. Keep each existing instance once.
```

The same plugin ID is not a replacement for the same object. Core rejects missing
instance dependencies and duplicate IDs. Factories borrow resources; their existing
owners retain cleanup. This Notes adapter expects the Notes realm/audiences and
`issue/renew/revoke` service. Adapting a different Auth requires an explicit host
adapter; it must not manufacture actors or install a second verifier silently.

To keep only login, call `createRecipeWeb({ authentication })`. To keep only files,
set `login: false` and supply `files`; authenticate through the host's existing
entry. To keep services with no HTTP entry, omit the Web plugin and listener.
The executable tests exercise these choices.

| Entry                            | Behaviour                                                                       |
| -------------------------------- | ------------------------------------------------------------------------------- |
| `POST /session`                  | `{ key }` JSON, at most 4096 bytes; returns a managed bearer session, no cookie |
| `POST /session/renew`            | Bearer session; rotates when due, otherwise 409                                 |
| `POST /session/revoke`           | Bearer session; revokes it                                                      |
| `PUT /files`                     | Bearer session, raw body; optional `X-File-Name` and `Content-Type`             |
| `GET/HEAD/DELETE /files/:fileId` | Bearer session, owner and fixed tenant checked for every action                 |

Uploads use a server-selected storage ID, tenant and 1 MiB limit. Owner identity
comes from Auth, never request JSON. File IDs do not grant access. Responses use
private/no-store; downloads are attachments. This recipe does not offer signed
public downloads, multipart uploads or a browser login UI.

## Migration and storage choices

The standalone SQLite fixture explicitly uses `migrateFiles`: existing Notes
migrations include `auth_sessions`; the Storage migration creates `lenso_files`.
It also creates the Notes table, although these two recipes do not use it. Existing
apps should review and apply the Auth and Storage SQL through their existing
migration owner, not run the Notes migration over an unrelated database. Migration
SQL lives under `packages/auth/migrations` and `packages/storage/migrations`; these
are repository paths, not exported package subpaths. Setup never applies SQL.
The runtime needs read/write access to that database and its selected storage root.

[`createPrivateNotesFiles`](src/private-files.ts) is platform-neutral. Replace the
storage plugin with another `Plugin<ObjectStorage>` and choose matching file queries
for your database. For native R2 in a Workers host:

```ts
import { createR2StoragePlugin } from "@lenso/storage/r2";
const privateStorage = createR2StoragePlugin({ id: "privateFiles", binding: env.FILES });
// Pass this exact plugin into createPrivateNotesFiles and the host plugin list.
```

The host must already own the R2 binding and a compatible database/session store
(e.g. D1 with the existing SQLite file queries). The Bun/SQLite host in
`application.ts` is not a Workers application. Native R2 requires Workers'
`FixedLengthStream` and a known upload size; the HTTP adapter forwards a numeric
`Content-Length` when present. Unknown sizes and native R2 in Bun are rejected.
`@lenso/storage/s3` is the separate S3-compatible adapter for a Bun host; configure
it through the host's existing credential owner. No cloud bucket, permission or
credential is provisioned here. Local workerd verifies the R2 provider separately;
the complete Auth + D1 + R2 deployment is not claimed as tested.

## Assembly diagnosis

`recipes:inspect` projects the existing public `inspect` result. It names each
selected plugin, exact declared dependencies, HTTP adapter, provider, path and
source file. `entry-not-declared` means this selected config does not declare that
recipe's entry; it does not guess routes by running setup. Missing instances use
Core's existing `missing-dependency` diagnostic. CLI methods come only from named
`operations`; there are none in the Notes recipe config. Console/MCP exposure
remains a separate explicit host decision.

Inspection opens no database, evaluates no Config source and starts no listener.
It imports trusted configuration, so it is not a sandbox. `declared` does not mean
listening, migrated, healthy or authorized. Runtime configuration errors identify
the plugin/source/field using existing safe diagnostics. A 401 means missing,
invalid or revoked evidence; 403 means the authenticated user does not own the
file; 413 means the upload limit; 409 on early renewal is expected. An unavailable
session/file service may indicate an unapplied migration or unreachable resource;
run the explicit migration and check local resource ownership. Raw driver errors
and credentials are deliberately absent from HTTP output.

See [verification evidence](RECIPE_VERIFICATION.md) and the separate
[durable background-task recipe](../tasks/RECIPE.md).
