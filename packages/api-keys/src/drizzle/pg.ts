import { and, asc, eq, gt, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import type { ApiKeyStore } from "../store";
import { apiKeys } from "./schema-pg";
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

const number = (n: number) => sql`${n}::bigint`;
const databaseClock = sql`floor(extract(epoch from clock_timestamp()) * 1000)::bigint`;

export function postgresApiKeyStore<
  TSchema extends Record<string, unknown> = Record<string, unknown>,
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
>(db: PgDatabase<TResult, TSchema>): ApiKeyStore {
  return safeStore({
    async create(record) {
      const predicate = requestPredicate(apiKeys, record);
      const [replay] = await db.select().from(apiKeys).where(predicate).limit(1);
      if (replay) return { created: false, record: decode(replay) };
      const [inserted] = await db
        .insert(apiKeys)
        .select(
          // Bun SQL otherwise JSON-encodes a serialized JSON parameter a second time.
          insertionSelect(
            apiKeys,
            record,
            databaseClock,
            number,
            sql`${JSON.stringify(record.scopes)}::text::jsonb`,
          ),
        )
        .onConflictDoNothing({ target: [apiKeys.namespace, apiKeys.tenantId, apiKeys.requestId] })
        .returning();
      if (inserted) return { created: true, record: decode(inserted) };
      const [existing] = await db.select().from(apiKeys).where(predicate).limit(1);
      if (!existing) throw new Error("Key creation refused");
      return { created: false, record: decode(existing) };
    },
    async read(id) {
      const [row] = await db.select().from(apiKeys).where(eq(apiKeys.id, id)).limit(1);
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
        .limit(pageLimit(limit));
      return rows.map(decode);
    },
    async rotate(input) {
      return db.transaction(async (tx) => {
        // Evaluate expiry in a new statement after the row lock, not before a wait.
        const locked = await tx
          .select({ id: apiKeys.id })
          .from(apiKeys)
          .where(and(subjectPredicate(apiKeys, input.subject), eq(apiKeys.id, input.id)))
          .for("update");
        if (!locked.length) return null;
        const clock = sql`GREATEST(${number(input.now)}, ${databaseClock})`;
        const [row] = await tx
          .update(apiKeys)
          .set(rotationValues(apiKeys, input, clock, number, "LEAST"))
          .where(rotationPredicate(apiKeys, input, clock, number))
          .returning();
        return row ? decode(row) : null;
      });
    },
    async revoke(subject, id, now) {
      const rows = await db
        .update(apiKeys)
        .set({
          revokedAt: sql`coalesce(${apiKeys.revokedAt}, GREATEST(${number(now)}, ${databaseClock}))`,
          previousDigest: null,
          overlapUntil: null,
        })
        .where(and(subjectPredicate(apiKeys, subject), eq(apiKeys.id, id)))
        .returning({ id: apiKeys.id });
      return rows.length > 0;
    },
  });
}
