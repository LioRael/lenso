import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";
import { definePlugin } from "@lenso/core";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { createNotesApplication } from "../../src/application";
import { startNotesServer } from "../../src/server";
import { createSqliteNotesQueries } from "../../src/queries-sqlite";
import * as schema from "../../src/schema-sqlite";

const sqlite = createBunSqlitePlugin({
  id: "notes-db",
  filename: process.argv[2]!,
  schema,
});
const database = definePlugin({
  id: sqlite.id,
  async setup(context) {
    process.send?.({ type: "database-started" });
    context.onCleanup(() => {
      return new Promise<void>((resolve) => {
        if (!process.send) return resolve();
        process.send({ type: "database-closed" }, () => resolve());
      });
    });
    const resource = await sqlite.setup(context);
    if (process.argv[3] === "setup-failure") throw new Error("Expected setup failure");
    return resource;
  },
});
const application = createNotesApplication({
  database,
  store: sqliteSessionStore,
  queries: createSqliteNotesQueries,
  principals: () =>
    process.argv[3] === "invalid" ? [] : [{ subjectId: "A", key: "a".repeat(64) }],
});
const server = await startNotesServer(application, 0);
reportDevReady({
  urls: [server.url],
  capabilities: server.app.status().map((plugin) => plugin.id),
});
process.once("SIGTERM", () => {
  void server.app.stop().then(
    () => process.exit(0),
    () => process.exit(1),
  );
});
