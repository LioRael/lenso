import { and, eq, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import type { Evaluation, RoleSnapshot, RoleStore } from "../types";
import { createStore } from "./shared";
import { authorizationRoleGraphs } from "./schema-pg";

export function postgresRoleStore<
  A extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
>(
  db: PgDatabase<TResult, TSchema>,
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
        .limit(1);
      return row;
    },
    async (snapshot) => {
      await db
        .insert(authorizationRoleGraphs)
        .values({
          namespace,
          revision: snapshot.revision,
          // Prevent Bun SQL from serializing Drizzle's JSON value a second time.
          graph: sql`${JSON.stringify(snapshot.graph)}::text::jsonb`,
        })
        .onConflictDoNothing();
    },
    async (expectedRevision, snapshot) => {
      const rows = await db
        .update(authorizationRoleGraphs)
        .set({
          revision: snapshot.revision,
          graph: sql`${JSON.stringify(snapshot.graph)}::text::jsonb`,
        })
        .where(
          and(
            eq(authorizationRoleGraphs.namespace, namespace),
            eq(authorizationRoleGraphs.revision, expectedRevision),
          ),
        )
        .returning({
          namespace: authorizationRoleGraphs.namespace,
        });
      return rows.length === 1;
    },
  );
}
