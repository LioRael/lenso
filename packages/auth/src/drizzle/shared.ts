import { and, sql, type SQL, type SQLWrapper } from "drizzle-orm";
import type { SessionMutation, SessionRecord } from "../session-store";

type SessionColumns = { [K in keyof SessionRecord]: SQLWrapper };

/** One predicate shared by all drivers: narrowing must not revive a session. */
export function mutationPredicate(
  table: SessionColumns,
  mutation: SessionMutation,
  clock: SQL,
  number: (value: number) => SQL,
): SQL {
  const { next } = mutation;
  const conditions = [
    sql`${table.realmId} = ${next.realmId}`,
    sql`${table.id} = ${next.id}`,
    sql`${table.revision} = ${number(mutation.expectedRevision)}`,
    sql`${table.tokenDigest} = ${mutation.expectedDigest}`,
    sql`${table.revokedAt} IS NULL`,
    sql`${clock} < ${table.expiresAt}`,
    sql`${clock} < ${table.lastActiveAt} + ${table.idleTimeoutMs}`,
    sql`${clock} < ${number(next.expiresAt)}`,
    sql`${clock} < ${table.lastActiveAt} + ${number(next.idleTimeoutMs)}`,
    sql`${number(next.expiresAt)} <= ${table.expiresAt}`,
    sql`${number(next.idleTimeoutMs)} <= ${table.idleTimeoutMs}`,
    sql`${number(next.renewAfterMs)} >= ${table.renewAfterMs}`,
    sql`${number(next.revision)} = ${table.revision} + 1`,
    sql`${number(next.lastActiveAt)} >= ${table.lastActiveAt}`,
    sql`${number(next.lastActiveAt)} <= ${clock}`,
    sql`${number(next.renewedAt)} >= ${table.renewedAt}`,
    sql`${number(next.renewedAt)} <= ${clock}`,
  ];
  if (mutation.kind === "renew") {
    conditions.push(sql`${clock} >= ${table.renewedAt} + ${number(next.renewAfterMs)}`);
  } else {
    conditions.push(
      sql`${table.tokenDigest} = ${next.tokenDigest}`,
      sql`${table.renewedAt} = ${number(next.renewedAt)}`,
    );
  }
  return and(...conditions)!;
}

export function mutationValues(next: SessionRecord) {
  return {
    tokenDigest: next.tokenDigest,
    revision: next.revision,
    lastActiveAt: next.lastActiveAt,
    renewedAt: next.renewedAt,
    expiresAt: next.expiresAt,
    idleTimeoutMs: next.idleTimeoutMs,
    renewAfterMs: next.renewAfterMs,
  };
}

export function decodeRecord<S extends string>(
  row: Omit<SessionRecord, "kind"> & { kind: string },
): SessionRecord<S> {
  if (row.kind !== "user" && row.kind !== "guest" && row.kind !== "service") {
    throw new Error("Invalid persisted session kind");
  }
  // Subject branding belongs to the application; persistence stores only its string.
  return { ...row, kind: row.kind, subjectId: row.subjectId as S };
}
