import { Database } from "bun:sqlite";
import { drizzle, type BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { Plugin } from "@lenso/core/plugin";
import { createDrizzlePlugin } from "./index";

export type BunSqliteOptions<TSchema extends Record<string, unknown>> = {
  id: string;
  schema: TSchema;
} & (
  | { filename: string; options?: ConstructorParameters<typeof Database>[1]; client?: never }
  | { client: Database; filename?: never; options?: never }
);

export function createBunSqlitePlugin<TSchema extends Record<string, unknown>>(
  options: BunSqliteOptions<TSchema>,
): Plugin<BunSQLiteDatabase<TSchema>> {
  return createDrizzlePlugin({
    id: options.id,
    connect(context) {
      const client = options.client ?? new Database(options.filename, options.options);
      if (!options.client) context.onCleanup(() => client.close());
      return drizzle({ client, schema: options.schema });
    },
  });
}
