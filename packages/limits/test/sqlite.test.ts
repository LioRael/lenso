import { afterEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createSqliteLimitStore, limitSchema } from "../src/sqlite";
import { createLimits } from "../src/index";

const dirs: string[] = [];
const clients: Database[] = [];
afterEach(async () => {
  for (const client of clients.splice(0)) client.close();
  await Promise.all(dirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

async function fixture() {
  const dir = await mkdtemp(join(tmpdir(), "lenso-limits-"));
  dirs.push(dir);
  const file = join(dir, "limits.sqlite");
  const first = new Database(file);
  clients.push(first);
  first.exec("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=10000;");
  first.exec(await readFile(new URL("../migrations/0001_sqlite.sql", import.meta.url), "utf8"));
  const second = new Database(file);
  clients.push(second);
  second.exec("PRAGMA busy_timeout=10000;");
  const stores = [
    createSqliteLimitStore(drizzle(first, { schema: limitSchema })),
    createSqliteLimitStore(drizzle(second, { schema: limitSchema })),
  ];
  return { file, first, second, stores };
}

test("file-backed SQLite store enforces counters and weighted leases across clients", async () => {
  const { stores } = await fixture();
  const scope = { instance: "app", tenant: "one", key: "shared" };
  const consume = { scope, capacity: 12, quantity: 1, periodMs: 31_622_400_000 };
  const decisions = await Promise.all(
    Array.from({ length: 30 }, (_, i) => stores[i % 2]!.consume("rate", consume)),
  );
  expect(decisions.filter((d) => d.allowed)).toHaveLength(12);
  await expect(stores[0]!.consume("rate", { ...consume, capacity: 13 })).rejects.toThrow();
  const acquire = { scope, capacity: 10, quantity: 4, ttlMs: 60_000 };
  const leases = await Promise.all(
    Array.from({ length: 8 }, (_, i) => stores[i % 2]!.acquire(acquire, `t${i}`)),
  );
  expect(leases.filter((lease) => lease.allowed)).toHaveLength(2);
  expect(await stores[0]!.renew(scope, "missing", 1000)).toBeNull();
  await stores[0]!.release(scope, "t0");
  await stores[1]!.release(scope, "t0");
  expect((await stores[1]!.acquire(acquire, "replacement")).allowed).toBe(true);
  expect(
    (await stores[0]!.acquire({ ...acquire, scope: { ...scope, tenant: "other" } }, "isolated"))
      .allowed,
  ).toBe(true);
});

test("four gated Bun processes admit exactly shared capacity, not per-process capacity", async () => {
  const { file } = await fixture();
  const worker = fileURLToPath(new URL("./sqlite-worker.ts", import.meta.url));
  const ready = Array.from({ length: 4 }, () => Promise.withResolvers<void>());
  const children = ready.map((gate, index) =>
    Bun.spawn({
      cmd: [process.execPath, worker, file, String(index)],
      stdout: "pipe",
      stderr: "pipe",
      ipc(message) {
        if (message === "ready") gate.resolve();
      },
    }),
  );
  try {
    await Promise.all(
      children.map((child, index) =>
        Promise.race([
          ready[index]!.promise,
          child.exited.then(async () => {
            throw new Error(await new Response(child.stderr).text());
          }),
        ]),
      ),
    );
    for (const child of children) child.send("go");
    const outputs = await Promise.all(
      children.map(async (child) => {
        const [code, stdout, stderr] = await Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]);
        expect(stderr).toBe("");
        expect(code).toBe(0);
        return JSON.parse(stdout) as { rate: number; quota: number; leases: number };
      }),
    );
    expect(outputs.reduce((sum, output) => sum + output.rate, 0)).toBe(75);
    expect(outputs.reduce((sum, output) => sum + output.quota, 0)).toBe(50);
    expect(outputs.reduce((sum, output) => sum + output.leases, 0)).toBe(5);
  } finally {
    for (const child of children) if (child.exitCode === null) child.kill();
    await Promise.all(children.map((child) => child.exited));
  }
}, 20_000);

test("expiry, old token, duplicate release, oversize and isolation use real SQLite", async () => {
  const { stores } = await fixture();
  const first = stores[0]!;
  const second = stores[1]!;
  const scope = { instance: "expiry", tenant: "a", key: "jobs" };
  const acquire = { scope, capacity: 2, quantity: 2, ttlMs: 15 };
  const old = (await first.acquire(acquire, "old")).lease!;
  await Bun.sleep(25);
  const replacement = (await second.acquire({ ...acquire, ttlMs: 1000 }, "new")).lease!;
  expect(replacement.token).toBe("new");
  expect(await first.renew(scope, old.token, 1000)).toBeNull();
  await first.release(scope, old.token);
  await first.release(scope, old.token);
  expect((await first.acquire(acquire, "extra")).allowed).toBe(false);
  expect((await second.renew(scope, replacement.token, 1000))!.token).toBe("new");
  await second.release(scope, replacement.token);
  expect((await first.acquire(acquire, "available")).allowed).toBe(true);
  expect((await second.acquire({ ...acquire, quantity: 2_147_483_647 }, "oversize")).reason).toBe(
    "too-large",
  );
  expect(
    (await second.acquire({ ...acquire, scope: { ...scope, instance: "other" } }, "isolated"))
      .allowed,
  ).toBe(true);
  const counter = { scope, capacity: 2, quantity: 2, periodMs: 1000 };
  expect((await first.consume("rate", counter)).remaining).toBe(0);
  expect((await second.consume("rate", { ...counter, quantity: 3 })).retryAfter).toBeNull();
  expect((await second.consume("quota", counter)).allowed).toBe(true);
  expect(
    (await second.consume("rate", { ...counter, scope: { ...scope, tenant: "b" } })).allowed,
  ).toBe(true);
});

test("persisted clocks clamp backward time for windows and lease expiry", async () => {
  const { first, stores } = await fixture();
  const scope = { instance: "clock", tenant: "a", key: "clamped" };
  const counter = { scope, capacity: 1, quantity: 1, periodMs: 1000 };
  await stores[0]!.consume("rate", counter);
  const key = JSON.stringify([scope.instance, scope.tenant, scope.key]);
  const future = Date.now() + 60_000;
  const start = Math.floor(future / 1000) * 1000;
  first
    .query("UPDATE lenso_limit_counters SET last_now=?, window_start=?, used=1 WHERE scope=?")
    .run(future, start, key);
  expect(await stores[1]!.consume("rate", counter)).toMatchObject({
    allowed: false,
    remaining: 0,
    retryAfter: start + 1000 - future,
  });
  const lease = (await stores[0]!.acquire({ scope, capacity: 1, quantity: 1, ttlMs: 1000 }, "old"))
    .lease!;
  first
    .query("UPDATE lenso_limit_concurrency SET last_now=? WHERE scope=?")
    .run(lease.expiresAt, key);
  expect(await stores[1]!.renew(scope, lease.token, 1000)).toBeNull();
  expect(
    (await stores[1]!.acquire({ scope, capacity: 1, quantity: 1, ttlMs: 1000 }, "replacement"))
      .allowed,
  ).toBe(true);
});

test("SQLite writer contention invokes the explicit application failure policy", async () => {
  const { first, second, stores } = await fixture();
  second.exec("PRAGMA busy_timeout=0");
  const scope = { instance: "busy", tenant: "a", key: "blocked" };
  const counter = { scope, capacity: 1, quantity: 1, periodMs: 1000 };
  const acquire = { scope, capacity: 1, quantity: 1, ttlMs: 60_000 };
  const lease = (await stores[1]!.acquire(acquire, "before-lock")).lease!;
  first.exec("BEGIN IMMEDIATE");
  try {
    for (const failurePolicy of ["allow", "deny", "throw"] as const) {
      const limits = createLimits({ store: stores[1]!, config: { failurePolicy } });
      try {
        if (failurePolicy === "throw") {
          await expect(limits.consumeRate(counter)).rejects.toMatchObject({
            code: "backend-failure",
          });
          await expect(limits.acquire(acquire)).rejects.toMatchObject({
            code: "backend-failure",
          });
        } else {
          expect(await limits.consumeRate(counter)).toEqual({
            allowed: failurePolicy === "allow",
            remaining: null,
            retryAfter: null,
            reason: "backend-failure",
          });
          expect(await limits.acquire(acquire)).toEqual({
            allowed: failurePolicy === "allow",
            remaining: null,
            retryAfter: null,
            reason: "backend-failure",
            lease: null,
          });
        }
        await expect(limits.renew(lease, 1000)).rejects.toThrow();
        await expect(limits.release(lease)).rejects.toThrow();
      } finally {
        await limits.close();
      }
    }
  } finally {
    first.exec("ROLLBACK");
  }
  expect((await stores[1]!.consume("rate", counter)).remaining).toBe(0);
  expect((await stores[1]!.renew(scope, lease.token, 1000))!.token).toBe(lease.token);
  await stores[1]!.release(scope, lease.token);
});
