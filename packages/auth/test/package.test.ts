import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { os, call } from "@orpc/server";
import { readFile } from "node:fs/promises";
import { startApp } from "@lenso/core";
import { audience, AuthError, createAuth, defineSource, realm } from "@lenso/auth";
import { createAuthPlugin } from "@lenso/auth/plugin";
import { bearerEvidence } from "@lenso/auth/fetch";
import { requiredAuth } from "@lenso/auth/orpc";
import { createManagedSessions, sessionLifetime } from "@lenso/auth/sessions";
import { sqliteSessionStore } from "@lenso/auth/drizzle/sqlite";

test("built public entrypoints share error identity and support custom subjects without a User table", async () => {
  const sqlite = new Database(":memory:");
  let enabled = true;
  const plugin = createAuthPlugin({
    id: "employees-auth",
    setup(ctx) {
      const login = defineSource({
        async verify(evidence: { proof: string }) {
          return evidence.proof === "verified-by-application"
            ? { status: "verified", subjectId: "employee:123" }
            : { status: "rejected" };
        },
      });
      const sessions = createManagedSessions({
        realmId: "employees",
        login,
        store: sqliteSessionStore(drizzle(sqlite)),
        lifetime: sessionLifetime({ idle: 60_000, absolute: 3_600_000, renewAfter: 10_000 }),
        subjectActive: async (subjectId) => enabled && subjectId === "employee:123",
      });
      ctx.onCleanup(() => sessions.close());
      const auth = createAuth(realm("employees", sessions.source));
      return { ...auth, issue: sessions.issue };
    },
  });
  const migration = await readFile(
    new URL("../migrations/sqlite/0000_auth_sessions.sql", import.meta.url),
    "utf8",
  );
  sqlite.exec(migration);
  const app = await startApp({ plugins: [plugin] });
  try {
    const auth = app.get(plugin);
    const issued = await auth.issue({ proof: "verified-by-application" });
    const access = auth.for(audience("notes:read"));
    const actor = await access.required(issued.credential);
    expect(actor.subjectId).toBe("employee:123");
    const procedure = os
      .$context<{ request: Request }>()
      .use(requiredAuth(access, bearerEvidence))
      .handler(async ({ context }) => {
        await access.enforce(context.actor, {}, () => false);
      });
    const request = new Request("https://app.example", {
      headers: { authorization: `Bearer ${issued.credential}` },
    });
    await expect(call(procedure, undefined, { context: { request } })).rejects.toMatchObject({
      code: "FORBIDDEN",
      message: "Access denied",
    });
    enabled = false;
    await expect(access.required(issued.credential)).rejects.toBeInstanceOf(AuthError);
    await expect(access.required(issued.credential)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await app.stop();
    sqlite.close();
  }
});
