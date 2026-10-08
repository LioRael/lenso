import { SQL } from "bun";
import { expect, test } from "bun:test";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sql";
import { createPostgresLimitStore } from "../src/postgres";
import { createLimits, createLimitsPlugin } from "../src/index";
import { defineApp, startApp } from "@lenso/core";
import { createBunSqlPlugin } from "@lenso/db/bun-sql";

const required = process.env.LENSO_REQUIRE_POSTGRES === "1";
const binaries: Record<string, string> = {};
for (const name of ["initdb", "pg_ctl"] as const) {
  const path = Bun.which(name);
  if (path)
    try {
      await access(path, constants.X_OK);
      binaries[name] = path;
    } catch {}
}
const missing = ["initdb", "pg_ctl"].filter((name) => !binaries[name]);
if (missing.length && required)
  throw new Error(`Required PostgreSQL binaries unavailable: ${missing.join(", ")}`);
if (missing.length)
  console.warn(`Skipping PostgreSQL integration: ${missing.join(", ")} unavailable`);
const pgTest = missing.length ? test.skip : test;

async function command(args: string[]) {
  const p = Bun.spawn(args, { stdout: "pipe", stderr: "pipe" });
  const [code, out, err] = await Promise.all([
    p.exited,
    new Response(p.stdout).text(),
    new Response(p.stderr).text(),
  ]);
  if (code) throw new Error(`${args[0]} failed: ${out}${err}`);
}
async function fixture(
  run: (
    owner: SQL,
    stores: ReturnType<typeof createPostgresLimitStore>[],
    connections: SQL[],
  ) => Promise<void>,
) {
  const root = await mkdtemp(
    join(process.env.DELTA_SCRATCH_DIR ?? dirname(import.meta.dir), ".limits-pg-"),
  );
  const data = join(root, "data");
  const clients: SQL[] = [];
  let started = false;
  try {
    await command([
      binaries.initdb!,
      "-D",
      data,
      "-U",
      "limits_test",
      "--auth=trust",
      "--no-locale",
    ]);
    const server = createServer();
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    const port = (server.address() as { port: number }).port;
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
    started = true;
    await command([
      binaries.pg_ctl!,
      "-D",
      data,
      "-l",
      join(root, "postgres.log"),
      "-w",
      "-o",
      `-h 127.0.0.1 -p ${port} -k '' -c fsync=off`,
      "start",
    ]);
    const connect = () => {
      const client = new SQL({
        adapter: "postgres",
        hostname: "127.0.0.1",
        port,
        username: "limits_test",
        password: "",
        database: "postgres",
        max: 1,
        tls: false,
      });
      clients.push(client);
      return client;
    };
    const owner = connect();
    await owner.unsafe(
      await readFile(new URL("../migrations/0001_postgres.sql", import.meta.url), "utf8"),
    );
    const connections = [connect(), connect()];
    for (const client of clients) await client`SET statement_timeout = '8s'`;
    await connections[0]!`SET application_name = 'limits-first'`;
    await connections[1]!`SET application_name = 'limits-second'`;
    await run(
      owner,
      connections.map((client) => createPostgresLimitStore(drizzle(client))),
      connections,
    );
  } finally {
    await Promise.allSettled(clients.map((client) => client.close({ timeout: 1 })));
    try {
      if (started) await command([binaries.pg_ctl!, "-D", data, "-w", "-m", "immediate", "stop"]);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }
}

pgTest(
  "PostgreSQL isolates concurrent counter and weighted lease operations",
  async () => {
    await fixture(async (_owner, stores) => {
      const scope = { instance: "pg", tenant: "one", key: "capacity" };
      const request = { scope, capacity: 7, quantity: 1, periodMs: 31_622_400_000 };
      const decisions = await Promise.all(
        Array.from({ length: 20 }, (_, i) => stores[i % 2]!.consume("rate", request)),
      );
      expect(decisions.filter((d) => d.allowed)).toHaveLength(7);
      expect((await stores[0]!.consume("quota", request)).allowed).toBe(true);
      expect(
        (await stores[0]!.consume("rate", { ...request, quantity: 2_147_483_647 })).reason,
      ).toBe("too-large");
      await expect(stores[1]!.consume("rate", { ...request, capacity: 8 })).rejects.toMatchObject({
        code: "policy-conflict",
      });
      await expect(
        stores[1]!.consume("rate", { ...request, periodMs: 1000 }),
      ).rejects.toMatchObject({ code: "policy-conflict" });
      const leaseInput = { scope, capacity: 10, quantity: 4, ttlMs: 60_000 };
      const acquired = await Promise.all(
        Array.from({ length: 8 }, (_, i) => stores[i % 2]!.acquire(leaseInput, `token-${i}`)),
      );
      expect(acquired.filter((item) => item.allowed)).toHaveLength(2);
      expect((await stores[0]!.acquire({ ...leaseInput, quantity: 11 }, "too-big")).reason).toBe(
        "too-large",
      );
      await expect(
        stores[1]!.acquire({ ...leaseInput, capacity: 9 }, "conflict"),
      ).rejects.toThrow();
      const winner = acquired.find((item) => item.lease)!;
      expect(await stores[0]!.renew(scope, "unknown", 1000)).toBeNull();
      await stores[0]!.release(scope, winner.lease!.token);
      await stores[1]!.release(scope, winner.lease!.token);
      expect((await stores[1]!.acquire(leaseInput, "replacement")).allowed).toBe(true);
      expect(
        (
          await stores[0]!.acquire(
            { ...leaseInput, scope: { ...scope, tenant: "other" } },
            "isolated",
          )
        ).allowed,
      ).toBe(true);
      expect(
        (
          await stores[0]!.acquire(
            { ...leaseInput, scope: { ...scope, instance: "other" } },
            "isolated",
          )
        ).allowed,
      ).toBe(true);
      expect(
        (await stores[0]!.consume("rate", { ...request, scope: { ...scope, tenant: "other" } }))
          .allowed,
      ).toBe(true);
      expect(
        (await stores[0]!.consume("rate", { ...request, scope: { ...scope, instance: "other" } }))
          .allowed,
      ).toBe(true);
    });
  },
  30_000,
);

pgTest(
  "PostgreSQL persisted clocks preserve boundaries and reject stale lease holders",
  async () => {
    await fixture(async (owner, stores) => {
      const scope = { instance: "pg-clock", tenant: "one", key: "boundary" };
      const key = JSON.stringify([scope.instance, scope.tenant, scope.key]);
      const input = { scope, capacity: 1, quantity: 1, periodMs: 1000 };
      await stores[0]!.consume("rate", input);
      const future = Date.now() + 60_000;
      const window = Math.floor(future / 1000) * 1000;
      await owner`UPDATE lenso_limit_counters SET last_now=${future}, window_start=${window}, used=1 WHERE scope=${key}`;
      expect(await stores[1]!.consume("rate", input)).toMatchObject({
        allowed: false,
        remaining: 0,
        retryAfter: window + 1000 - future,
      });
      await owner`UPDATE lenso_limit_counters SET window_start=${window - 1000} WHERE scope=${key}`;
      expect((await stores[1]!.consume("rate", input)).allowed).toBe(true);
      const leaseInput = { scope, capacity: 1, quantity: 1, ttlMs: 1000 };
      const old = (await stores[0]!.acquire(leaseInput, "old")).lease!;
      await owner`UPDATE lenso_limit_concurrency SET last_now=${old.expiresAt} WHERE scope=${key}`;
      expect(await stores[1]!.renew(scope, old.token, 1000)).toBeNull();
      const replacement = (await stores[1]!.acquire(leaseInput, "replacement")).lease!;
      expect(replacement.token).toBe("replacement");
      await stores[0]!.release(scope, old.token);
      await stores[1]!.release(scope, old.token);
      expect((await stores[0]!.acquire(leaseInput, "denied")).allowed).toBe(false);
      expect((await stores[1]!.renew(scope, replacement.token, 1))!.expiresAt).toBe(
        replacement.expiresAt,
      );
      await stores[0]!.release({ ...scope, tenant: "wrong" }, replacement.token);
      expect((await stores[0]!.acquire(leaseInput, "still-denied")).allowed).toBe(false);
      await expect(
        stores[0]!.consume("rate", { ...input, quantity: Infinity }),
      ).rejects.toMatchObject({ code: "invalid-input" });
      await expect(
        stores[0]!.acquire({ ...leaseInput, ttlMs: 0 }, "invalid"),
      ).rejects.toMatchObject({ code: "invalid-input" });
    });
  },
  30_000,
);

async function waitBlocked(observer: SQL) {
  const deadline = Date.now() + 5000;
  while (Date.now() < deadline) {
    const [row] = await observer`SELECT count(*)::integer AS blocked FROM pg_stat_activity
      WHERE application_name='limits-second' AND wait_event_type='Lock'`;
    if (row.blocked > 0) return;
    await Bun.sleep(5);
  }
  throw new Error("PostgreSQL contender did not reach the row lock");
}

pgTest(
  "PostgreSQL samples time after row-lock waits, not transaction start",
  async () => {
    await fixture(async (owner, stores, clients) => {
      const scope = { instance: "pg-wait", tenant: "one", key: "expiry" };
      const key = JSON.stringify([scope.instance, scope.tenant, scope.key]);
      const input = { scope, capacity: 1, quantity: 1, ttlMs: 1000 };
      const old = (await stores[0]!.acquire(input, "old")).lease!;
      const locked = Promise.withResolvers<void>();
      const unlock = Promise.withResolvers<void>();
      const lock = owner.begin(async (tx) => {
        await tx`SELECT scope FROM lenso_limit_concurrency WHERE scope=${key} FOR UPDATE`;
        locked.resolve();
        await unlock.promise;
      });
      void lock.catch((error) => locked.reject(error));
      await locked.promise;
      const waitingRenewal = stores[1]!.renew(scope, old.token, 60_000);
      void waitingRenewal.catch(() => {});
      try {
        await waitBlocked(clients[0]!);
        expect(Date.now()).toBeLessThan(old.expiresAt);
        await Bun.sleep(Math.max(0, old.expiresAt - Date.now()) + 25);
      } finally {
        unlock.resolve();
        await lock;
      }
      expect(await waitingRenewal).toBeNull();
      expect((await stores[0]!.acquire(input, "replacement")).allowed).toBe(true);
    });
  },
  30_000,
);

pgTest(
  "PostgreSQL lock timeouts keep fault policy explicit and do not consume",
  async () => {
    await fixture(async (owner, stores, clients) => {
      const scope = { instance: "pg-timeout", tenant: "one", key: "blocked" };
      const key = JSON.stringify([scope.instance, scope.tenant, scope.key]);
      const input = { scope, capacity: 10, quantity: 1, periodMs: 31_622_400_000 };
      await stores[0]!.consume("rate", input);
      await clients[1]!`SET lock_timeout = '50ms'`;
      const locked = Promise.withResolvers<void>();
      const unlock = Promise.withResolvers<void>();
      const lock = owner.begin(async (tx) => {
        await tx`SELECT scope FROM lenso_limit_counters WHERE scope=${key} AND kind='rate' FOR UPDATE`;
        locked.resolve();
        await unlock.promise;
      });
      void lock.catch((error) => locked.reject(error));
      await locked.promise;
      try {
        // A different scope must remain usable while this one is held.
        expect(
          (await stores[1]!.consume("rate", { ...input, scope: { ...scope, key: "independent" } }))
            .allowed,
        ).toBe(true);
        for (const failurePolicy of ["allow", "deny", "throw"] as const) {
          const service = createLimits({ store: stores[1]!, config: { failurePolicy } });
          try {
            if (failurePolicy === "throw")
              await expect(service.consumeRate(input)).rejects.toMatchObject({
                code: "backend-failure",
              });
            else
              expect(await service.consumeRate(input)).toMatchObject({
                allowed: failurePolicy === "allow",
                remaining: null,
                retryAfter: null,
                reason: "backend-failure",
              });
          } finally {
            await service.close();
          }
        }
      } finally {
        unlock.resolve();
        await lock;
      }
      expect((await stores[1]!.consume("rate", input)).remaining).toBe(8);
    });
  },
  30_000,
);

pgTest(
  "PostgreSQL public DB plugin remains borrowed after limits shutdown",
  async () => {
    await fixture(async (owner) => {
      const db = createBunSqlPlugin({ id: "pg-db", schema: {}, client: owner });
      const limits = createLimitsPlugin({
        id: "pg-limits",
        requires: [db],
        config: { failurePolicy: "throw" },
        connect: (context) => createPostgresLimitStore(context.get(db)),
      });
      const app = await startApp(defineApp({ plugins: [db, limits] }));
      const scope = { instance: "pg-integration", tenant: "one", key: "shutdown" };
      const input = { scope, capacity: 1, quantity: 1, ttlMs: 60_000 };
      try {
        expect((await app.get(limits).acquire(input)).allowed).toBe(true);
      } finally {
        await app.stop();
      }
      const [row] = await owner`SELECT 1 AS usable`;
      expect(row.usable).toBe(1);
      expect(
        (await createPostgresLimitStore(drizzle(owner)).acquire(input, "after-stop")).allowed,
      ).toBe(true);
    });
  },
  30_000,
);
