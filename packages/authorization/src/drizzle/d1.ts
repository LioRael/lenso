import type { DrizzleD1Database } from "drizzle-orm/d1";
import { sqliteStore } from "./sqlite";

export function d1RoleStore<
  A extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
>(db: DrizzleD1Database<TSchema>, namespace: string, actions: readonly A[]) {
  return sqliteStore(db, namespace, actions);
}
