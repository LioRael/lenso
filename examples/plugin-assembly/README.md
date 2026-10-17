# Auth + local Storage + Cache

A finite Bun application using built public package exports, without an HTTP server,
database, credentials, Runtime, generated files or config schema. `createNotesApp`
declares plugins without opening storage or constructing Auth/cache services.
Each read authenticates and checks ownership before looking in the cache.
`authorized` verifies once at that operation boundary; it avoids the redundant
`required` then `enforce` verification pair.

From the repository root, after the normal workspace dependency installation:

```sh
bun run --filter @lenso/core build
bun run --filter @lenso/auth --filter @lenso/storage --filter @lenso/cache build
bun run --filter @lenso/example-plugin-assembly typecheck
bun run --filter @lenso/example-plugin-assembly test
bun run --filter @lenso/example-plugin-assembly demo
```

The demo uses a fresh, private temporary directory and an ephemeral local proof.
It prints `Hello from Auth + Storage + Cache`, stops all services, and deletes
only its own directory. The proof verifier is a local fixture, not a login system.
Replace it with a trusted Auth source before exposing an entry.

## Less assembly and lifetime glue

Previously, dependencies had to be registered again at the application root:

```ts
const app = defineApp({ plugins: [auth, storage, adapter, cache, notes] });
const running = await startApp(app);
try {
  return await running.get(notes).read(proof, "alice");
} finally {
  await running.stop(); // A cleanup rejection can replace the original work error.
}
```

Now the same exact dependency objects can be collected explicitly:

```ts
const app = defineApp({ plugins: collectPlugins([notes]) });
return withApp(app, (running) => running.get(notes).read(proof, "alice"));
```

`collectPlugins` includes only declared `requires` edges, deduplicates by object
identity, and validates IDs/cycles before setup. List standalone entry plugins
as additional roots. Distinct objects with the same ID still fail: reuse the
original dependency object or assign distinct IDs. `defineApp`/`startApp` retain
their strict behavior; collection is opt-in. `app.get(notes)` still infers the
ordinary service type without casts or a service registry.

`withApp` awaits callback work and cleanup. Startup uses existing rollback;
callback failures retain their original identity. If work and cleanup both fail,
`AggregateError.errors` contains the work error followed by the cleanup error.
Do not return services for later use or detach work beyond the callback. For a
long-running listener, keep explicit `startApp`/`stop` ownership and request drain.

## Defaults, recovery and platform limits

- Cache needs only a namespace: the existing defaults give a 60-second TTL,
  bounded memory and fail-closed behavior. Cache results never authorize access.
- Local storage is create-only: a second put for the same owner conflicts. This
  example is a single immutable note, not an overwrite API. Failed auth leaves
  storage untouched; correcting evidence can retry the same service call.
- Restart with a fresh `createNotesApp` assembly after startup/work failure. The
  test verifies persistent note reads after a failed lifetime, revoked access on
  cache hits, and closed cache behavior after cleanup.
- Local storage refuses symlink ancestors. The demo resolves the OS temporary
  directory before making its private child (needed on macOS `/var` aliases).
  Do not loosen provider path checks to make a development path work.
- This example is verified on local Bun. `storage/local` uses filesystem APIs
  and is not a Workers adapter. For Workers, select `storage/r2` and supply the
  actual binding; that deployment path is not verified by this example. Memory
  cache is per-process and is neither durable nor shared across workers.
- Durable tasks require their actual database/queue provider; this example
  intentionally uses Cache rather than claiming an in-memory task substitute.
