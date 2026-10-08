import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { test } from "node:test";
import { URL } from "node:url";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import type { RouterClient } from "@orpc/server";
import type { NotesRouter } from "../../notes/src/router";
import type { Note } from "../../notes/src/notes";
import { drizzle } from "drizzle-orm/d1";
import { d1SessionStore } from "@lenso/auth/drizzle/d1";
import { createManagedSessions } from "@lenso/auth/sessions";

function loginKey(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

const principals = [
  { subjectId: "alice", key: loginKey() },
  { subjectId: "bob", key: loginKey() },
];

async function runtime() {
  const script = await readFile(new URL("../.lenso/workerd/notes.js", import.meta.url), "utf8");
  return new Miniflare({
    ...convertV4MiniflareOptions({
      modules: true,
      script,
      compatibilityDate: "2026-10-06",
      compatibilityFlags: ["nodejs_compat", "enable_request_signal"],
      d1Databases: { DB: crypto.randomUUID() },
      d1Persist: false,
      bindings: {
        NOTES_LOGIN_KEYS: JSON.stringify(principals),
        NOTES_RENEW_AFTER_MS: "1",
      },
    }),
    host: "127.0.0.1",
    port: 0,
    telemetry: { enabled: false },
  });
}

async function migrate(mf: Miniflare) {
  const db = await mf.getD1Database("DB");
  const directory = new URL("../../notes/migrations/sqlite/", import.meta.url);
  for (const file of (await readdir(directory)).filter((path) => path.endsWith(".sql")).sort()) {
    const script = await readFile(new URL(file, directory), "utf8");
    // These reviewed baseline migrations contain DDL only, without SQL procedural bodies.
    const ddl = script.replace(/--[^\n]*/g, "");
    for (const statement of ddl
      .split(";")
      .map((sql) => sql.trim())
      .filter(Boolean)) {
      await db.prepare(statement).run();
    }
  }
  return db;
}

interface IssuedSession {
  credential: string;
  sessionId: string;
  expiresAt: number;
}

async function post(url: URL, path: string, token?: string, body?: unknown) {
  return fetch(new URL(path, url), {
    method: "POST",
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body === undefined ? {} : { "content-type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

async function login(url: URL, subjectId: "alice" | "bob"): Promise<IssuedSession> {
  const response = await post(url, "/session", undefined, {
    key: principals.find((principal) => principal.subjectId === subjectId)!.key,
  });
  assert.equal(response.status, 200);
  return response.json() as Promise<IssuedSession>;
}

function rpc(url: URL, token?: string): RouterClient<NotesRouter> {
  return createORPCClient(
    new RPCLink({
      origin: url.origin,
      url: "/rpc",
      headers: token ? { authorization: `Bearer ${token}` } : {},
    }),
  );
}

test("actual Notes workerd/D1 refuses anonymous private RPC", { timeout: 30_000 }, async () => {
  const mf = await runtime();
  try {
    await migrate(mf);
    const url = new URL((await mf.ready).href);
    await assert.rejects(rpc(url).list(), { code: "UNAUTHORIZED" });
    const response = await fetch(new URL("/notes", url));
    assert.equal(response.status, 401);
    assert.equal(((await response.json()) as { code: string }).code, "UNAUTHORIZED");
  } finally {
    await mf.dispose();
  }
});

test(
  "actual Notes workerd/D1 shares private object authorization across Fetch and oRPC",
  { timeout: 30_000 },
  async () => {
    const mf = await runtime();
    try {
      const db = await migrate(mf);
      const url = new URL((await mf.ready).href);
      const alice = await login(url, "alice");
      const bob = await login(url, "bob");
      const a = rpc(url, alice.credential);
      const b = rpc(url, bob.credential);
      const noteA = await a.create({ title: "Alice private" });
      const createdB = await post(url, "/notes", bob.credential, { title: "Bob private" });
      assert.equal(createdB.status, 201);
      const noteB = (await createdB.json()) as Note;
      assert.deepEqual(
        (await a.list()).map((note) => note.id),
        [noteA.id],
      );
      assert.deepEqual(
        (await b.list()).map((note) => note.id),
        [noteB.id],
      );
      for (const [caller, other] of [
        [a, noteB],
        [b, noteA],
      ] as const) {
        await assert.rejects(caller.read({ id: other.id }), { code: "FORBIDDEN" });
        await assert.rejects(caller.update({ id: other.id, title: "stolen" }), {
          code: "FORBIDDEN",
        });
        await assert.rejects(caller.remove({ id: other.id }), { code: "FORBIDDEN" });
      }
      for (const method of ["GET", "PATCH", "DELETE"]) {
        const response = await fetch(new URL(`/notes/${noteB.id}`, url), {
          method,
          headers: {
            authorization: `Bearer ${alice.credential}`,
            "content-type": "application/json",
          },
          body: method === "PATCH" ? JSON.stringify({ title: "stolen" }) : undefined,
        });
        assert.equal(response.status, 403);
        await response.arrayBuffer();
      }
      const own = await fetch(new URL(`/notes/${noteA.id}`, url), {
        headers: { authorization: `Bearer ${alice.credential}` },
      });
      assert.equal(own.status, 200);
      assert.equal(((await own.json()) as Note).title, "Alice private");
      assert.equal((await b.read({ id: noteB.id }))?.title, "Bob private");

      const foreign = createManagedSessions({
        realmId: "operators",
        login: {
          async verify(key: string) {
            return key === principals[0]!.key
              ? { status: "verified", subjectId: "alice" }
              : { status: "rejected" };
          },
        },
        store: d1SessionStore(drizzle(db)),
        lifetime: { idle: 60_000, absolute: 120_000, renewAfter: 1 },
        subjectActive: async () => true,
      });
      try {
        const wrongRealm = await foreign.issue(principals[0]!.key);
        await assert.rejects(rpc(url, wrongRealm.credential).list(), { code: "UNAUTHORIZED" });
      } finally {
        await foreign.close();
      }
      const revoked = await post(url, "/session/revoke", alice.credential);
      assert.equal(revoked.status, 200);
      assert.deepEqual(await revoked.json(), { revoked: true });
      await assert.rejects(a.list(), { code: "UNAUTHORIZED" });
      await assert.rejects(a.update({ id: noteA.id, title: "after revoke" }), {
        code: "UNAUTHORIZED",
      });
      const after = await fetch(new URL(`/notes/${noteA.id}`, url), {
        headers: { authorization: `Bearer ${alice.credential}` },
      });
      assert.equal(after.status, 401);
      await after.arrayBuffer();
    } finally {
      await mf.dispose();
    }
  },
);

test(
  "actual Notes workerd/D1 concurrent renewals have one winner and revocation never resurrects",
  { timeout: 30_000 },
  async () => {
    const mf = await runtime();
    try {
      await migrate(mf);
      const url = new URL((await mf.ready).href);
      const issued = await login(url, "alice");
      const results = await Promise.all([
        post(url, "/session/renew", issued.credential),
        post(url, "/session/renew", issued.credential),
      ]);
      assert.deepEqual(results.map((response) => response.status).sort(), [200, 401]);
      const winner = (await results
        .find((response) => response.status === 200)!
        .json()) as IssuedSession;
      await results.find((response) => response.status === 401)!.arrayBuffer();
      await assert.rejects(rpc(url, issued.credential).list(), { code: "UNAUTHORIZED" });
      await rpc(url, winner.credential).list();
      const revoked = await post(url, "/session/revoke", winner.credential);
      assert.equal(revoked.status, 200);
      assert.deepEqual(await revoked.json(), { revoked: true });
      await assert.rejects(rpc(url, winner.credential).list(), { code: "UNAUTHORIZED" });
      assert.equal((await post(url, "/session/renew", winner.credential)).status, 401);

      const raced = await login(url, "alice");
      const [revoke, renew] = await Promise.all([
        post(url, "/session/revoke", raced.credential),
        post(url, "/session/renew", raced.credential),
      ]);
      assert.ok(revoke.status === 200 || revoke.status === 401);
      assert.ok(renew.status === 200 || renew.status === 401);
      const successor = renew.status === 200 ? ((await renew.json()) as IssuedSession) : null;
      if (renew.status !== 200) await renew.arrayBuffer();
      await revoke.arrayBuffer();
      if (revoke.status === 200) {
        await assert.rejects(rpc(url, raced.credential).list(), { code: "UNAUTHORIZED" });
        if (successor) {
          await assert.rejects(rpc(url, successor.credential).list(), { code: "UNAUTHORIZED" });
        }
      } else {
        // Rotation can invalidate the old credential before logout reads it; revoke the current one.
        assert.ok(successor);
        const revoked = await post(url, "/session/revoke", successor.credential);
        assert.equal(revoked.status, 200);
        await revoked.arrayBuffer();
        await assert.rejects(rpc(url, successor.credential).list(), { code: "UNAUTHORIZED" });
      }
    } finally {
      await mf.dispose();
    }
  },
);
