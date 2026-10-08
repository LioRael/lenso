import { test } from "bun:test";
import type { D1Database } from "@cloudflare/workers-types";
import { readFile } from "node:fs/promises";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { d1ApiKeyStore } from "../src/drizzle/d1";
import { storeContract } from "./drizzle.test";

test("actual local workerd D1: authoritative API key store contract", async () => {
  const mf = new Miniflare({
    ...convertV4MiniflareOptions({
      modules: true,
      script: "export default { fetch() { return new Response('local API key test'); } };",
      compatibilityDate: "2026-10-06",
      d1Databases: { DB: crypto.randomUUID() },
      d1Persist: false,
    }),
    host: "127.0.0.1",
    port: 0,
    telemetry: { enabled: false },
  });
  try {
    const binding = await mf.getD1Database("DB");
    const migration = await readFile(
      new URL("../migrations/sqlite/0000_api_keys.sql", import.meta.url),
      "utf8",
    );
    for (const statement of migration
      .split(";")
      .map((sql) => sql.trim())
      .filter(Boolean)) {
      await binding.prepare(statement).run();
    }
    // Miniflare's proxy executes real workerd D1 queries, not a mock binding.
    await storeContract(
      d1ApiKeyStore(binding as unknown as D1Database),
      d1ApiKeyStore(binding as unknown as D1Database),
    );
  } finally {
    await mf.dispose();
  }
}, 30_000);
