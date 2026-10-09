import assert from "node:assert/strict";
import { SQL } from "bun";
import { defineApp, startApp } from "@lenso/core";
import { definePlugin } from "@lenso/core/plugin";
import { createSearchService, type SearchScope } from "@lenso/search";
import { createPostgresSearchProvider, postgresSearchMigration } from "@lenso/search/postgres";
import { bunSqlSearchDatabase } from "@lenso/search/bun-sql";
import { createPostgresSearchPlugin } from "@lenso/search/plugin";

const url = process.env.SEARCH_TEST_DATABASE_URL;
if (!url) throw new Error("Set SEARCH_TEST_DATABASE_URL to a disposable authorized PostgreSQL DB");
const client = new SQL({ url, adapter: "postgres" });
const table = `packed_search_${crypto.randomUUID().replaceAll("-", "").slice(0, 20)}`;
const secret = crypto.getRandomValues(new Uint8Array(32));
const scope: SearchScope = { namespace: "packed", tenantId: "tenant", ownerId: "owner" };
try {
  await client.unsafe(postgresSearchMigration(table)).simple();
  const database = bunSqlSearchDatabase(client);
  const service = createSearchService({
    provider: createPostgresSearchProvider({ database, table }),
    cursorSecret: secret,
  });
  await service.upsert(scope, {
    id: "note-1",
    type: "note",
    tenantId: "tenant",
    ownerId: "owner",
    title: "packedmarker",
    body: "real PostgreSQL public entry",
  });
  assert.equal((await service.query(scope, { text: "packedmarker", includeTotal: true })).total, 1);
  const db = definePlugin({ id: "borrowed-db", setup: () => database });
  const search = createPostgresSearchPlugin({
    id: "search",
    database: db,
    adapter: (value) => value,
    config: { table, language: "simple" },
    cursorSecret: secret,
  });
  const app = await startApp(defineApp({ plugins: [db, search] }));
  try {
    const page = await app.get(search).query(scope, { text: "packedmarker" });
    assert.equal(page.hits[0]?.id, "note-1");
    assert.equal(page.hits[0]?.type, "note");
  } finally {
    await app.stop();
  }
  await service.delete(scope, { id: "note-1", type: "note" });
  assert.equal((await service.query(scope, { text: "packedmarker", includeTotal: true })).total, 0);
  // Plugin shutdown must leave the host's borrowed client usable.
  await client.unsafe("SELECT 1");
  console.info(
    "Packed public entry consumer: four imports, types, PG CRUD and borrowed lifecycle passed",
  );
} finally {
  try {
    await client.unsafe(`DROP TABLE IF EXISTS "${table}"`);
  } finally {
    await client.close();
  }
}
