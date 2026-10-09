import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { SQL } from "bun";
import {
  createSearchService,
  SearchError,
  searchErrorDiagnostic,
  type SearchScope,
} from "../src/index";
import {
  createPostgresSearchProvider,
  postgresSearchMigration,
  type SearchDatabase,
} from "../src/postgres";
import { bunSqlSearchDatabase } from "../src/bun-sql";

const url = process.env.SEARCH_TEST_DATABASE_URL;
const suite = url ? describe : describe.skip;
suite("real PostgreSQL search (host-authorized disposable database)", () => {
  const table = `search_test_${crypto.randomUUID().replaceAll("-", "").slice(0, 20)}`;
  let client: SQL;
  let database: SearchDatabase;
  let captured: { text: string; parameters: readonly unknown[] } | undefined;
  let search: ReturnType<typeof createSearchService>;
  const a: SearchScope = { namespace: "notes", tenantId: "tenant-a", ownerId: "alice" };
  const b: SearchScope = { namespace: "notes", tenantId: "tenant-b", ownerId: "alice" };
  const c: SearchScope = { namespace: "notes", tenantId: "tenant-a", ownerId: "bob" };
  const sentinel = "SECRET_OTHER_PARTITION_778921";
  function note(id: string, scope = a, body = "orbit visible content", title = "orbit") {
    return {
      id,
      type: "note",
      tenantId: scope.tenantId,
      ownerId: scope.ownerId,
      title,
      body,
      metadata: { source: id },
    };
  }
  beforeAll(async () => {
    client = new SQL({ url: url!, adapter: "postgres", max: 3 });
    const adapter = bunSqlSearchDatabase(client);
    database = {
      async execute(text, parameters) {
        captured = { text, parameters };
        return adapter.execute(text, parameters);
      },
    };
    // The test alone owns this randomly named table and executes migrations explicitly.
    await client.unsafe(postgresSearchMigration(table)).simple();
    search = createSearchService({
      provider: createPostgresSearchProvider({ database, table }),
      cursorSecret: crypto.getRandomValues(new Uint8Array(32)),
    });
  });
  afterAll(async () => {
    if (client) {
      try {
        await client.unsafe(`DROP TABLE IF EXISTS "${table}"`);
      } finally {
        await client.close();
      }
    }
  });

  test("migration template agrees with reviewable default migration", async () => {
    const file = await Bun.file(new URL("../migrations/001_search.sql", import.meta.url)).text();
    expect(file.replace(/^--[^\n]*\n/, "").trim()).toBe(postgresSearchMigration().trim());
  });

  test("insert/update/delete are visible and repeated operations are idempotent", async () => {
    const doc = note("lifecycle", a, "nebula", "first");
    await search.upsert(a, doc);
    await search.upsert(a, doc);
    expect((await search.query(a, { text: "nebula", includeTotal: true })).total).toBe(1);
    await search.upsert(a, { ...doc, body: "supernova" });
    expect((await search.query(a, { text: "nebula" })).hits).toEqual([]);
    expect((await search.query(a, { text: "supernova" })).hits[0]?.id).toBe(doc.id);
    await search.delete(a, { id: doc.id, type: doc.type });
    await search.delete(a, { id: doc.id, type: doc.type });
    expect((await search.query(a, { text: "supernova" })).hits).toEqual([]);
  });

  test("same IDs in different tenant/owner/namespace partitions never mix", async () => {
    await search.upsert(a, note("same"));
    await search.upsert(b, note("same", b, `orbit ${sentinel}`));
    await search.upsert(c, note("same", c, `orbit ${sentinel}`));
    await search.upsert(
      { namespace: "public" },
      {
        id: "same",
        type: "note",
        title: "orbit",
        body: `orbit ${sentinel}`,
      },
    );
    const page = await search.query(a, { text: "orbit", includeTotal: true });
    expect(page.total).toBe(1);
    expect(page.hits[0]?.id).toBe("same");
    expect(JSON.stringify(page)).not.toContain(sentinel);
    expect((await search.query(a, { text: sentinel, includeTotal: true })).total).toBe(0);
    await search.delete(a, { id: "same", type: "note" });
    expect((await search.query(b, { text: sentinel })).hits).toHaveLength(1);
    expect((await search.query(c, { text: sentinel })).hits).toHaveLength(1);
    expect((await search.query(a, { text: "orbit", includeTotal: true })).total).toBe(0);
  });

  test("missing/conflicting/wildcard scope fails for every operation without disclosure", async () => {
    for (const run of [
      () => search.query(undefined as unknown as SearchScope, { text: sentinel }),
      () => search.upsert(undefined as unknown as SearchScope, note("bad")),
      () => search.delete(undefined as unknown as SearchScope, { id: "same", type: "note" }),
      () => search.upsert(a, note("bad", b)),
      () => search.query({ ...a, owners: ["alice", "bob"] } as SearchScope, { text: sentinel }),
    ]) {
      try {
        await run();
        throw new Error("expected rejection");
      } catch (error) {
        expect(error).toBeInstanceOf(SearchError);
        expect(JSON.stringify(searchErrorDiagnostic(error))).not.toContain(sentinel);
        expect((error as Error).cause).toBeUndefined();
      }
    }
  });

  test("empty query is not browsing; special characters and SQL injection stay parameters", async () => {
    expect(await search.query(a, { text: "   ", includeTotal: true })).toEqual({
      hits: [],
      total: 0,
    });
    const malicious = "'; DROP TABLE x; --";
    const id = "id' OR 1=1 --";
    await search.upsert(a, note(id, a, "injectionmarker", "safe"));
    expect((await search.query(a, { text: "injectionmarker" })).hits[0]?.id).toBe(id);
    for (const text of [malicious, '"', ":* & | ! ( )", "\\", "中文"]) {
      const page = await search.query(a, { text, includeTotal: true });
      expect(JSON.stringify(page)).not.toContain(sentinel);
    }
    expect(captured?.text).not.toContain(malicious);
    await search.delete(a, { id, type: "note" });
    expect((await search.query(a, { text: "injectionmarker" })).hits).toEqual([]);
    expect(() => createPostgresSearchProvider({ database, table: 'x"; DROP TABLE y; --' })).toThrow(
      SearchError,
    );
  });

  test("summaries are authorized bounded plain text, never generated HTML highlights", async () => {
    await search.upsert(
      a,
      note(
        "summary",
        a,
        "<script>alert('summarytoken')</script> summarytoken <img src=x onerror=alert(1)> " +
          "summarytoken ".repeat(80),
        "summarytoken",
      ),
    );
    const page = await search.query(a, { text: "summarytoken" });
    expect(page.hits).toHaveLength(1);
    expect(page.hits[0]!.summary.length).toBeLessThanOrEqual(240);
    expect(page.hits[0]!.summary).not.toContain("<b>");
    expect(JSON.stringify(page)).not.toContain(sentinel);
    await search.delete(a, { id: "summary", type: "note" });
  });

  test("stable rank ties and ID ordering, cursor binds every authorization/query parameter", async () => {
    for (const id of ["page-c", "page-a", "page-b"])
      await search.upsert(a, note(id, a, "pagingtoken"));
    for (const sort of ["relevance", "id"] as const) {
      const input = { text: "pagingtoken", pageSize: 1, includeTotal: true, sort };
      const first = await search.query(a, input);
      expect(first.total).toBe(3);
      expect(first.hits.map((hit) => hit.id)).toEqual(["page-a"]);
      expect(first.nextCursor).toBeDefined();
      const second = await search.query(a, { ...input, cursor: first.nextCursor });
      const third = await search.query(a, { ...input, cursor: second.nextCursor });
      expect(second.hits[0]?.id).toBe("page-b");
      expect(third.hits[0]?.id).toBe("page-c");
      expect(third.nextCursor).toBeUndefined();
      for (const [scope, query] of [
        [b, input],
        [c, input],
        [{ ...a, namespace: "different" }, input],
        [a, { ...input, text: sentinel }],
        [a, { ...input, sort: sort === "id" ? "relevance" : "id" }],
        [a, { ...input, pageSize: 2 }],
        [a, { ...input, includeTotal: false }],
        [a, { ...input, type: "other" }],
      ] as const) {
        await expect(
          search.query(scope, { ...query, cursor: first.nextCursor }),
        ).rejects.toMatchObject({ code: "invalid-cursor" });
      }
      await expect(
        search.query(a, { ...input, cursor: first.nextCursor + "x" }),
      ).rejects.toMatchObject({ code: "invalid-cursor" });
      expect(first.nextCursor).not.toContain(sentinel);
    }
    await expect(search.query(a, { text: "pagingtoken", pageSize: 101 })).rejects.toMatchObject({
      code: "invalid-input",
    });
  });

  test("title weighting, English stemming and optional exact count are real PG behavior", async () => {
    const english = createSearchService({
      provider: createPostgresSearchProvider({ database, table, language: "english" }),
      cursorSecret: new Uint8Array(32).fill(1),
    });
    await english.upsert(a, note("weight-body", a, "running", "ordinary"));
    await english.upsert(a, note("weight-title", a, "ordinary", "running"));
    const page = await english.query(a, { text: "run", includeTotal: true });
    expect(page.total).toBe(2);
    expect(page.hits.map((hit) => hit.id)).toEqual(["weight-title", "weight-body"]);
    expect(page.hits[0]!.score).toBeGreaterThan(page.hits[1]!.score);
    expect((await search.query(a, { text: "run", includeTotal: true })).total).toBe(0);
    expect((await english.query(a, { text: "run" })).total).toBeUndefined();
  });

  test("index failures and a real rolled-back DB failure can be retried safely", async () => {
    const broken = createSearchService({
      provider: createPostgresSearchProvider({ database, table: `${table}_absent` }),
      cursorSecret: new Uint8Array(32).fill(2),
    });
    try {
      await broken.upsert(a, note("failure"));
      throw new Error("expected rejection");
    } catch (error) {
      expect(error).toMatchObject({ code: "index-failed" });
      expect(JSON.stringify(searchErrorDiagnostic(error))).not.toContain(table);
      expect(JSON.stringify(searchErrorDiagnostic(error))).not.toContain(sentinel);
      expect((error as Error).cause).toBeUndefined();
    }
    await client
      .begin(async (transaction) => {
        const locked = createSearchService({
          provider: createPostgresSearchProvider({
            table,
            database: bunSqlSearchDatabase(transaction),
          }),
          cursorSecret: new Uint8Array(32).fill(3),
        });
        await transaction.unsafe("SET TRANSACTION READ ONLY");
        await expect(locked.upsert(a, note("retry", a, "retrytoken"))).rejects.toMatchObject({
          code: "index-failed",
        });
        // Failed transaction is explicitly rolled back by Bun when this callback rejects.
        throw new SearchError("index-failed");
      })
      .catch((error) => {
        expect(error).toBeInstanceOf(SearchError);
      });
    await search.upsert(a, note("retry", a, "retrytoken"));
    await search.upsert(a, note("retry", a, "retrytoken"));
    expect((await search.query(a, { text: "retrytoken", includeTotal: true })).total).toBe(1);
  });

  test("unavailable closed host connection returns a safe DB error", async () => {
    const closed = new SQL({ url: url!, adapter: "postgres" });
    const unavailable = createSearchService({
      provider: createPostgresSearchProvider({ database: bunSqlSearchDatabase(closed), table }),
      cursorSecret: new Uint8Array(32).fill(4),
    });
    await closed.close();
    for (const run of [
      () => unavailable.query(a, { text: sentinel }),
      () => unavailable.upsert(a, note("unavailable")),
      () => unavailable.delete(a, { id: "same", type: "note" }),
    ]) {
      const error = await run().catch((failure: unknown) => failure);
      expect(error).toMatchObject({ code: "db-unavailable" });
      expect(JSON.stringify(searchErrorDiagnostic(error))).not.toContain(sentinel);
      expect(JSON.stringify(searchErrorDiagnostic(error))).not.toContain(table);
      expect((error as Error).cause).toBeUndefined();
    }
  });

  test("finite 6000-document fixture uses a database index in the actual query plan", async () => {
    await client.unsafe(`INSERT INTO "${table}"
      (namespace, tenant_id, owner_id, document_type, document_id, title, body, language)
      SELECT 'plan', 'tenant-' || (n % 100)::text, 'owner', 'note', n::text,
        CASE WHEN n % 100 = 0 THEN 'rareplanmarker' ELSE 'ordinary' END,
        'bounded fixture', 'simple'::regconfig FROM generate_series(1, 6000) n`);
    await client.unsafe(`ANALYZE "${table}"`);
    const scope = { namespace: "plan", tenantId: "tenant-0", ownerId: "owner" };
    const page = await search.query(scope, { text: "rareplanmarker", includeTotal: true });
    expect(page.total).toBe(60);
    expect(page.hits.length).toBeLessThanOrEqual(20);
    const query = captured!;
    const plan = await client.unsafe(`EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ${query.text}`, [
      ...query.parameters,
    ]);
    const serialized = JSON.stringify(plan);
    expect(serialized).toMatch(/Index Scan|Bitmap Index Scan|Index Only Scan/);
    expect(serialized).toContain(table);
    const nodes: Record<string, unknown>[] = [];
    function collect(value: unknown): void {
      if (Array.isArray(value)) {
        for (const child of value) collect(child);
      } else if (value && typeof value === "object") {
        const node = value as Record<string, unknown>;
        if (node["Node Type"]) nodes.push(node);
        for (const child of Object.values(node)) collect(child);
      }
    }
    collect(plan);
    expect(
      nodes.filter((node) => node["Relation Name"] === table && node["Node Type"] === "Seq Scan"),
    ).toEqual([]);
    console.info("Search bounded PG plan:", {
      fixtureRows: 6000,
      authorizedMatches: page.total,
      returned: page.hits.length,
      indexes: nodes.map((node) => node["Index Name"]).filter(Boolean),
    });
  });
});
