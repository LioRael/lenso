import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import { drizzle } from "drizzle-orm/d1";
import { d1OrganizationStore } from "../src/drizzle/d1";
import { backendContract } from "./backend-contract";

const require = createRequire(new URL("../../../examples/workers/package.json", import.meta.url));
const { Miniflare, convertV4MiniflareOptions } = await import(
  pathToFileURL(require.resolve("miniflare")).href
);
const runtime = new Miniflare({
  ...convertV4MiniflareOptions({
    modules: true,
    script: "export default { fetch() { return new Response('organization D1 fixture'); } };",
    compatibilityDate: "2026-10-06",
    d1Databases: { DB: crypto.randomUUID() },
    d1Persist: false,
  }),
  host: "127.0.0.1",
  port: 0,
  telemetry: { enabled: false },
});
try {
  const db = await runtime.getD1Database("DB");
  const script = await readFile(
    new URL("../migrations/sqlite/0000_organizations.sql", import.meta.url),
    "utf8",
  );
  const ddl = script.replace(/--[^\n]*/g, "");
  for (const statement of ddl
    .split(";")
    .map((sql) => sql.trim())
    .filter(Boolean)) {
    await db.prepare(statement).run();
  }
  const first = d1OrganizationStore(drizzle(db));
  const second = d1OrganizationStore(drizzle(await runtime.getD1Database("DB")));
  await backendContract([first, second], (id) =>
    db.prepare("SELECT * FROM organizations WHERE id = ?").bind(id).first(),
  );
  console.info("Real local Miniflare/workerd D1 organization contract passed");
} finally {
  await runtime.dispose();
}
