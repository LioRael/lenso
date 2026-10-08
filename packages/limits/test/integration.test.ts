import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { readFile } from "node:fs/promises";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { valuesSource } from "@lenso/core/config";
import { createBunSqlitePlugin } from "@lenso/db/bun-sqlite";
import { audience, createAuth, defineSource, realm, type ActorOf } from "@lenso/auth";
import { createAuthPlugin } from "@lenso/auth/plugin";
import { createLimitsPlugin } from "../src/index";
import { createSqliteLimitStore, limitSchema } from "../src/sqlite";

test("real public Config, Auth, DB and exact plugin instances share authorized admission and cleanup", async () => {
  const client = new Database();
  try {
    client.exec(await readFile(new URL("../migrations/0001_sqlite.sql", import.meta.url), "utf8"));
    const db = createBunSqlitePlugin({ id: "limit-db", schema: limitSchema, client });
    const auth = createAuthPlugin({
      id: "limit-auth",
      setup: () =>
        createAuth(
          realm(
            "users",
            defineSource({
              // Local verified credential fixtures exercise real Auth proof/enforcement, not a provider.
              async verify(evidence: string) {
                return ["fixture-alice", "fixture-bob"].includes(evidence)
                  ? { status: "verified" as const, subjectId: evidence.slice(8) }
                  : { status: "rejected" as const };
              },
            }),
          ),
        ),
    });
    const limits = createLimitsPlugin({
      id: "limits",
      requires: [db],
      config: [valuesSource({ failurePolicy: "throw" })],
      connect: (context) => createSqliteLimitStore(context.get(db)),
    });
    const business = definePlugin({
      id: "reports",
      requires: [auth, limits],
      setup(context) {
        const access = context.get(auth).for(audience("reports:run"));
        const limiter = context.get(limits);
        return {
          access,
          async run(actor: ActorOf<typeof access> | null, input: { report: string }) {
            const principal = await access.enforce(
              actor,
              undefined,
              ({ principal: candidate }) => candidate.kind === "user",
            );
            const tenant = principal.subjectId === "alice" ? "tenant-a" : "tenant-b";
            const scope = { instance: "reports", tenant, key: `run:${principal.subjectId}` };
            const decision = await limiter.consumeQuota({
              scope,
              capacity: 1,
              quantity: 1,
              periodMs: 31_622_400_000,
            });
            return { decision, report: input.report };
          },
        };
      },
    });
    const app = await startApp(
      defineApp({ plugins: [business, limits, db, auth], instanceId: "runtime-worker-1" }),
    );
    try {
      const service = app.get(business);
      const alice = await service.access.required("fixture-alice");
      const bob = await service.access.required("fixture-bob");
      expect((await service.run(alice, { report: "one" })).decision.allowed).toBe(true);
      const injected = { report: "two", tenant: "tenant-b", key: "new-key" };
      expect((await service.run(alice, injected)).decision.allowed).toBe(false);
      expect((await service.run(bob, { report: "one" })).decision.allowed).toBe(true);
      await expect(service.run({ ...alice }, { report: "forged" })).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      const held = await app.get(limits).acquire({
        scope: { instance: "reports", tenant: "tenant-a", key: "running" },
        capacity: 1,
        quantity: 1,
        ttlMs: 60_000,
      });
      expect(held.allowed).toBe(true);
    } finally {
      await app.stop();
    }
    // The DB plugin borrowed this client, so app shutdown must leave it usable.
    const row = client.query("SELECT leases FROM lenso_limit_concurrency").get() as {
      leases: string;
    };
    expect(JSON.parse(row.leases)).toEqual([]);
    expect(client.query("SELECT 1 AS usable").get()).toEqual({ usable: 1 });
  } finally {
    client.close();
  }
});

test("config preflight refuses missing policy before connecting and rollback releases leases", async () => {
  let connected = false;
  const invalid = createLimitsPlugin({
    id: "invalid",
    config: [],
    connect: () => {
      connected = true;
      throw new Error("must not connect");
    },
  });
  await expect(startApp(defineApp({ plugins: [invalid] }))).rejects.toMatchObject({
    name: "ConfigError",
  });
  expect(connected).toBe(false);

  const { createMemoryLimitStore } = await import("../src/index");
  const store = createMemoryLimitStore();
  const limits = createLimitsPlugin({
    id: "rollback-limits",
    config: { failurePolicy: "throw" },
    connect: () => store,
  });
  const input = {
    scope: { instance: "rollback", tenant: "one", key: "held" },
    capacity: 1,
    quantity: 1,
    ttlMs: 1000,
  };
  const cause = new Error("later setup failed");
  const failing = definePlugin({
    id: "failure",
    requires: [limits],
    async setup(context) {
      await context.get(limits).acquire(input);
      throw cause;
    },
  });
  await expect(startApp(defineApp({ plugins: [limits, failing] }))).rejects.toBe(cause);
  expect((await store.acquire(input, "after-rollback")).allowed).toBe(true);
});
