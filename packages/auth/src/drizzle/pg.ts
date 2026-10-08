import { and, eq, isNull, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import type { SessionStore } from "../session-store";
import { authSessions } from "./schema-pg";
import { decodeRecord, mutationPredicate, mutationValues } from "./shared";

export function postgresSessionStore<
  S extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
>(db: PgDatabase<TResult, TSchema>): SessionStore<S> {
  return {
    async create(record) {
      await db.insert(authSessions).values({
        ...record,
        // Bind as text so Bun SQL does not JSON-encode Drizzle's serialized JSON a second time.
        assurance: sql`${JSON.stringify(record.assurance)}::text::jsonb`,
      });
    },
    async read(realmId, id) {
      const [row] = await db
        .select()
        .from(authSessions)
        .where(and(eq(authSessions.realmId, realmId), eq(authSessions.id, id)))
        .limit(1);
      return row ? decodeRecord<S>(row) : null;
    },
    async mutate(mutation) {
      // Explicit bigint casts avoid unknown/narrow parameter inference in raw arithmetic.
      const number = (value: number) => sql`${value}::bigint`;
      const clock = sql`GREATEST(${number(mutation.now)}, floor(extract(epoch from clock_timestamp()) * 1000)::bigint)`;
      return db.transaction(async (tx) => {
        // UPDATE can evaluate its WHERE before an unchanged-row lock wait; lock first,
        // then evaluate expiration in a fresh statement while retaining ownership.
        const locked = await tx
          .select({ id: authSessions.id })
          .from(authSessions)
          .where(
            and(
              eq(authSessions.realmId, mutation.next.realmId),
              eq(authSessions.id, mutation.next.id),
            ),
          )
          .for("update");
        if (locked.length === 0) return false;
        const rows = await tx
          .update(authSessions)
          .set(mutationValues(mutation.next))
          .where(mutationPredicate(authSessions, mutation, clock, number))
          .returning();
        return rows.length > 0;
      });
    },
    async revoke(realmId, id, at) {
      const rows = await db
        .update(authSessions)
        .set({ revokedAt: at })
        .where(
          and(
            eq(authSessions.realmId, realmId),
            eq(authSessions.id, id),
            isNull(authSessions.revokedAt),
          ),
        )
        .returning();
      return rows.length > 0;
    },
  };
}
