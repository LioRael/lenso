import { Database, type SQLQueryBindings } from "bun:sqlite";
import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import type { D1Database, D1PreparedStatement, D1Result } from "@cloudflare/workers-types";
import { drizzle as sqliteDrizzle } from "drizzle-orm/bun-sqlite";
import { drizzle as d1Drizzle } from "drizzle-orm/d1";
import { drizzle as pgDrizzle } from "drizzle-orm/bun-sql";
import { sqliteTable, text } from "drizzle-orm/sqlite-core";
import { pgTable, text as pgText } from "drizzle-orm/pg-core";
import { sql } from "drizzle-orm";
import { sqliteSessionStore } from "../src/drizzle/sqlite";
import { d1SessionStore } from "../src/drizzle/d1";
import { postgresSessionStore } from "../src/drizzle/pg";
import { authSessions } from "../src/drizzle/schema-sqlite";
import { authSessions as pgSessions } from "../src/drizzle/schema-pg";
import { mutationPredicate, mutationValues } from "../src/drizzle/shared";
import type { SessionMutation, SessionRecord, SessionStore } from "../src/session-store";

const migration = await readFile(
  new URL("../migrations/sqlite/0000_auth_sessions.sql", import.meta.url),
  "utf8",
);
const appTable = sqliteTable("application_data", { value: text("value") });
const schema = { authSessions, appTable };
type SubjectId = string & { readonly subjectIdBrand: unique symbol };

/** Local public D1 API adapter, not a Cloudflare deployment/emulator test. */
class SQLiteD1Statement implements D1PreparedStatement {
  constructor(
    private readonly db: Database,
    private readonly query: string,
    private readonly params: SQLQueryBindings[] = [],
    private readonly executed: string[],
  ) {}

  bind(...values: unknown[]): D1PreparedStatement {
    const params = values.map((value): SQLQueryBindings => {
      if (value === null || typeof value === "string" || typeof value === "number") return value;
      throw new Error("Unsupported test D1 binding");
    });
    return new SQLiteD1Statement(this.db, this.query, params, this.executed);
  }

  async all<T = Record<string, unknown>>(): Promise<D1Result<T>> {
    this.executed.push(this.query);
    const results = this.db.query<T, SQLQueryBindings[]>(this.query).all(...this.params);
    return this.result(results);
  }

  async run<T = Record<string, unknown>>(): Promise<D1Result<T>> {
    this.executed.push(this.query);
    const change = this.db.query(this.query).run(...this.params);
    return this.result<T>([], change.changes, Number(change.lastInsertRowid));
  }

  first<T = unknown>(colName: string): Promise<T | null>;
  first<T = Record<string, unknown>>(): Promise<T | null>;
  async first<T>(colName?: string): Promise<T | null> {
    const { results } = await this.all<T>();
    if (colName === undefined) return results[0] ?? null;
    const { results: records } = await this.all<Record<string, T>>();
    return records[0]?.[colName] ?? null;
  }

  raw<T = unknown[]>(options: { columnNames: true }): Promise<[string[], ...T[]]>;
  raw<T = unknown[]>(options?: { columnNames?: false }): Promise<T[]>;
  async raw<T>(options?: { columnNames?: boolean }): Promise<T[] | [string[], ...T[]]> {
    this.executed.push(this.query);
    const statement = this.db.query(this.query);
    // D1's public raw<T> promises caller-selected row arrays.
    const rows = statement.values(...this.params).map((row) => row as T);
    return options?.columnNames ? [statement.columnNames, ...rows] : rows;
  }

  private result<T>(results: T[], changes = 0, lastRowId = 0): D1Result<T> {
    return {
      success: true,
      results,
      meta: {
        duration: 0,
        size_after: 0,
        rows_read: results.length,
        rows_written: changes,
        last_row_id: lastRowId,
        changed_db: changes > 0,
        changes,
      },
    };
  }
}

class SQLiteD1Binding implements D1Database {
  readonly executed: string[] = [];
  constructor(private readonly db: Database) {}
  prepare(query: string): D1PreparedStatement {
    return new SQLiteD1Statement(this.db, query, [], this.executed);
  }
  async batch<T>(statements: D1PreparedStatement[]): Promise<D1Result<T>[]> {
    return Promise.all(statements.map((statement) => statement.all<T>()));
  }
  async exec(query: string) {
    this.db.exec(query);
    return { count: 1, duration: 0 };
  }
  withSession(): never {
    throw new Error("D1 sessions are not exercised by this test");
  }
  async dump(): Promise<ArrayBuffer> {
    throw new Error("D1 dump is not exercised by this test");
  }
}

function record(overrides: Partial<SessionRecord> = {}): SessionRecord {
  const now = Date.now();
  return {
    id: "session",
    realmId: "realm",
    subjectId: "subject",
    kind: "user",
    tokenDigest: "digest",
    revision: 1,
    issuedAt: now - 60_000,
    expiresAt: now + 600_000,
    idleTimeoutMs: 300_000,
    renewAfterMs: 10_000,
    lastActiveAt: now - 1_000,
    renewedAt: now - 60_000,
    authenticatedAt: now - 60_000,
    assurance: ["password"],
    revokedAt: null,
    ...overrides,
  };
}

function mutation(
  old: SessionRecord,
  overrides: Partial<SessionRecord> = {},
  kind: "touch" | "renew" = "touch",
): SessionMutation {
  const now = Date.now();
  return {
    kind,
    expectedRevision: old.revision,
    expectedDigest: old.tokenDigest,
    now,
    next: {
      ...old,
      revision: old.revision + 1,
      lastActiveAt: now,
      ...(kind === "renew" ? { tokenDigest: `${old.tokenDigest}-rotated`, renewedAt: now } : {}),
      ...overrides,
    },
  };
}

for (const driver of ["Bun SQLite", "D1 local binding"] as const) {
  async function fixture(run: (store: SessionStore, executed: string[]) => Promise<void>) {
    const db = new Database(":memory:");
    db.exec(migration);
    const executed: string[] = [];
    const binding = new SQLiteD1Binding(db);
    const store =
      driver === "Bun SQLite"
        ? sqliteSessionStore(
            sqliteDrizzle(db, {
              schema,
              logger: {
                logQuery(query) {
                  executed.push(query);
                },
              },
            }),
          )
        : d1SessionStore(d1Drizzle(binding, { schema }));
    try {
      await run(store, driver === "Bun SQLite" ? executed : binding.executed);
    } finally {
      db.close();
    }
  }

  test(`${driver}: create/read, realms, uniqueness and revocation`, () =>
    fixture(async (store) => {
      const first = record();
      const second = record({
        realmId: "other",
        subjectId: "other-subject",
        tokenDigest: "other-digest",
        kind: "guest",
        authenticatedAt: null,
      });
      await store.create(first);
      await store.create(second);
      expect(await store.read(first.realmId, first.id)).toEqual(first);
      expect(await store.read(second.realmId, second.id)).toEqual(second);
      expect(await store.read("missing", first.id)).toBeNull();
      await expect(store.create(first)).rejects.toThrow();
      await expect(store.create(record({ id: "different" }))).rejects.toThrow();
      expect(await store.revoke("missing", first.id, Date.now())).toBe(false);
      const at = Date.now();
      expect(await store.revoke(first.realmId, first.id, at)).toBe(true);
      expect(await store.revoke(first.realmId, first.id, at + 1)).toBe(false);
      expect(await store.mutate(mutation(first))).toBe(false);
      expect(await store.read(first.realmId, first.id)).toEqual({ ...first, revokedAt: at });
      expect(await store.read(second.realmId, second.id)).toEqual(second);
    }));

  test(`${driver}: touch and renewal persist only session activity, one UPDATE per CAS`, () =>
    fixture(async (store, executed) => {
      const first = record();
      await store.create(first);
      const touch = mutation(first, {
        subjectId: "attacker",
        kind: "service",
        issuedAt: 0,
        authenticatedAt: 0,
        assurance: ["fabricated"],
        revokedAt: 0,
        expiresAt: first.expiresAt - 1_000,
        idleTimeoutMs: first.idleTimeoutMs - 1_000,
        renewAfterMs: first.renewAfterMs + 1_000,
      });
      executed.length = 0;
      expect(await store.mutate(touch)).toBe(true);
      expect(executed).toHaveLength(1);
      expect(executed[0]).toMatch(/^update /i);
      const touched = { ...first, ...mutationValues(touch.next) };
      expect(await store.read(first.realmId, first.id)).toEqual(touched);
      const renew = mutation(touched, {}, "renew");
      executed.length = 0;
      expect(await store.mutate(renew)).toBe(true);
      expect(executed).toHaveLength(1);
      expect(await store.read(first.realmId, first.id)).toEqual({
        ...touched,
        ...mutationValues(renew.next),
      });
      expect(await store.mutate(renew)).toBe(false);
      expect(await store.mutate(mutation(touched))).toBe(false);
    }));

  test(`${driver}: rejects stale CAS, widening, touch rotation and future activity`, () =>
    fixture(async (store) => {
      const first = record();
      await store.create(first);
      const valid = mutation(first);
      const cases: SessionMutation[] = [
        { ...valid, expectedRevision: 0 },
        { ...valid, expectedDigest: "stale" },
        mutation(first, { id: "missing" }),
        mutation(first, { realmId: "missing" }),
        mutation(first, { revision: first.revision }),
        mutation(first, { revision: first.revision + 2 }),
        mutation(first, { expiresAt: first.expiresAt + 1 }),
        mutation(first, { idleTimeoutMs: first.idleTimeoutMs + 1 }),
        mutation(first, { renewAfterMs: first.renewAfterMs - 1 }),
        mutation(first, { tokenDigest: "rotated-on-touch" }),
        mutation(first, { renewedAt: Date.now() }),
        mutation(first, { lastActiveAt: Date.now() + 600_000 }),
        mutation(first, { renewedAt: Date.now() + 600_000 }, "renew"),
      ];
      for (const attempt of cases) {
        expect(await store.mutate(attempt)).toBe(false);
        expect(await store.read(first.realmId, first.id)).toEqual(first);
      }
    }));

  test(`${driver}: stored and effective-next expiration use statement clock, never revive`, () =>
    fixture(async (store) => {
      const time = Date.now();
      const rows = [
        record({ id: "expired", tokenDigest: "a", expiresAt: time - 1 }),
        record({ id: "idle", tokenDigest: "b", lastActiveAt: time - 300_001 }),
        record({ id: "narrow-expiry", tokenDigest: "c" }),
        record({ id: "narrow-idle", tokenDigest: "d", lastActiveAt: time - 30_000 }),
      ];
      for (const old of rows) {
        await store.create(old);
        const attempt = mutation(
          old,
          old.id === "narrow-expiry"
            ? { expiresAt: time - 1 }
            : old.id === "narrow-idle"
              ? { idleTimeoutMs: 10_000 }
              : {},
        );
        // An earlier caller clock must not bypass database-time expiry.
        expect(await store.mutate({ ...attempt, now: time - 600_000 })).toBe(false);
        expect(await store.read(old.realmId, old.id)).toEqual(old);
      }
      const first = record({ id: "caller-clock", tokenDigest: "e" });
      await store.create(first);
      expect(await store.mutate({ ...mutation(first), now: first.expiresAt })).toBe(false);
    }));

  test(`${driver}: renewal observes next (maximum) interval`, () =>
    fixture(async (store) => {
      const first = record({ renewedAt: Date.now() - 5_000 });
      await store.create(first);
      expect(await store.mutate(mutation(first, {}, "renew"))).toBe(false);
      const second = record({
        id: "second",
        tokenDigest: "second-digest",
        renewedAt: Date.now() - 20_000,
      });
      await store.create(second);
      expect(await store.mutate(mutation(second, { renewAfterMs: 60_000 }, "renew"))).toBe(false);
      expect(await store.mutate(mutation(second, {}, "renew"))).toBe(true);
    }));

  test(`${driver}: concurrent CAS has one winner`, () =>
    fixture(async (store) => {
      const first = record();
      await store.create(first);
      const attempt = mutation(first, {}, "renew");
      const results = await Promise.all([
        store.mutate(attempt),
        store.mutate({ ...attempt, next: { ...attempt.next, tokenDigest: "competitor" } }),
      ]);
      expect(results.filter(Boolean)).toHaveLength(1);
      expect((await store.read(first.realmId, first.id))?.revision).toBe(2);
    }));
}

test("PG native schema input and generated CAS SQL keep bigint parameters typed", () => {
  const applicationData = pgTable("application_data", { value: pgText("value") });
  const db = pgDrizzle.mock({ schema: { pgSessions, applicationData } });
  const store: SessionStore = postgresSessionStore(db);
  const branded: SessionStore<SubjectId> = postgresSessionStore<SubjectId>(db);
  expect(store).toBeDefined();
  expect(branded).toBeDefined();
  const attempt = mutation(record(), {}, "renew");
  const number = (value: number) => sql`${value}::bigint`;
  const clock = sql`GREATEST(${number(attempt.now)}, floor(extract(epoch from clock_timestamp()) * 1000)::bigint)`;
  const query = db
    .update(pgSessions)
    .set(mutationValues(attempt.next))
    .where(mutationPredicate(pgSessions, attempt, clock, number))
    .returning()
    .toSQL();
  expect(query.sql).toMatch(/^update /i);
  expect(query.sql).toContain("clock_timestamp()");
  expect(query.sql).toContain('"last_active_at" + $');
  expect(query.sql).toContain('"renewed_at" + $');
  // Every numeric predicate parameter has an explicit cast, including standalone comparisons.
  const where = query.sql.slice(query.sql.indexOf(" where "));
  for (const match of where.matchAll(/\$(\d+)/g)) {
    const index = Number(match[1]) - 1;
    if (typeof query.params[index] === "number") {
      expect(where.slice(match.index! + match[0].length)).toMatch(/^::bigint/);
    }
  }
});

test("native SQLite and D1 schemas support branded subjects at persistence decoding", async () => {
  const db = new Database(":memory:");
  db.exec(migration);
  try {
    const sqlite: SessionStore<SubjectId> = sqliteSessionStore<SubjectId>(
      sqliteDrizzle(db, { schema }),
    );
    const d1: SessionStore<SubjectId> = d1SessionStore<SubjectId>(
      d1Drizzle(new SQLiteD1Binding(db), { schema }),
    );
    const original = { ...record(), subjectId: "subject" as SubjectId };
    await sqlite.create(original);
    expect(await d1.read(original.realmId, original.id)).toEqual(original);
  } finally {
    db.close();
  }
});

test("Auth SQLite migration creates an isolated session table with expected constraints", async () => {
  const db = new Database(":memory:");
  try {
    db.exec(migration);
    const tables = db
      .query<{ name: string }, []>("SELECT name FROM sqlite_master WHERE type = 'table'")
      .all();
    expect(tables.map(({ name }) => name)).toEqual(["auth_sessions"]);
    expect(() =>
      db
        .query(
          "INSERT INTO auth_sessions (id, realm_id, subject_id, kind, token_digest, revision, issued_at, expires_at, idle_timeout_ms, renew_after_ms, last_active_at, renewed_at, assurance) VALUES ('a','r','s','user','d',1,0,100,50,10,0,0,'[]')",
        )
        .run(),
    ).not.toThrow();
    expect(() =>
      db
        .query(
          "INSERT INTO auth_sessions (id, realm_id, subject_id, kind, token_digest, revision, issued_at, expires_at, idle_timeout_ms, renew_after_ms, last_active_at, renewed_at, assurance) VALUES ('b','r','s','user','d',1,0,100,50,10,0,0,'[]')",
        )
        .run(),
    ).toThrow();
  } finally {
    db.close();
  }
});
