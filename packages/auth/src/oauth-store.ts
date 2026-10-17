/** OAuth-owned durable records. Adapters must provide atomic insert/CAS across workers. */
export interface OAuthRecord {
  kind: string;
  id: string;
  subjectId: string;
  expiresAt: number;
  revision: number;
  value: unknown;
}
export interface OAuthStore {
  insert(record: OAuthRecord): Promise<boolean>;
  /** Read current durable state, without a cache that can hide revocation or rotation. */
  read(kind: string, id: string): Promise<OAuthRecord | null>;
  replace(record: OAuthRecord, expectedRevision: number): Promise<boolean>;
  list(kind: string, subjectId: string): Promise<readonly OAuthRecord[]>;
  /** Optional host-invoked bounded cleanup; never starts a timer or acquires a connection. */
  pruneExpired?(before: number, limit: number): Promise<number>;
}
export interface OAuthSqlDriver {
  /** Return rows for SELECT and mutations with RETURNING. Values must be bound, never interpolated. */
  query(sql: string, values: readonly unknown[]): Promise<readonly Record<string, unknown>[]>;
}
/** Borrowed driver; application selects namespace and owns connection lifetime/migration. */
export function createSqlOAuthStore(
  driver: OAuthSqlDriver,
  namespace: string,
  dialect: "postgres" | "sqlite",
): OAuthStore {
  if (!/^[A-Za-z0-9_.:-]{1,128}$/.test(namespace)) throw new TypeError("Invalid OAuth namespace");
  const query = (sql: string, values: readonly unknown[]) => {
    let index = 0;
    return driver.query(
      sql.replace(/\?/g, () => (dialect === "postgres" ? `$${++index}` : "?")),
      values,
    );
  };
  function decode(row: Record<string, unknown>): OAuthRecord {
    return {
      kind: String(row.kind),
      id: String(row.id),
      subjectId: String(row.subject_id),
      expiresAt: Number(row.expires_at),
      revision: Number(row.revision),
      value: JSON.parse(String(row.document)),
    };
  }
  return {
    async insert(r) {
      return (
        (
          await query(
            "INSERT INTO auth_oauth_records(namespace,kind,id,subject_id,expires_at,revision,document) VALUES(?,?,?,?,?,?,?) ON CONFLICT(namespace,kind,id) DO NOTHING RETURNING id",
            [
              namespace,
              r.kind,
              r.id,
              r.subjectId,
              r.expiresAt,
              r.revision,
              JSON.stringify(r.value),
            ],
          )
        ).length === 1
      );
    },
    async read(kind, id) {
      const rows = await query(
        "SELECT * FROM auth_oauth_records WHERE namespace=? AND kind=? AND id=?",
        [namespace, kind, id],
      );
      return rows[0] ? decode(rows[0]) : null;
    },
    async replace(r, expected) {
      return (
        (
          await query(
            "UPDATE auth_oauth_records SET subject_id=?,expires_at=?,revision=?,document=? WHERE namespace=? AND kind=? AND id=? AND revision=? RETURNING id",
            [
              r.subjectId,
              r.expiresAt,
              expected + 1,
              JSON.stringify(r.value),
              namespace,
              r.kind,
              r.id,
              expected,
            ],
          )
        ).length === 1
      );
    },
    async pruneExpired(before, limit) {
      if (
        !Number.isSafeInteger(before) ||
        before < 0 ||
        !Number.isSafeInteger(limit) ||
        limit < 1 ||
        limit > 256
      )
        throw new TypeError("Invalid OAuth cleanup budget");
      return (
        await query(
          "DELETE FROM auth_oauth_records WHERE namespace=? AND (kind,id) IN (SELECT kind,id FROM auth_oauth_records WHERE namespace=? AND expires_at<=? ORDER BY expires_at LIMIT ?) RETURNING id",
          [namespace, namespace, before, limit],
        )
      ).length;
    },
    async list(kind, subject) {
      return (
        await query(
          "SELECT * FROM auth_oauth_records WHERE namespace=? AND kind=? AND subject_id=? ORDER BY id LIMIT 1000",
          [namespace, kind, subject],
        )
      ).map(decode);
    },
  };
}
