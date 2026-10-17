import { expect, test } from "bun:test";
import { createCache } from "../src/index";
import { createMemoryCacheAdapter } from "../src/memory";
import { catalogQueries, type Product } from "../examples/catalog";

test("API queries coalesce, retry HTTP failures, and reload after a successful update", async () => {
  const cache = createCache<Product>({ namespace: "catalog", adapter: createMemoryCacheAdapter() });
  let calls = 0;
  let title = "Original";
  const request = (async (input: URL | RequestInfo) => {
    expect(new URL(String(input)).pathname).toBe("/products/42");
    calls++;
    return calls === 1
      ? new Response("Unavailable", { status: 503 })
      : Response.json({ id: 42, title });
  }) as typeof fetch;
  const catalog = catalogQueries(cache, "https://catalog.example/", request);
  const failed = await Promise.allSettled([catalog.product(42), catalog.product(42)]);
  expect(failed.map((result) => result.status)).toEqual(["rejected", "rejected"]);
  expect(calls).toBe(1);
  expect(await Promise.all([catalog.product(42), catalog.product(42)])).toEqual([
    { id: 42, title },
    { id: 42, title },
  ]);
  expect(calls).toBe(2);
  await catalog.product(42);
  expect(calls).toBe(2);
  title = "Updated";
  await catalog.changed(42);
  expect(await catalog.product(42)).toEqual({ id: 42, title: "Updated" });
  expect(calls).toBe(3);
  cache.close();
});
