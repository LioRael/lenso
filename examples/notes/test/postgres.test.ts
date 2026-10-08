import { expect, test } from "bun:test";
import { SQL } from "bun";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import type { RouterClient } from "@orpc/server";
import { createBunSqlPlugin } from "@lenso/db/bun-sql";
import { startApp } from "@lenso/core";
import { sql } from "drizzle-orm";
import { migratePostgres } from "../src/migrate-pg";
import { createNotesServer } from "../src/server";
import type { NotesRouter } from "../src/router";
import { notesAudiences } from "../src/notes";

const connection = process.env.LENSO_TEST_DATABASE_URL;

test.skipIf(!connection)(
  "real PostgreSQL: explicit migration, CLI process, HTTP CRUD, persisted restart and pool ownership",
  async () => {
    if (!connection) throw new Error("LENSO_TEST_DATABASE_URL is required");
    await migratePostgres(connection);
    await migratePostgres(connection);
    const key = Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
    const otherKey = Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
    const principals = [
      { subjectId: `notes-test-${crypto.randomUUID()}`, key },
      { subjectId: `notes-other-${crypto.randomUUID()}`, key: otherKey },
    ];
    const configuration = JSON.stringify(principals);
    const login = Bun.spawn(
      [process.execPath, new URL("../src/cli.ts", import.meta.url).pathname, "login"],
      {
        env: {
          ...process.env,
          DATABASE_URL: connection,
          NOTES_LOGIN_KEYS: configuration,
          NOTES_LOGIN_KEY: key,
        },
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const loginOutput = await new Response(login.stdout).text();
    const loginErrors = await new Response(login.stderr).text();
    if ((await login.exited) !== 0) throw new Error(`Notes login failed: ${loginErrors}`);
    const session: { credential: string } = JSON.parse(loginOutput);
    const rejected = Bun.spawn(
      [process.execPath, new URL("../src/cli.ts", import.meta.url).pathname, "list", "{}"],
      {
        env: {
          ...process.env,
          DATABASE_URL: connection,
          NOTES_LOGIN_KEYS: configuration,
          NOTES_SESSION: "",
        },
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    expect(await rejected.exited).toBe(1);
    expect(await new Response(rejected.stdout).text()).toBe("");
    expect(JSON.parse(await new Response(rejected.stderr).text()).code).toBe("UNAUTHORIZED");
    const title = `CLI note ${crypto.randomUUID()}`;
    const child = Bun.spawn(
      [
        process.execPath,
        new URL("../src/cli.ts", import.meta.url).pathname,
        "create",
        JSON.stringify({ title, body: "persisted by another process" }),
      ],
      {
        env: {
          ...process.env,
          DATABASE_URL: connection,
          NOTES_LOGIN_KEYS: configuration,
          NOTES_SESSION: session.credential,
        },
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
    const server = await createNotesServer(connection, principals, 0);
    const client: RouterClient<NotesRouter> = createORPCClient(
      new RPCLink({
        url: new URL("rpc", server.url),
        headers: () => ({ authorization: `Bearer ${session.credential}` }),
      }),
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
      const otherSession = await server.authentication.issue(otherKey);
      const otherClient: RouterClient<NotesRouter> = createORPCClient(
        new RPCLink({
          url: new URL("rpc", server.url),
          headers: { authorization: `Bearer ${otherSession.credential}` },
        }),
      );
      const otherNote = await otherClient.create({ title: "Other user's private note" });
      for (const args of [
        ["read", JSON.stringify({ id: otherNote.id })],
        ["update", JSON.stringify({ id: otherNote.id, title: "Stolen through CLI" })],
        ["remove", JSON.stringify({ id: otherNote.id })],
      ]) {
        const denied = Bun.spawn(
          [process.execPath, new URL("../src/cli.ts", import.meta.url).pathname, ...args],
          {
            env: {
              ...process.env,
              DATABASE_URL: connection,
              NOTES_LOGIN_KEYS: configuration,
              NOTES_SESSION: session.credential,
            },
            stdout: "pipe",
            stderr: "pipe",
          },
        );
        const [exit, stdout, stderr] = await Promise.all([
          denied.exited,
          new Response(denied.stdout).text(),
          new Response(denied.stderr).text(),
        ]);
        expect(exit).toBe(1);
        expect(stdout).toBe("");
        expect(JSON.parse(stderr).code).toBe("FORBIDDEN");
      }
      expect((await otherClient.read({ id: otherNote.id }))?.title).toBe(
        "Other user's private note",
      );
      await otherClient.remove({ id: otherNote.id });
      await server.authentication.revoke(session.credential);
      const afterRevoke = Bun.spawn(
        [process.execPath, new URL("../src/cli.ts", import.meta.url).pathname, "list", "{}"],
        {
          env: {
            ...process.env,
            DATABASE_URL: connection,
            NOTES_LOGIN_KEYS: configuration,
            NOTES_SESSION: session.credential,
          },
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      const [exit, stdout, stderr] = await Promise.all([
        afterRevoke.exited,
        new Response(afterRevoke.stdout).text(),
        new Response(afterRevoke.stderr).text(),
      ]);
      expect(exit).toBe(1);
      expect(stdout).toBe("");
      expect(JSON.parse(stderr).code).toBe("UNAUTHORIZED");
      await expect(client.list()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      const replacement = await server.authentication.issue(key);
      session.credential = replacement.credential;
      expect((await client.update({ id: row.id, title: "Updated through HTTP" }))?.title).toBe(
        "Updated through HTTP",
      );
    } finally {
      await server.app.stop();
    }
    const restarted = await createNotesServer(connection, principals, 0);
    try {
      const auth = restarted.authentication;
      expect(
        (
          await restarted.notes.list(
            await auth.for(notesAudiences.list).required(session.credential),
          )
        ).find((note) => note.id === row.id)?.title,
      ).toBe("Updated through HTTP");
      expect(
        await restarted.notes.remove(
          await auth.for(notesAudiences.remove).required(session.credential),
          row.id,
        ),
      ).toBe(true);
      expect(
        await restarted.notes.remove(
          await auth.for(notesAudiences.remove).required(session.credential),
          row.id,
        ),
      ).toBe(false);
      expect(
        await restarted.notes.update(
          await auth.for(notesAudiences.update).required(session.credential),
          row.id,
          { title: "Missing" },
        ),
      ).toBeNull();
      if (createdId)
        await restarted.notes.remove(
          await auth.for(notesAudiences.remove).required(session.credential),
          createdId,
        );
      await auth.revoke(session.credential);
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
