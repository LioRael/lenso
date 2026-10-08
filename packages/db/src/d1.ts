import type { D1Database } from "@cloudflare/workers-types";
import { drizzle, type DrizzleD1Database } from "drizzle-orm/d1";
import type { Plugin } from "lenso/plugin";
import { createDrizzlePlugin } from "./index";

/** D1 is a platform binding, never an owned connection or pool. */
export function createD1Plugin<TSchema extends Record<string, unknown>>(options: {
  id: string;
  binding: D1Database;
  schema: TSchema;
}): Plugin<DrizzleD1Database<TSchema>> {
  return createDrizzlePlugin({
    id: options.id,
    connect: () => drizzle(options.binding, { schema: options.schema }),
  });
}
