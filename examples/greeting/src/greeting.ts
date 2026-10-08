import { definePlugin, type Plugin } from "@lenso/core/plugin";
import type { Cache } from "@lenso/cache";
import { greetingInput, GreetingInputError } from "./contracts";
export interface GreetingService {
  greet(input: { name: string }): Promise<{ message: string; count: number }>;
}

export function createGreetingService(cache?: Cache<string>): GreetingService {
  let count = 0;
  return {
    async greet({ name }) {
      const parsed = greetingInput.safeParse({ name });
      if (!parsed.success) throw new GreetingInputError();
      const trimmed = parsed.data.name;
      const format = async () => `Hello, ${trimmed}!`;
      // Cache only the pure message, never the per-call counter or input validation.
      const bytes = cache ? new TextEncoder().encode(JSON.stringify(trimmed)) : undefined;
      let message: string;
      if (cache && bytes && bytes.length <= 16_384) {
        const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
        const key = Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
        message = await cache.getOrSet(key, format);
      } else {
        message = await format();
      }
      return { message, count: ++count };
    },
  };
}

export function createGreetingPlugin(options: {
  id: string;
  cache?: Plugin<Cache<string>>;
}): Plugin<GreetingService> {
  return definePlugin({
    id: options.id,
    requires: options.cache ? [options.cache] : [],
    setup(context) {
      return createGreetingService(options.cache ? context.get(options.cache) : undefined);
    },
  });
}

export const greeting = createGreetingPlugin({ id: "greeting" });
