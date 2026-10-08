import {
  createApiKeys,
  sameKeySubject,
  type ApiKeyOptions,
  type ApiKeyStore,
  type KeyRecord,
} from "../src";

/** Deterministic store for core tests only; real adapter tests use disposable databases. */
export function fixture<C = object, R = { tenantId: string }>(
  overrides: Partial<ApiKeyOptions<C, R>> = {},
) {
  let now = Date.now();
  const rows = new Map<string, KeyRecord>();
  const store: ApiKeyStore = {
    async create(record) {
      const prior = [...rows.values()].find(
        (row) =>
          row.subject.namespace === record.subject.namespace &&
          row.subject.tenantId === record.subject.tenantId &&
          row.requestId === record.requestId,
      );
      if (prior) return { created: false, record: prior };
      rows.set(record.id, record);
      return { created: true, record };
    },
    async read(id) {
      return rows.get(id) ?? null;
    },
    async list(subject, after, limit) {
      return [...rows.values()]
        .filter((row) => sameKeySubject(row.subject, subject) && (after === null || row.id > after))
        .sort((a, b) => a.id.localeCompare(b.id))
        .slice(0, limit);
    },
    async rotate(input) {
      const row = rows.get(input.id);
      if (
        !row ||
        !sameKeySubject(row.subject, input.subject) ||
        row.revision !== input.expectedRevision ||
        row.revokedAt !== null ||
        row.expiresAt <= now ||
        (row.overlapUntil ?? 0) > now
      )
        return null;
      const next = {
        ...row,
        digest: input.digest,
        revision: row.revision + 1,
        previousDigest: input.overlapMs ? row.digest : null,
        overlapUntil: input.overlapMs ? Math.min(row.expiresAt, now + input.overlapMs) : null,
      };
      rows.set(row.id, next);
      return next;
    },
    async revoke(subject, id, at) {
      const row = rows.get(id);
      if (!row || !sameKeySubject(row.subject, subject)) return false;
      rows.set(id, { ...row, revokedAt: row.revokedAt ?? at });
      return true;
    },
  };
  const keys = createApiKeys<C, R>({
    store,
    config: { maxLifetimeMs: 60_000, maxOverlapMs: 5_000 },
    authorizeManagement: () => true,
    grantScopes: (_caller, _subject, requested) =>
      requested.filter((scope) => scope === "notes:read"),
    subjectActive: () => true,
    authorizeUse: () => true,
    now: () => now,
    ...overrides,
  });
  const subject = { namespace: "accounts", tenantId: "tenant-a", subjectId: "same-id" };
  const input = {
    subject,
    requestedScopes: ["notes:read"],
    expiresAt: now + 30_000,
    requestId: "test-request",
  };
  return {
    keys,
    rows,
    store,
    subject,
    input,
    advance: (ms: number) => {
      now += ms;
    },
  };
}
