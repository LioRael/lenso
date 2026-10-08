import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { createSqliteLimitStore } from "../src/sqlite";

const file = process.argv[2]!;
const id = process.argv[3]!;
const client = new Database(file);
try {
  client.exec("PRAGMA busy_timeout=10000");
  const store = createSqliteLimitStore(drizzle(client));
  const go = new Promise<void>((resolve) => process.once("message", () => resolve()));
  process.send?.("ready");
  await go;
  const scope = { instance: "processes", tenant: "shared", key: "atomic" };
  let rate = 0;
  let quota = 0;
  let leases = 0;
  for (let i = 0; i < 100; i++) {
    if (
      (await store.consume("rate", { scope, capacity: 75, quantity: 1, periodMs: 31_622_400_000 }))
        .allowed
    )
      rate++;
    if (
      (await store.consume("quota", { scope, capacity: 50, quantity: 1, periodMs: 31_622_400_000 }))
        .allowed
    )
      quota++;
    if (
      (await store.acquire({ scope, capacity: 11, quantity: 2, ttlMs: 60_000 }, `${id}:${i}`))
        .allowed
    )
      leases++;
  }
  console.log(JSON.stringify({ rate, quota, leases }));
} finally {
  client.close();
  process.disconnect?.();
}
