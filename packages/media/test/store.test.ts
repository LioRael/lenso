import { Database } from "bun:sqlite";
import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createSqliteMediaStore } from "../src/sqlite";
import { createD1MediaStore } from "../src/d1";
import type { MediaArtifact, MediaRecord } from "../src/contracts";

const databases: Database[] = [];
const directories: string[] = [];

afterEach(() => {
  for (const db of databases.splice(0)) db.close();
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

function database(path = ":memory:"): Database {
  const db = new Database(path);
  databases.push(db);
  return db;
}

function migrate(db: Database): void {
  db.exec(readFileSync(new URL("../migrations/sqlite/0001_media.sql", import.meta.url), "utf8"));
}

function record(id: string, revision = 0): MediaRecord {
  return { id, revision, nested: { value: "initial" } } as unknown as MediaRecord;
}

function artifact(fileId: string, derivationId: string, revision = 0): MediaArtifact {
  return {
    fileId,
    derivationId,
    fence: 1,
    executionId: "execution",
    revision,
    state: "staged",
    createdAt: 1,
  };
}

describe("SQLite media store", () => {
  test("insert is idempotent and reads are detached JSON copies", async () => {
    const db = database();
    migrate(db);
    const store = createSqliteMediaStore(db);
    expect(await Promise.all([store.insert(record("m1")), store.insert(record("m1"))])).toEqual([
      true,
      false,
    ]);
    const loaded = await store.get("m1");
    expect(loaded).toEqual(record("m1"));
    (loaded as unknown as { nested: { value: string } }).nested.value = "mutated";
    expect(await store.get("m1")).toEqual(record("m1"));
  });

  test("CAS permits one winner and rejects stale or invalid revisions", async () => {
    const db = database();
    migrate(db);
    const store = createSqliteMediaStore(db);
    await store.insert(record("m1"));
    const updates = await Promise.all([
      store.replace("m1", 0, record("m1", 1)),
      store.replace("m1", 0, record("m1", 1)),
    ]);
    expect(updates.filter(Boolean)).toHaveLength(1);
    expect(await store.replace("m1", 0, record("m1", 1))).toBe(false);
    await expect(store.replace("m1", 1, record("other", 2))).rejects.toThrow();
    await expect(store.replace("m1", 1, record("m1", 3))).rejects.toThrow();
  });

  test("artifacts are unique, queryable in deterministic order, and CAS replaceable", async () => {
    const db = database();
    migrate(db);
    const store = createSqliteMediaStore(db);
    await store.insert(record("m1"));
    await store.insertArtifact(artifact("z", "m1"));
    await store.insertArtifact(artifact("a", "m1"));
    expect(await store.artifacts("m1")).toEqual([artifact("a", "m1"), artifact("z", "m1")]);
    expect(await store.getArtifact("z")).toEqual(artifact("z", "m1"));
    await expect(store.insertArtifact(artifact("z", "m1"))).rejects.toThrow();
    expect(await store.replaceArtifact("z", 0, artifact("z", "m1", 1))).toBe(true);
    expect(await store.replaceArtifact("z", 0, artifact("z", "m1", 1))).toBe(false);
  });

  test("data survives reopening a file database", async () => {
    const directory = mkdtempSync(join(tmpdir(), "lenso-store-test-"));
    directories.push(directory);
    const path = join(directory, "media.sqlite");
    const first = database(path);
    migrate(first);
    await createSqliteMediaStore(first).insert(record("persisted"));
    first.close();
    databases.splice(databases.indexOf(first), 1);
    const reopened = database(path);
    expect(await createSqliteMediaStore(reopened).get("persisted")).toEqual(record("persisted"));
  });

  test("factory does not migrate and missing schema fails on operation", async () => {
    const store = createSqliteMediaStore(database());
    await expect(store.get("missing")).rejects.toThrow();
  });
});

test("D1 journal fails closed if a binding resolves an unsuccessful insertion", async () => {
  const statement = {
    bind() {
      return this;
    },
    async first<T>() {
      return null as T | null;
    },
    async run() {
      return { success: false, meta: { changes: 0 } };
    },
    async all<T>() {
      return { success: false, results: [] as T[] };
    },
  };
  const store = createD1MediaStore({
    prepare: () => statement,
    withSession() {
      return {};
    },
  });
  await expect(store.insertArtifact(artifact("untracked", "m1"))).rejects.toThrow();
  expect(() => createD1MediaStore({ prepare: () => statement } as never)).toThrow("plain D1");
});
