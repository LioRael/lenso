import { and, desc, eq, isNull, lt, or, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import type { SessionAdminStore } from "../session-store";
import { authSessions } from "./schema-pg";
import {
  checkPage,
  checkRevokeRevision,
  decodeRecord,
  mutationPredicate,
  mutationValues,
} from "./shared";

const bigintParameter = (value: number) => sql`${value}::bigint`;

export function postgresSessionStore<
  S extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
>(db: PgDatabase<TResult, TSchema>): SessionAdminStore<S> {
  return {
    async page(realmId, limit, before) {
      checkPage(limit, before);
      const rows = await db
        .select()
        .from(authSessions)
        .where(
          and(
            eq(authSessions.realmId, realmId),
            before
              ? or(
                  lt(authSessions.issuedAt, before.issuedAt),
                  and(
                    eq(authSessions.issuedAt, before.issuedAt),
                    sql`${authSessions.id} COLLATE "C" < ${before.id}`,
                  ),
                )
              : undefined,
          ),
        )
        .orderBy(desc(authSessions.issuedAt), desc(sql`${authSessions.id} COLLATE "C"`))
        .limit(limit);
      return rows.map((row) => decodeRecord<S>(row));
    },
    async revokeRevision(realmId, id, expectedRevision, at) {
      checkRevokeRevision(expectedRevision, at);
      const rows = await db
        .update(authSessions)
        .set({
          revokedAt: sql`GREATEST(${bigintParameter(at)}, ${authSessions.issuedAt}, floor(extract(epoch from clock_timestamp()) * 1000)::bigint)`,
          revision: sql`${authSessions.revision} + 1`,
        })
        .where(
          and(
            eq(authSessions.realmId, realmId),
            eq(authSessions.id, id),
            eq(authSessions.revision, expectedRevision),
            isNull(authSessions.revokedAt),
          ),
        )
        .returning();
      return rows.length > 0;
    },
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
      const clock = sql`GREATEST(${bigintParameter(mutation.now)}, floor(extract(epoch from clock_timestamp()) * 1000)::bigint)`;
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
          .where(mutationPredicate(authSessions, mutation, clock, bigintParameter))
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
