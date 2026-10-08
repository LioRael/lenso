import { expect, test } from "bun:test";
import { SQL } from "bun";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import type { RouterClient } from "@orpc/server";
import { createBunSqlPlugin } from "@lenso/db/bun-sql";
import { startApp } from "lenso";
import { sql } from "drizzle-orm";
import { migratePostgres } from "../src/migrate-pg";
import { createNotesServer } from "../src/server";
import type { NotesRouter } from "../src/router";

const connection = process.env.LENSO_TEST_DATABASE_URL;

test.skipIf(!connection)(
  "real PostgreSQL: explicit migration, CLI process, HTTP CRUD, persisted restart and pool ownership",
  async () => {
    if (!connection) throw new Error("LENSO_TEST_DATABASE_URL is required");
    await migratePostgres(connection);
    await migratePostgres(connection);
    const title = `CLI note ${crypto.randomUUID()}`;
    const child = Bun.spawn(
      [
        process.execPath,
        new URL("../src/cli.ts", import.meta.url).pathname,
        "create",
        title,
        "persisted by another process",
      ],
      {
        env: { ...process.env, DATABASE_URL: connection },
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const output = await new Response(child.stdout).text();
    const errors = await new Response(child.stderr).text();
    expect(await child.exited, errors).toBe(0);
    const row: { id: string; title: string; body: string; createdAt: string } = JSON.parse(output);
    expect(row.title).toBe(title);
    let createdId: string | undefined;
    const server = await createNotesServer(connection, 0);
    const client: RouterClient<NotesRouter> = createORPCClient(
      new RPCLink({ url: new URL("rpc", server.url) }),
    );
    try {
      expect((await client.list()).find((note) => note.id === row.id)?.body).toBe(
        "persisted by another process",
      );
      await expect(client.create({ title: " " })).rejects.toMatchObject({ code: "BAD_REQUEST" });
      const created = await client.create({
        title: "HTTP '); DROP TABLE notes; --",
        body: "via the same service",
      });
      createdId = created.id;
      expect((await client.update({ id: row.id, title: "Updated through HTTP" }))?.title).toBe(
        "Updated through HTTP",
      );
    } finally {
      await server.app.stop();
    }
    const restarted = await createNotesServer(connection, 0);
    try {
      expect((await restarted.notes.list()).find((note) => note.id === row.id)?.title).toBe(
        "Updated through HTTP",
      );
      expect(await restarted.notes.remove(row.id)).toBe(true);
      expect(await restarted.notes.remove(row.id)).toBe(false);
      expect(await restarted.notes.update(row.id, { title: "Missing" })).toBeNull();
      if (createdId) await restarted.notes.remove(createdId);
    } finally {
      await restarted.app.stop();
    }
    const borrowed = new SQL(connection);
    try {
      const plugin = createBunSqlPlugin({ id: "borrowed-pg", client: borrowed, schema: {} });
      const app = await startApp({ plugins: [plugin] });
      await app.stop();
      const [version] = await borrowed`select version() as version`;
      expect(version.version).toContain("PostgreSQL");
    } finally {
      await borrowed.close();
    }
    const owned = createBunSqlPlugin({ id: "owned-pg", connection, schema: {} });
    const app = await startApp({ plugins: [owned] });
    const database = app.get(owned);
    await database.execute(sql`select 1`);
    await app.stop();
    await expect(Promise.resolve(database.execute(sql`select 1`))).rejects.toThrow();
  },
  20_000,
);
