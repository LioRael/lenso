import type { Cache } from "@lenso/cache";

export type Product = { id: number; title: string };

/** Public product projection; pass the cache from the existing Native/Workers assembly. */
export function catalogQueries(
  cache: Cache<Product>,
  baseURL: string,
  request: typeof fetch = fetch,
) {
  return {
    product(id: number, signal?: AbortSignal): Promise<Product> {
      if (!Number.isSafeInteger(id) || id <= 0) throw new Error("Invalid product id");
      return cache.getOrSet(
        `product:${id}`,
        async (loadSignal) => {
          const response = await request(new URL(`products/${id}`, baseURL), {
            signal: loadSignal,
          });
          // HTTP errors must reject: caching an error body would turn an outage into a hit.
          if (!response.ok) throw new Error(`Catalog HTTP ${response.status}`);
          const data: unknown = await response.json();
          if (
            !data ||
            typeof data !== "object" ||
            !("id" in data) ||
            data.id !== id ||
            !("title" in data) ||
            typeof data.title !== "string"
          )
            throw new Error("Invalid product");
          return { id, title: data.title };
        },
        { signal, ttlMs: 30_000 },
      );
    },
    // Invoke only after the authoritative API update succeeds.
    changed(id: number) {
      return cache.delete(`product:${id}`);
    },
  };
}
