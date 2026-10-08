import { and, eq, sql } from "drizzle-orm";
import type { DrizzleD1Database } from "drizzle-orm/d1";
import type { OrganizationStore } from "../contracts";
import { assertValidVersion, decodeSnapshot, encodeState, validateState } from "./shared";
import { organizations } from "./schema-sqlite";

export function d1OrganizationStore<
  TSchema extends Record<string, unknown> = Record<string, unknown>,
>(db: DrizzleD1Database<TSchema>): OrganizationStore {
  return {
    async create(state) {
      await db
        .insert(organizations)
        .values({
          id: state.organization.id,
          version: state.organization.version,
          state: encodeState(state),
        })
        .run();
    },
    async read(id) {
      const [row] = await db
        .select({
          id: organizations.id,
          version: organizations.version,
          state: organizations.state,
          observedAt: sql<number>`CAST(unixepoch('subsec') * 1000 AS INTEGER)`,
        })
        .from(organizations)
        .where(eq(organizations.id, id))
        .limit(1)
        .all();
      return row ? decodeSnapshot(row.id, row.version, row.state, Number(row.observedAt)) : null;
    },
    async compareAndSwap(expectedVersion, next, validUntil) {
      assertValidVersion(expectedVersion, "expected version");
      validateState(next);
      if (next.organization.version !== expectedVersion + 1)
        throw new Error("Invalid organization version transition");
      if (validUntil !== undefined && !Number.isSafeInteger(validUntil))
        throw new Error("Invalid organization validity deadline");
      const rows = await db
        .update(organizations)
        .set({ version: next.organization.version, state: encodeState(next) })
        .where(
          and(
            eq(organizations.id, next.organization.id),
            eq(organizations.version, expectedVersion),
            validUntil === undefined
              ? sql`1 = 1`
              : sql`CAST(unixepoch('subsec') * 1000 AS INTEGER) < ${validUntil}`,
          )!,
        )
        .returning({ id: organizations.id })
        .all();
      return rows.length > 0;
    },
  };
}
