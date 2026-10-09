import { expect, test } from "bun:test";
import { defineApp, startApp } from "@lenso/core";
import { definePlugin } from "@lenso/core/plugin";
import {
  createSearchService,
  SearchError,
  resolveSearchConfig,
  type SearchDocument,
  type SearchProvider,
  type SearchQuery,
  type SearchScope,
} from "../src/index";
import { createPostgresSearchPlugin } from "../src/plugin";
import { createPostgresSearchProvider } from "../src/postgres";

const scope = { namespace: "host-owned" };
const document: SearchDocument = { id: "id", type: "note", title: "title", body: "body" };
function fixture() {
  let calls = 0;
  const provider: SearchProvider = {
    identity: "validation-test-only",
    capabilities: {
      fullText: true,
      sorts: ["id"],
      pagination: "offset",
      summary: "plain-text",
      count: "unsupported",
    },
    async upsert() {
      calls++;
      throw new Error("raw SQL must not escape");
    },
    async delete() {
      calls++;
      throw new Error("raw SQL must not escape");
    },
    async query() {
      calls++;
      throw new Error("raw SQL must not escape");
    },
  };
  return {
    service: createSearchService({ provider, cursorSecret: new Uint8Array(32) }),
    calls: () => calls,
    provider,
  };
}

test("strict document and metadata bounds reject arbitrary objects before provider I/O", async () => {
  const f = fixture();
  for (const input of [
    { ...document, title: 1 },
    { ...document, body: "x".repeat(100_001) },
    { ...document, title: "x".repeat(1025) },
    { ...document, id: "" },
    { ...document, type: "x".repeat(65) },
    { ...document, body: "\0" },
    { ...document, arbitrary: { secret: "not indexed" } },
    { ...document, metadata: { nested: { secret: "not indexed" } } },
    { ...document, metadata: { value: Infinity } },
    { ...document, metadata: { value: "x".repeat(513) } },
    { ...document, metadata: Array(10).fill("bad") },
    {
      ...document,
      metadata: Object.fromEntries(Array.from({ length: 17 }, (_, i) => [`k${i}`, i])),
    },
    {
      ...document,
      metadata: Object.fromEntries(
        Array.from({ length: 10 }, (_, i) => [`k${i}`, "x".repeat(500)]),
      ),
    },
  ])
    await expect(f.service.upsert(scope, input as SearchDocument)).rejects.toMatchObject({
      code: "invalid-input",
    });
  expect(f.calls()).toBe(0);
  const small = createSearchService({
    provider: f.provider,
    cursorSecret: new Uint8Array(32),
    config: { maxDocumentBytes: 20 },
  });
  await expect(small.upsert(scope, document)).rejects.toMatchObject({ code: "invalid-input" });
});

test("scope fields reject malformed values, wildcard objects and conflicting ownership", async () => {
  const f = fixture();
  for (const value of [
    {},
    null,
    { namespace: "" },
    { namespace: ["all"] },
    { namespace: "x", tenantId: "" },
    { namespace: "x", ownerId: 1 },
    { namespace: "x", owners: ["a", "b"] },
  ]) {
    await expect(f.service.upsert(value as SearchScope, document)).rejects.toBeInstanceOf(
      SearchError,
    );
    await expect(
      f.service.delete(value as SearchScope, { id: "id", type: "note" }),
    ).rejects.toBeInstanceOf(SearchError);
    await expect(f.service.query(value as SearchScope, { text: "term" })).rejects.toBeInstanceOf(
      SearchError,
    );
  }
  await expect(
    f.service.upsert(scope, { ...document, ownerId: "unexpected" }),
  ).rejects.toMatchObject({ code: "scope-conflict" });
  expect(f.calls()).toBe(0);
});

test("unsupported capability, query bounds and raw failures have fixed safe errors", async () => {
  const f = fixture();
  await expect(f.service.query(scope, { text: "term" })).rejects.toMatchObject({
    code: "unsupported-capability",
  });
  await expect(
    f.service.query(scope, { text: "term", sort: "id", includeTotal: true }),
  ).rejects.toMatchObject({ code: "unsupported-capability" });
  for (const input of [
    { text: "x".repeat(513) },
    { text: "term", pageSize: 0 },
    { text: "term", pageSize: 51 },
    { text: "term", pageSize: 1.1 },
    { text: "term", cursor: "" },
    { text: "term", includeTotal: "true" },
    { text: "term", unknown: true },
  ])
    await expect(
      f.service.query(scope, { ...input, sort: "id" } as SearchQuery),
    ).rejects.toBeInstanceOf(SearchError);
  expect(f.calls()).toBe(0);
  await expect(f.service.upsert(scope, document)).rejects.toMatchObject({ code: "index-failed" });
  await expect(f.service.delete(scope, { id: "id", type: "note" })).rejects.toMatchObject({
    code: "index-failed",
  });
  await expect(f.service.query(scope, { text: "term", sort: "id" })).rejects.toMatchObject({
    code: "query-failed",
  });
});

test("disabled service and PostgreSQL provider reject reads and writes without I/O", async () => {
  let calls = 0;
  const provider = createPostgresSearchProvider({
    database: {
      async execute() {
        calls++;
        throw new Error();
      },
    },
    config: { enabled: false },
  });
  const service = createSearchService({
    provider,
    cursorSecret: new Uint8Array(32),
    config: { enabled: false },
  });
  await expect(service.upsert(scope, document)).rejects.toMatchObject({ code: "disabled" });
  await expect(service.delete(scope, { id: "id", type: "note" })).rejects.toMatchObject({
    code: "disabled",
  });
  await expect(service.query(scope, { text: "term" })).rejects.toMatchObject({ code: "disabled" });
  await expect(
    provider.query(scope, {
      text: " ",
      pageSize: 1,
      offset: 0,
      sort: "id",
      includeTotal: true,
      summaryChars: 10,
    }),
  ).rejects.toMatchObject({ code: "disabled" });
  await expect(provider.upsert(scope, document)).rejects.toMatchObject({ code: "disabled" });
  await expect(provider.delete(scope, { id: "id", type: "note" })).rejects.toMatchObject({
    code: "disabled",
  });
  expect(calls).toBe(0);
});

test("config is bounded and explicit plugin borrows exact DB without migration or cleanup", async () => {
  for (const value of [
    { unknown: true },
    { enabled: "yes" },
    { maxOffset: -1 },
    { maxPageSize: 101 },
    { summaryChars: 1001 },
    { maxDocumentBytes: 2_000_001 },
  ]) {
    expect(() => resolveSearchConfig(value as never)).toThrow(SearchError);
  }
  expect(() =>
    createSearchService({ provider: fixture().provider, cursorSecret: new Uint8Array(31) }),
  ).toThrow(SearchError);
  expect(() =>
    createPostgresSearchProvider({
      database: { execute: async () => [] },
      table: { toString: () => "safe_table" } as unknown as string,
    }),
  ).toThrow(SearchError);
  let queries = 0;
  let cleanups = 0;
  const database = definePlugin({
    id: "host-db",
    setup(context) {
      context.onCleanup(() => {
        cleanups++;
      });
      return {
        async execute() {
          queries++;
          throw new Error();
        },
      };
    },
  });
  const plugin = createPostgresSearchPlugin({
    id: "search",
    database,
    adapter: (db) => db,
    config: { enabled: false },
    cursorSecret: new Uint8Array(32),
  });
  expect(plugin.requires).toEqual([database]);
  const app = await startApp(defineApp({ plugins: [database, plugin] }));
  expect(queries).toBe(0);
  await expect(app.get(plugin).query(scope, { text: "term" })).rejects.toMatchObject({
    code: "disabled",
  });
  await app.stop();
  expect(queries).toBe(0);
  expect(cleanups).toBe(1);
});
