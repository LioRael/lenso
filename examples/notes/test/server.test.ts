import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { isDevReadyMessage } from "@lenso/engine/dev-ready";
import { createNotesApplication } from "../src/application";
import { migrateSqlite } from "../src/migrate-sqlite";
import { notesAudiences } from "../src/notes";
import { createSqliteNotesQueries } from "../src/queries-sqlite";
import { startNotesServer } from "../src/server";
import * as schema from "../src/schema-sqlite";

test("Notes main does not report readiness when configuration fails", async () => {
  const messages: unknown[] = [];
  const child = Bun.spawn([process.execPath, join(import.meta.dir, "../src/server.ts")], {
    env: { ...process.env, DATABASE_URL: "" },
    stdout: "ignore",
    stderr: "pipe",
    ipc(message) {
      messages.push(message);
    },
  });
  const stderr = new Response(child.stderr).text();
  const timeout = setTimeout(() => child.kill("SIGKILL"), 5000);
  try {
    expect(await child.exited).not.toBe(0);
    expect(await stderr).toContain("Set DATABASE_URL");
    expect(messages.some(isDevReadyMessage)).toBe(false);
  } finally {
    clearTimeout(timeout);
    if (child.exitCode === null) child.kill("SIGKILL");
    await child.exited;
  }
});

test("shared SQLite assembly serves Notes with exact listener-origin validation", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-notes-server-"));
  const filename = join(directory, "notes.sqlite");
  try {
    migrateSqlite(filename);
    const database = createBunSqlitePlugin({ id: "notes-db", filename, schema });
    const definition = createNotesApplication({
      database,
      store: sqliteSessionStore,
      queries: createSqliteNotesQueries,
      principals: [{ subjectId: "A", key: "a".repeat(64) }],
    });
    const server = await startNotesServer(definition, 0);
    try {
      const credential = (await server.authentication.issue("a".repeat(64))).credential;
      const actor = await server.authentication.for(notesAudiences.create).required(credential);
      expect((await server.notes.create(actor, { title: "Shared assembly" })).ownerId).toBe("A");
      const rpc = new URL("rpc", server.url);
      const rejected = await fetch(rpc, { headers: { origin: "http://localhost:1" } });
      expect(rejected.status).toBe(403);
      expect(await rejected.text()).toBe("Invalid origin");
      const accepted = await fetch(rpc, { headers: { origin: server.url.origin } });
      expect(accepted.status).not.toBe(403);
      await accepted.arrayBuffer();
      const absent = await fetch(rpc);
      expect(absent.status).not.toBe(403);
      await absent.arrayBuffer();
    } finally {
      await server.app.stop();
    }
    await expect(fetch(server.url)).rejects.toThrow();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

for (const invalid of [false, true]) {
  test(`SQLite child ${invalid ? "startup rollback has no readiness" : "readiness precedes clean shutdown"}`, async () => {
    const directory = await mkdtemp(join(tmpdir(), "lenso-notes-ipc-"));
    const filename = join(directory, "notes.sqlite");
    migrateSqlite(filename);
    const messages: unknown[] = [];
    let resolveReady!: (url: string) => void;
    const ready = new Promise<string>((resolve) => {
      resolveReady = resolve;
    });
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dir, "fixtures/server-child.ts"),
        filename,
        invalid ? "invalid" : "valid",
      ],
      {
        stdout: "ignore",
        stderr: "pipe",
        ipc(message) {
          messages.push(message);
          if (isDevReadyMessage(message) && message.urls?.[0]) resolveReady(message.urls[0]);
        },
      },
    );
    const stderr = new Response(child.stderr).text();
    const timeout = setTimeout(() => child.kill("SIGKILL"), 5000);
    try {
      if (!invalid) {
        const url = await Promise.race([
          ready,
          child.exited.then(async () => {
            throw new Error(await stderr);
          }),
        ]);
        const response = await fetch(new URL("rpc", url));
        await response.arrayBuffer();
        child.kill("SIGTERM");
        expect(await child.exited).toBe(0);
        await expect(fetch(url)).rejects.toThrow();
      } else {
        expect(await child.exited).not.toBe(0);
        expect(messages.some(isDevReadyMessage)).toBe(false);
      }
      expect(messages).toContainEqual({ type: "database-closed" });
      await stderr;
    } finally {
      clearTimeout(timeout);
      if (child.exitCode === null) child.kill("SIGKILL");
      await child.exited;
      await rm(directory, { recursive: true, force: true });
    }
  });
}
