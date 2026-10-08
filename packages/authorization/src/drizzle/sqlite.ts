import { and, eq } from "drizzle-orm";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { Evaluation, RoleSnapshot, RoleStore } from "../types";
import { authorizationRoleGraphs } from "./schema-sqlite";
import { createStore } from "./shared";

type SqliteDb<TSchema extends Record<string, unknown>> =
  | BunSQLiteDatabase<TSchema>
  | DrizzleD1Database<TSchema>;

export function sqliteStore<A extends string, TSchema extends Record<string, unknown>>(
  db: SqliteDb<TSchema>,
  namespace: string,
  actions: readonly A[],
): RoleStore<A> & { initialize(snapshot: RoleSnapshot<A>): Promise<void> } {
  return createStore(
    namespace,
    actions,
    async (_evaluation: Evaluation) => {
      const [row] = await db
        .select()
        .from(authorizationRoleGraphs)
        .where(eq(authorizationRoleGraphs.namespace, namespace))
        .limit(1)
        .all();
      return row;
    },
    async (snapshot) => {
      await db
        .insert(authorizationRoleGraphs)
        .values({
          namespace,
          revision: snapshot.revision,
          graph: snapshot.graph,
        })
        .onConflictDoNothing()
        .run();
    },
    async (expectedRevision, snapshot) => {
      const rows = await db
        .update(authorizationRoleGraphs)
        .set({
          revision: snapshot.revision,
          graph: snapshot.graph,
        })
        .where(
          and(
            eq(authorizationRoleGraphs.namespace, namespace),
            eq(authorizationRoleGraphs.revision, expectedRevision),
          ),
        )
        .returning()
        .all();
      return rows.length === 1;
    },
  );
}

export function sqliteRoleStore<
  A extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
>(
  db: BunSQLiteDatabase<TSchema>,
  namespace: string,
  actions: readonly A[],
): RoleStore<A> & { initialize(snapshot: RoleSnapshot<A>): Promise<void> } {
  return sqliteStore(db, namespace, actions);
}
