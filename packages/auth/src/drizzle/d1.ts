import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { SessionStore } from "../session-store";
import { sqliteStore } from "./sqlite";

export function d1SessionStore<
  S extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
>(db: DrizzleD1Database<TSchema>): SessionStore<S> {
  return sqliteStore<S, TSchema>(db);
}
