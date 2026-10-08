import { and, asc, eq, gt, sql } from "drizzle-orm";
import type { BunSQLiteDatabase } from "drizzle-orm/bun-sqlite";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { ApiKeyStore } from "../store";
import { apiKeys } from "./schema-sqlite";
import {
  decode,
  insertionSelect,
  pageLimit,
  requestPredicate,
  rotationPredicate,
  rotationValues,
  safeStore,
  subjectPredicate,
} from "./shared";

type Database<T extends Record<string, unknown>> = BunSQLiteDatabase<T> | DrizzleD1Database<T>;
const databaseClock = sql`CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4, 3) AS INTEGER)`;
const number = (n: number) => sql`${n}`;

export function sqliteStore<T extends Record<string, unknown>>(db: Database<T>): ApiKeyStore {
  return safeStore({
    async create(record) {
      const predicate = requestPredicate(apiKeys, record);
      const [replay] = await db.select().from(apiKeys).where(predicate).limit(1).all();
      if (replay) return { created: false, record: decode(replay) };
      const inserted = await db
        .insert(apiKeys)
        .select(
          insertionSelect(
            apiKeys,
            record,
            databaseClock,
            number,
            sql`${JSON.stringify(record.scopes)}`,
          ),
        )
        .onConflictDoNothing({ target: [apiKeys.namespace, apiKeys.tenantId, apiKeys.requestId] })
        .returning()
        .all();
      if (inserted[0]) return { created: true, record: decode(inserted[0]) };
      const [existing] = await db.select().from(apiKeys).where(predicate).limit(1).all();
      if (!existing) throw new Error("Key creation refused");
      return { created: false, record: decode(existing) };
    },
    async read(id) {
      const [row] = await db.select().from(apiKeys).where(eq(apiKeys.id, id)).limit(1).all();
      return row ? decode(row) : null;
    },
    async list(subject, after, limit) {
      const rows = await db
        .select()
        .from(apiKeys)
        .where(
          and(
            subjectPredicate(apiKeys, subject),
            after === null ? undefined : gt(apiKeys.id, after),
          ),
        )
        .orderBy(asc(apiKeys.id))
        .limit(pageLimit(limit))
        .all();
      return rows.map(decode);
    },
    async rotate(input) {
      const clock = sql`max(${input.now}, ${databaseClock})`;
      const [row] = await db
        .update(apiKeys)
        .set(rotationValues(apiKeys, input, clock, number, "min"))
        .where(rotationPredicate(apiKeys, input, clock, number))
        .returning()
        .all();
      return row ? decode(row) : null;
    },
    async revoke(subject, id, now) {
      const rows = await db
        .update(apiKeys)
        .set({
          revokedAt: sql`coalesce(${apiKeys.revokedAt}, max(${now}, ${databaseClock}))`,
          previousDigest: null,
          overlapUntil: null,
        })
        .where(and(subjectPredicate(apiKeys, subject), eq(apiKeys.id, id)))
        .returning()
        .all();
      return rows.length > 0;
    },
  });
}

export function sqliteApiKeyStore<T extends Record<string, unknown> = Record<string, unknown>>(
  db: BunSQLiteDatabase<T>,
): ApiKeyStore {
  return sqliteStore(db);
}
