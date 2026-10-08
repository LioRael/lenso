import { and, sql, type SQL, type SQLWrapper } from "drizzle-orm";
import type { ApiKeyStore, KeyRecord, KeyRotation, KeySubject } from "../store";

type Columns = { [K in keyof ReturnType<typeof values>]: SQLWrapper };

export function values(record: KeyRecord) {
  return {
    id: record.id,
    namespace: record.subject.namespace,
    tenantId: record.subject.tenantId,
    subjectId: record.subject.subjectId,
    requestId: record.requestId,
    digest: record.digest,
    previousDigest: record.previousDigest,
    scopes: [...record.scopes],
    revision: record.revision,
    issuedAt: record.issuedAt,
    expiresAt: record.expiresAt,
    revokedAt: record.revokedAt,
    overlapUntil: record.overlapUntil,
  };
}

export function decode(row: ReturnType<typeof values>): KeyRecord {
  const { namespace, tenantId, subjectId, ...record } = row;
  return { ...record, subject: { namespace, tenantId, subjectId } };
}

export function subjectPredicate(table: Columns, subject: KeySubject): SQL {
  return and(
    sql`${table.namespace} = ${subject.namespace}`,
    sql`${table.tenantId} = ${subject.tenantId}`,
    sql`${table.subjectId} = ${subject.subjectId}`,
  )!;
}

export function requestPredicate(table: Columns, record: KeyRecord): SQL {
  return and(
    sql`${table.namespace} = ${record.subject.namespace}`,
    sql`${table.tenantId} = ${record.subject.tenantId}`,
    sql`${table.requestId} = ${record.requestId}`,
  )!;
}

// Column order follows both api_keys schemas. INSERT SELECT checks store-time
// expiry in the same statement that claims the idempotency key.
export function insertionSelect(
  table: Columns & SQLWrapper,
  record: KeyRecord,
  clock: SQL,
  number: (n: number) => SQL,
  scopes: SQL,
): SQL {
  const row = values(record);
  return sql`SELECT
    ${row.id}, ${row.namespace}, ${row.tenantId}, ${row.subjectId},
    ${row.requestId}, ${row.digest}, ${row.previousDigest}, ${scopes},
    ${number(row.revision)}, ${number(row.issuedAt)}, ${number(row.expiresAt)},
    ${row.revokedAt}, ${row.overlapUntil}
    WHERE ${clock} < ${number(record.expiresAt)}
    AND NOT EXISTS (SELECT 1 FROM ${table} WHERE ${requestPredicate(table, record)})`;
}

export function rotationPredicate(
  table: Columns,
  input: KeyRotation,
  clock: SQL,
  number: (n: number) => SQL,
): SQL {
  if (
    !Number.isSafeInteger(input.overlapMs) ||
    input.overlapMs < 0 ||
    !Number.isSafeInteger(input.now)
  ) {
    throw new Error("Invalid key operation");
  }
  return and(
    subjectPredicate(table, input.subject),
    sql`${table.id} = ${input.id}`,
    sql`${table.revision} = ${number(input.expectedRevision)}`,
    sql`${table.revokedAt} IS NULL`,
    sql`${table.expiresAt} > ${clock}`,
    sql`(${table.overlapUntil} IS NULL OR ${table.overlapUntil} <= ${clock})`,
    sql`${table.digest} <> ${input.digest}`,
    sql`(${table.previousDigest} IS NULL OR ${table.previousDigest} <> ${input.digest})`,
  )!;
}

export function rotationValues(
  table: Columns,
  input: KeyRotation,
  clock: SQL,
  number: (n: number) => SQL,
  minimum: "min" | "LEAST",
) {
  return {
    digest: input.digest,
    previousDigest: input.overlapMs === 0 ? null : sql`${table.digest}`,
    revision: sql`${table.revision} + 1`,
    overlapUntil:
      input.overlapMs === 0
        ? null
        : sql`${sql.raw(minimum)}(${table.expiresAt}, ${clock} + ${number(input.overlapMs)})`,
  };
}

export function pageLimit(limit: number): number {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 100)
    throw new Error("Invalid key operation");
  return limit;
}

/** Driver failures can embed SQL parameters; never forward them or their causes. */
export function safeStore(store: ApiKeyStore): ApiKeyStore {
  async function safe<T>(operation: () => Promise<T>): Promise<T> {
    try {
      return await operation();
    } catch {
      throw new Error("Key storage operation failed");
    }
  }
  return {
    create: (record) => safe(() => store.create(record)),
    read: (id) => safe(() => store.read(id)),
    list: (subject, after, limit) => safe(() => store.list(subject, after, limit)),
    rotate: (input) => safe(() => store.rotate(input)),
    revoke: (subject, id, now) => safe(() => store.revoke(subject, id, now)),
  };
}
