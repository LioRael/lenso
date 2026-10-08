import { SQL } from "bun";
import { drizzle, type BunSQLDatabase } from "drizzle-orm/bun-sql";
import type { Plugin } from "lenso/plugin";
import { createDrizzlePlugin } from "./index";

export type BunPostgresConnection = Omit<SQL.PostgresOrMySQLOptions, "adapter"> & {
  adapter?: "postgres";
};

export type BunSqlOptions<TSchema extends Record<string, unknown>> = {
  id: string;
  schema: TSchema;
} & (
  | { connection: string | BunPostgresConnection; client?: never }
  | { client: SQL; connection?: never }
);

/** A supplied SQL client belongs to its caller; only a new pool is closed. */
export function createBunSqlPlugin<TSchema extends Record<string, unknown>>(
  options: BunSqlOptions<TSchema>,
): Plugin<BunSQLDatabase<TSchema>> {
  return createDrizzlePlugin({
    id: options.id,
    connect(context) {
      const client =
        options.client ??
        new SQL(
          typeof options.connection === "string"
            ? { url: options.connection, adapter: "postgres" }
            : { ...options.connection, adapter: "postgres" },
        );
      if (!options.client) context.onCleanup(() => client.close());
      if (client.options.adapter !== "postgres") {
        throw new Error("The Bun SQL Drizzle resource requires a PostgreSQL client");
      }
      return drizzle({ client, schema: options.schema });
    },
  });
}
