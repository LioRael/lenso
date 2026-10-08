import { and, eq, sql } from "drizzle-orm";
import type { PgDatabase, PgQueryResultHKT } from "drizzle-orm/pg-core";
import type { OrganizationStore } from "../contracts";
import { decodeSnapshot, encodeState, validateState, assertValidVersion } from "./shared";
import { organizations } from "./schema-pg";

export function postgresOrganizationStore<
  TSchema extends Record<string, unknown> = Record<string, unknown>,
  TResult extends PgQueryResultHKT = PgQueryResultHKT,
>(db: PgDatabase<TResult, TSchema>): OrganizationStore {
  return {
    async create(state) {
      const encoded = encodeState(state);
      await db.insert(organizations).values({
        id: state.organization.id,
        version: state.organization.version,
        state: encoded,
      });
    },
    async read(id) {
      const [row] = await db
        .select({
          id: organizations.id,
          version: organizations.version,
          state: organizations.state,
          observedAt: sql<number>`floor(extract(epoch from clock_timestamp()) * 1000)::bigint`,
        })
        .from(organizations)
        .where(eq(organizations.id, id))
        .limit(1);
      return row ? decodeSnapshot(row.id, row.version, row.state, Number(row.observedAt)) : null;
    },
    async compareAndSwap(expectedVersion, next, validUntil) {
      assertValidVersion(expectedVersion, "expected version");
      validateState(next);
      if (next.organization.version !== expectedVersion + 1) {
        throw new Error("Invalid organization version transition");
      }
      const encoded = encodeState(next);
      return db.transaction(async (tx) => {
        const locked = await tx
          .select({ id: organizations.id })
          .from(organizations)
          .where(eq(organizations.id, next.organization.id))
          .for("update");
        if (!locked.length) return false;
        const conditions = [
          eq(organizations.id, next.organization.id),
          eq(organizations.version, expectedVersion),
        ];
        if (validUntil !== undefined) {
          if (!Number.isSafeInteger(validUntil))
            throw new Error("Invalid organization validity deadline");
          conditions.push(
            sql`floor(extract(epoch from clock_timestamp()) * 1000)::bigint < ${validUntil}::bigint`,
          );
        }
        const rows = await tx
          .update(organizations)
          .set({ version: next.organization.version, state: encoded })
          .where(and(...conditions)!)
          .returning({ id: organizations.id });
        return rows.length > 0;
      });
    },
  };
}
