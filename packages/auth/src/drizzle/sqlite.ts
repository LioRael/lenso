import { and, desc, eq, isNull, lt, or, sql } from "drizzle-orm";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { SessionAdminStore } from "../session-store";
import { authSessions } from "./schema-sqlite";
import {
  checkPage,
  checkRevokeRevision,
  decodeRecord,
  mutationPredicate,
  mutationValues,
} from "./shared";

type SqliteDb<TSchema extends Record<string, unknown>> =
  | BunSQLiteDatabase<TSchema>
  | DrizzleD1Database<TSchema>;

export function sqliteStore<S extends string, TSchema extends Record<string, unknown>>(
  db: SqliteDb<TSchema>,
): SessionAdminStore<S> {
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
                  and(eq(authSessions.issuedAt, before.issuedAt), lt(authSessions.id, before.id)),
                )
              : undefined,
          ),
        )
        .orderBy(desc(authSessions.issuedAt), desc(authSessions.id))
        .limit(limit)
        .all();
      return rows.map((row) => decodeRecord<S>(row));
    },
    async revokeRevision(realmId, id, expectedRevision, at) {
      checkRevokeRevision(expectedRevision, at);
      const rows = await db
        .update(authSessions)
        .set({
          revokedAt: sql`max(${at}, ${authSessions.issuedAt}, CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4, 3) AS INTEGER))`,
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
        .returning()
        .all();
      return rows.length > 0;
    },
    async create(record) {
      await db
        .insert(authSessions)
        .values({ ...record, assurance: [...record.assurance] })
        .run();
    },
    async read(realmId, id) {
      const [row] = await db
        .select()
        .from(authSessions)
        .where(and(eq(authSessions.realmId, realmId), eq(authSessions.id, id)))
        .limit(1)
        .all();
      return row ? decodeRecord<S>(row) : null;
    },
    async mutate(mutation) {
      const clock = sql`max(${mutation.now}, CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4, 3) AS INTEGER))`;
      const rows = await db
        .update(authSessions)
        .set(mutationValues(mutation.next))
        .where(mutationPredicate(authSessions, mutation, clock, (value) => sql`${value}`))
        .returning()
        .all();
      return rows.length > 0;
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
        .returning()
        .all();
      return rows.length > 0;
    },
  };
}

export function sqliteSessionStore<
  S extends string = string,
  TSchema extends Record<string, unknown> = Record<string, unknown>,
>(db: BunSQLiteDatabase<TSchema>): SessionAdminStore<S> {
  return sqliteStore<S, TSchema>(db);
}
