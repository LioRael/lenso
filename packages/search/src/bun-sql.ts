import type { SQL } from "bun";
import type { SearchDatabase } from "./postgres";
import { SearchError } from "./errors";

/** Reuse @lenso/db/bun-sql's Drizzle db.$client; this adapter never closes it. */
export function bunSqlSearchDatabase(client: SQL): SearchDatabase {
  if (client.options.adapter !== "postgres") throw new SearchError("invalid-input");
  return {
    async execute(text, parameters) {
      return await client.unsafe(text, [...parameters]);
    },
  };
}
