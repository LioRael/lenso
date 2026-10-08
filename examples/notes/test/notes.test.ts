import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";
import { audience, createAuth, defineSource, realm } from "@lenso/auth";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import type { RouterClient } from "@orpc/server";
import { startApp } from "lenso";
import { eq } from "drizzle-orm";
import { createNotesAuthPlugin, parseNotesPrincipals } from "../src/auth";
import { createNotesOperationsService } from "../src/operations";
import { migrateSqlite } from "../src/migrate-sqlite";
import {
  createNotesPlugin,
  createNotesService,
  NoteInputError,
  notesAudiences,
  type NotesActor,
} from "../src/notes";
import { createSqliteNotesQueries } from "../src/queries-sqlite";
import type { NotesRouter } from "../src/router";
import { createNotesWebPlugin } from "../src/web";
import * as schema from "../src/schema-sqlite";

function fixtureKey(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

test("SQLite private CRUD, CLI dispatch, HTTP and persistent sessions", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-notes-"));
  const filename = join(directory, "notes.sqlite");
  const archive = join(directory, "archive.sqlite");
  const principals = [
    { subjectId: "A", key: fixtureKey() },
    { subjectId: "B", key: fixtureKey() },
  ];
  const database = createBunSqlitePlugin({ id: "notes-db", filename, schema });
  const archiveDatabase = createBunSqlitePlugin({ id: "archive-db", filename: archive, schema });
  const authentication = createNotesAuthPlugin({
    database,
    store: sqliteSessionStore,
    principals,
    lifetime: { idle: 60_000, absolute: 120_000, renewAfter: 1 },
  });
  const archiveAuth = createNotesAuthPlugin({
    id: "archive-auth",
    database: archiveDatabase,
    store: sqliteSessionStore,
    principals,
  });
  const notes = createNotesPlugin({
    id: "notes",
    database,
    authentication,
    queries: createSqliteNotesQueries,
  });
  const archiveNotes = createNotesPlugin({
    id: "archive",
    database: archiveDatabase,
    authentication: archiveAuth,
    queries: createSqliteNotesQueries,
  });
  const web = createNotesWebPlugin(notes, authentication);
  const definition = {
    plugins: [web, notes, database, authentication, archiveNotes, archiveDatabase, archiveAuth],
  };
  let credential = "";
  let id = "";
  try {
    migrateSqlite(filename);
    migrateSqlite(filename);
    migrateSqlite(archive);
    const app = await startApp(definition);
    try {
      const service = app.get(notes);
      const auth = app.get(authentication);
      credential = (await auth.issue(principals[0]!.key)).credential;
      const other = (await auth.issue(principals[1]!.key)).credential;
      const actor = <O extends keyof typeof notesAudiences>(operation: O, token = credential) =>
        auth.for(audience(`notes:${operation}`)).required(token);
      await expect(service.create(null, { title: "Anonymous" })).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      await expect(
        createNotesOperationsService(service, auth, () => null).list({}),
      ).rejects.toMatchObject({
        diagnostic: { code: "UNAUTHORIZED" },
      });
      await expect(service.create(await actor("create"), { title: " " })).rejects.toBeInstanceOf(
        NoteInputError,
      );
      await createNotesOperationsService(service, auth, () => credential).create({
        title: "  Persistent '); DROP TABLE notes; --  ",
        body: "body",
      });
      const [row] = await service.list(await actor("list"));
      id = row!.id;
      expect(row!.ownerId).toBe("A");
      expect(row!.title).toBe("Persistent '); DROP TABLE notes; --");
      const privateNote = await service.create(await actor("create", other), {
        title: "B's private note",
      });
      expect((await service.list(await actor("list"))).map((note) => note.id)).toEqual([id]);
      expect(await service.list(await actor("list", other))).toEqual([privateNote]);
      await expect(service.read(await actor("read"), privateNote.id)).rejects.toMatchObject({
        code: "FORBIDDEN",
      });
      await expect(
        service.update(await actor("update"), privateNote.id, { title: "Stolen" }),
      ).rejects.toMatchObject({ code: "FORBIDDEN" });
      await expect(service.remove(await actor("remove"), privateNote.id)).rejects.toMatchObject({
        code: "FORBIDDEN",
      });
      const immutable = await service.read(await actor("read"), id);
      const extraFields = {
        title: row!.title,
        body: "body",
        ownerId: "B",
        id: crypto.randomUUID(),
        createdAt: new Date(0),
      };
      await expect(service.update(await actor("update"), id, extraFields)).rejects.toBeInstanceOf(
        NoteInputError,
      );
      expect(await service.read(await actor("read"), id)).toEqual(immutable);

      const native = app.get(database);
      const queries = createSqliteNotesQueries(native);
      const staleWriter = createNotesService(
        {
          ...queries,
          async update(noteId, ownerId, input) {
            await native
              .update(schema.notes)
              .set({ ownerId: "B" })
              .where(eq(schema.notes.id, noteId));
            return queries.update(noteId, ownerId, input);
          },
          async remove(noteId, ownerId) {
            await native
              .update(schema.notes)
              .set({ ownerId: "B" })
              .where(eq(schema.notes.id, noteId));
            return queries.remove(noteId, ownerId);
          },
        },
        auth,
      );
      const transferred = await service.create(await actor("create"), {
        title: "Transferred before update",
      });
      expect(
        await staleWriter.update(await actor("update"), transferred.id, { title: "Stale write" }),
      ).toBeNull();
      expect((await service.read(await actor("read", other), transferred.id))?.title).toBe(
        "Transferred before update",
      );
      await service.remove(await actor("remove", other), transferred.id);
      const retained = await service.create(await actor("create"), {
        title: "Transferred before remove",
      });
      expect(await staleWriter.remove(await actor("remove"), retained.id)).toBe(false);
      expect((await service.read(await actor("read", other), retained.id))?.title).toBe(
        "Transferred before remove",
      );
      await service.remove(await actor("remove", other), retained.id);
      await expect(service.read(null, crypto.randomUUID())).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      await expect(
        service.update(null, crypto.randomUUID(), { title: "Missing" }),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      await expect(service.remove(null, crypto.randomUUID())).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      await expect(
        service.read((await actor("list")) as unknown as NotesActor<"read">, id),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      const foreign = createAuth(
        realm(
          "foreign",
          defineSource<string, string>({
            async verify() {
              return { status: "verified", subjectId: "A", kind: "user" };
            },
          }),
        ),
      );
      try {
        await expect(
          service.read(
            (await foreign
              .for(notesAudiences.read)
              .required("test")) as unknown as NotesActor<"read">,
            id,
          ),
        ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      } finally {
        await foreign.close();
      }
      const nonUser = createAuth(
        realm(
          "notes",
          defineSource<string | null, string>({
            async verify() {
              return { status: "verified", subjectId: "A", kind: "service" };
            },
          }),
        ),
      );
      try {
        const nonUserService = createNotesService(createSqliteNotesQueries(app.get(database)), {
          ...auth,
          ...nonUser,
        });
        await expect(
          nonUserService.read(await nonUser.for(notesAudiences.read).required("test"), id),
        ).rejects.toMatchObject({ code: "FORBIDDEN" });
      } finally {
        await nonUser.close();
      }
      await expect(
        service.read(
          {
            realmId: "notes",
            subjectId: "A",
            kind: "user",
            audience: "notes:read",
          } as NotesActor<"read">,
          id,
        ),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      const archiveAuthentication = app.get(archiveAuth);
      const archiveToken = (await archiveAuthentication.issue(principals[0]!.key)).credential;
      expect(
        await app
          .get(archiveNotes)
          .list(await archiveAuthentication.for(notesAudiences.list).required(archiveToken)),
      ).toEqual([]);
      await app
        .get(archiveNotes)
        .create(await archiveAuthentication.for(notesAudiences.create).required(archiveToken), {
          title: "Separate archive",
        });
      const handler = app.get(web);
      const request = (
        path: string,
        method = "GET",
        token: string | null = credential,
        body?: unknown,
      ) =>
        handler.fetch(
          new Request(`http://notes.test${path}`, {
            method,
            headers: {
              ...(token ? { authorization: `Bearer ${token}` } : {}),
              "content-type": "application/json",
            },
            ...(body === undefined ? {} : { body: JSON.stringify(body) }),
          }),
        );
      const loginResponse = await request("/session", "POST", null, { key: principals[0]!.key });
      expect(loginResponse.status).toBe(200);
      const loginSession: { credential: string } = await loginResponse.json();
      expect((await request("/session", "POST", null, { key: fixtureKey() })).status).toBe(401);
      const createdResponse = await request("/notes", "POST", loginSession.credential, {
        title: "REST note",
      });
      expect(createdResponse.status).toBe(201);
      const restNote: { id: string; ownerId: string } = await createdResponse.json();
      expect(restNote.ownerId).toBe("A");
      expect((await request(`/notes/${restNote.id}`)).status).toBe(200);
      const changed = await request(`/notes/${restNote.id}`, "PATCH", credential, {
        title: "Updated REST",
      });
      expect((await changed.json()).title).toBe("Updated REST");
      expect(await (await request(`/notes/${restNote.id}`, "DELETE")).json()).toEqual({
        removed: true,
      });
      expect(await (await request(`/notes/${restNote.id}`)).json()).toBeNull();
      expect((await request("/session/revoke", "POST", loginSession.credential)).status).toBe(200);
      expect((await request("/notes", "GET", loginSession.credential)).status).toBe(401);
      expect((await request("/notes", "GET", null)).status).toBe(401);
      expect((await request(`/notes/${privateNote.id}`)).status).toBe(403);
      expect(
        (await request(`/notes/${privateNote.id}`, "PATCH", credential, { title: "Stolen" }))
          .status,
      ).toBe(403);
      expect((await request(`/notes/${privateNote.id}`, "DELETE")).status).toBe(403);
      expect(
        (await request("/notes", "POST", credential, { title: "Input", ownerId: "B" })).status,
      ).toBe(400);
      expect(
        (
          await request(`/notes/${id}`, "PATCH", credential, {
            title: "Input",
            ownerId: "B",
            id: crypto.randomUUID(),
            createdAt: new Date(0).toISOString(),
          })
        ).status,
      ).toBe(400);
      expect((await request("/notes", "PUT")).status).toBe(405);
      expect((await request("/unknown")).status).toBe(404);
      expect(
        (
          await handler.fetch(
            new Request("http://notes.test/notes", { headers: { authorization: "Basic secret" } }),
          )
        ).status,
      ).toBe(401);
      const client: RouterClient<NotesRouter> = createORPCClient(
        new RPCLink({
          url: "http://notes.test/rpc",
          headers: () => ({ authorization: `Bearer ${credential}` }),
          fetch: (input, init) => handler.fetch(new Request(input, init)),
        }),
      );
      expect((await client.read({ id }))?.id).toBe(id);
      await expect(client.read({ id: privateNote.id })).rejects.toMatchObject({
        code: "FORBIDDEN",
      });
      await expect(client.create({ title: " " })).rejects.toMatchObject({ code: "BAD_REQUEST" });
      const injectedUpdate = {
        id,
        title: "Input",
        ownerId: "B",
        createdAt: new Date(0).toISOString(),
      };
      await expect(client.update(injectedUpdate)).rejects.toMatchObject({ code: "BAD_REQUEST" });
      await Bun.sleep(2);
      const renewed = await request("/session/renew", "POST");
      expect(renewed.status).toBe(200);
      const rotation: { credential: string } = await renewed.json();
      expect((await request("/notes")).status).toBe(401);
      credential = rotation.credential;
      const proof = await actor("read", other);
      await auth.revoke(other);
      await expect(service.read(proof, privateNote.id)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      await expect(
        createNotesOperationsService(service, auth, () => other).list({}),
      ).rejects.toMatchObject({
        diagnostic: { code: "UNAUTHORIZED" },
      });
    } finally {
      await app.stop();
    }
    const restarted = await startApp(definition);
    try {
      const service = restarted.get(notes);
      const auth = restarted.get(authentication);
      const actor = <O extends keyof typeof notesAudiences>(operation: O) =>
        auth.for(audience(`notes:${operation}`)).required(credential);
      expect((await service.list(await actor("list")))[0]!.id).toBe(id);
      expect((await service.read(await actor("read"), id))?.body).toBe("body");
      expect(
        (await service.update(await actor("update"), id, { title: "Updated", body: "new body" }))
          ?.body,
      ).toBe("new body");
      expect(await service.remove(await actor("remove"), id)).toBe(true);
      expect(await service.remove(await actor("remove"), id)).toBe(false);
      expect(await service.update(await actor("update"), id, { title: "Missing" })).toBeNull();
      expect(await service.read(await actor("read"), id)).toBeNull();
      expect(await service.list(await actor("list"))).toEqual([]);
      await auth.revoke(credential);
      await expect(auth.for(notesAudiences.list).required(credential)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
    } finally {
      await restarted.stop();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("login configuration is explicit", () => {
  const key = fixtureKey();
  expect(parseNotesPrincipals(JSON.stringify([{ subjectId: "A", key }]))).toEqual([
    { subjectId: "A", key },
  ]);
  for (const value of [
    undefined,
    "not JSON",
    "[]",
    JSON.stringify([{ subjectId: "__legacy_unowned__", key }]),
    JSON.stringify([
      { subjectId: "A", key },
      { subjectId: "A", key: fixtureKey() },
    ]),
  ])
    expect(() => parseNotesPrincipals(value)).toThrow("Configure NOTES_LOGIN_KEYS");
});

test("legacy SQLite rows remain unowned after idempotent migration", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-notes-legacy-"));
  const filename = join(directory, "legacy.sqlite");
  const client = new Database(filename);
  try {
    client.exec(
      await Bun.file(new URL("../migrations/sqlite/0000_notes.sql", import.meta.url)).text(),
    );
    client.run("INSERT INTO notes (id,title,body,created_at) VALUES (?,?,?,?)", [
      crypto.randomUUID(),
      "Legacy private data",
      "",
      Date.now(),
    ]);
    client.exec(
      "CREATE TABLE __drizzle_migrations (id SERIAL PRIMARY KEY, hash text NOT NULL, created_at numeric)",
    );
    const journal: { entries: { when: number }[] } = await Bun.file(
      new URL("../migrations/sqlite/meta/_journal.json", import.meta.url),
    ).json();
    client.run("INSERT INTO __drizzle_migrations (hash,created_at) VALUES (?,?)", [
      "legacy",
      journal.entries[0]!.when,
    ]);
    migrateSqlite(filename);
    migrateSqlite(filename);
    expect(client.query("SELECT owner_id FROM notes").get()).toEqual({
      owner_id: "__legacy_unowned__",
    });
    expect(client.query("SELECT count(*) AS count FROM auth_sessions").get()).toEqual({ count: 0 });
  } finally {
    client.close();
    await rm(directory, { recursive: true, force: true });
  }
});
