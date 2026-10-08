import { expect, test } from "bun:test";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { drizzle } from "drizzle-orm/d1";
import { createD1LimitStore, d1LimitSchema } from "../src/d1";
import { createLimits, type Acquisition, type Decision } from "../src/index";

async function fixture(
  run: (context: {
    first: ReturnType<typeof createD1LimitStore>;
    second: ReturnType<typeof createD1LimitStore>;
    binding: Awaited<ReturnType<Miniflare["getD1Database"]>>;
    origin: URL;
  }) => Promise<void>,
) {
  const built = await Bun.build({
    entrypoints: [new URL("./d1-worker.ts", import.meta.url).pathname],
    target: "browser",
  });
  expect(built.success).toBe(true);
  const script = await built.outputs[0]!.text();
  expect(script).not.toMatch(/(?:from\s*|import\s*\()\s*["'](?:bun:|node:fs)/);
  const directory = await mkdtemp(join(tmpdir(), "lenso-limits-d1-"));
  let mf: Miniflare | undefined;
  try {
    mf = new Miniflare(
      convertV4MiniflareOptions({
        modules: true,
        script,
        compatibilityDate: "2026-10-08",
        host: "127.0.0.1",
        d1Databases: ["DB"],
        d1Persist: directory,
      }),
    );
    const origin = await mf.ready;
    const binding = await mf.getD1Database("DB");
    const migration = await readFile(new URL("../migrations/0001_d1.sql", import.meta.url), "utf8");
    const statements = migration
      .split(";")
      .map((statement) => statement.trim())
      .filter(Boolean);
    await binding.batch(statements.map((statement) => binding.prepare(statement)));
    await run({
      first: createD1LimitStore(drizzle(binding, { schema: d1LimitSchema })),
      second: createD1LimitStore(drizzle(await mf.getD1Database("DB"))),
      binding,
      origin,
    });
  } finally {
    await mf?.dispose();
    await rm(directory, { recursive: true, force: true });
  }
}

const scope = { instance: "d1", tenant: "one", key: "atomic" };
const counter = { scope, capacity: 12, quantity: 1, periodMs: 31_622_400_000 };
const acquire = { scope, capacity: 7, quantity: 2, ttlMs: 60_000 };

test("workerd D1 concurrent batches admit only shared request, quota and lease capacity", async () => {
  await fixture(async ({ first, second }) => {
    const stores = [first, second];
    const decisions: Decision[] = await Promise.all(
      Array.from({ length: 30 }, (_, i) => stores[i % 2]!.consume("rate", counter)),
    );
    expect(decisions.filter((decision) => decision.allowed)).toHaveLength(12);
    expect(Math.min(...decisions.map((decision) => decision.remaining!))).toBe(0);
    expect((await second.consume("quota", counter)).allowed).toBe(true);
    expect((await first.consume("rate", { ...counter, quantity: 2_147_483_647 })).reason).toBe(
      "too-large",
    );
    await expect(second.consume("rate", { ...counter, capacity: 13 })).rejects.toMatchObject({
      code: "policy-conflict",
    });
    await expect(second.consume("rate", { ...counter, periodMs: 1000 })).rejects.toMatchObject({
      code: "policy-conflict",
    });
    for (const isolated of [
      { ...scope, tenant: "two" },
      { ...scope, instance: "other" },
    ])
      expect((await second.consume("rate", { ...counter, scope: isolated })).allowed).toBe(true);
    const leases: Acquisition[] = await Promise.all(
      Array.from({ length: 10 }, (_, i) => stores[i % 2]!.acquire(acquire, `lease:${i}`)),
    );
    expect(leases.filter((result) => result.allowed)).toHaveLength(3);
    expect(await first.acquire({ ...acquire, quantity: 8 }, "oversize")).toMatchObject({
      allowed: false,
      remaining: 1,
      retryAfter: null,
      reason: "too-large",
      lease: null,
    });
    await expect(first.acquire({ ...acquire, capacity: 10 }, "conflict")).rejects.toMatchObject({
      code: "policy-conflict",
    });
    const winner = leases.find((result) => result.lease)!.lease!;
    await first.release(scope, winner.token);
    await second.release(scope, winner.token);
    expect((await second.acquire(acquire, "replacement")).allowed).toBe(true);
    expect(
      (await first.acquire({ ...acquire, scope: { ...scope, tenant: "two" } }, "isolated")).allowed,
    ).toBe(true);
    expect(
      (await first.acquire({ ...acquire, scope: { ...scope, instance: "other" } }, "isolated"))
        .allowed,
    ).toBe(true);
  });
}, 30_000);

test("workerd D1 expiration, stale tokens, quantity retry and clock boundaries", async () => {
  await fixture(async ({ first, second, binding }) => {
    const key = JSON.stringify([scope.instance, scope.tenant, scope.key]);
    const input = { ...acquire, capacity: 5, ttlMs: 1000 };
    const old = (await first.acquire(input, "old")).lease!;
    const next = (await first.acquire({ ...input, ttlMs: 2000 }, "next")).lease!;
    const denied = await second.acquire({ ...input, quantity: 4 }, "denied");
    expect(denied.allowed).toBe(false);
    expect(denied.retryAfter).toBeGreaterThan(0);
    // A fixture-only forward high-water clock expires old without changing backend time.
    await binding
      .prepare("UPDATE lenso_d1_limit_concurrency SET last_now=? WHERE scope=?")
      .bind(old.expiresAt, key)
      .run();
    expect(await first.renew(scope, old.token, 1000)).toBeNull();
    const replacement = (await second.acquire(input, "replacement")).lease!;
    expect(replacement.token).toBe("replacement");
    await first.release(scope, old.token);
    await first.release(scope, old.token);
    expect((await second.acquire(input, "no-room")).allowed).toBe(false);
    const renewed = (await first.renew(scope, replacement.token, 1))!;
    expect(renewed.expiresAt).toBe(replacement.expiresAt); // Never shorten.
    expect(await second.renew({ ...scope, tenant: "wrong" }, replacement.token, 1000)).toBeNull();
    await first.release({ ...scope, tenant: "wrong" }, replacement.token);
    await first.release(scope, next.token);
    expect((await second.acquire(input, "available")).allowed).toBe(true);

    const counterInput = { ...counter, capacity: 1, periodMs: 1000 };
    await first.consume("rate", counterInput);
    const future = Date.now() + 60_000;
    const windowStart = Math.floor(future / 1000) * 1000;
    await binding
      .prepare(
        "UPDATE lenso_d1_limit_counters SET last_now=?, window_start=?, used=1 WHERE scope=? AND kind='rate'",
      )
      .bind(future, windowStart, key)
      .run();
    expect(await second.consume("rate", counterInput)).toMatchObject({
      allowed: false,
      remaining: 0,
      retryAfter: windowStart + 1000 - future,
    });
    await binding
      .prepare("UPDATE lenso_d1_limit_counters SET window_start=? WHERE scope=? AND kind='rate'")
      .bind(windowStart - 1000, key)
      .run();
    expect((await second.consume("rate", counterInput)).allowed).toBe(true);
    // Invalid values must be rejected before sending statements.
    await expect(first.consume("rate", { ...counterInput, quantity: NaN })).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(first.acquire({ ...input, ttlMs: 0 }, "invalid")).rejects.toMatchObject({
      code: "invalid-input",
    });
  });
}, 30_000);

test("workerd D1 rolls back failed batches and leaves backend policy explicit", async () => {
  await fixture(async ({ first, binding }) => {
    const key = JSON.stringify([scope.instance, scope.tenant, scope.key]);
    const old = (await first.acquire({ ...acquire, ttlMs: 1000 }, "old")).lease!;
    await binding
      .prepare("UPDATE lenso_d1_limit_concurrency SET last_now=? WHERE scope=?")
      .bind(old.expiresAt, key)
      .run();
    await binding
      .prepare(`CREATE TRIGGER fixture_lease_failure BEFORE INSERT ON lenso_d1_limit_leases
      BEGIN SELECT RAISE(ABORT, 'fixture backend failure'); END`)
      .run();
    try {
      for (const failurePolicy of ["allow", "deny", "throw"] as const) {
        const service = createLimits({ store: first, config: { failurePolicy } });
        try {
          if (failurePolicy === "throw")
            await expect(service.acquire(acquire)).rejects.toMatchObject({
              code: "backend-failure",
            });
          else
            expect(await service.acquire(acquire)).toMatchObject({
              allowed: failurePolicy === "allow",
              remaining: null,
              retryAfter: null,
              reason: "backend-failure",
              lease: null,
            });
        } finally {
          await service.close();
        }
      }
      const result = await binding
        .prepare("SELECT token FROM lenso_d1_limit_leases WHERE scope=?")
        .bind(key)
        .all<{ token: string }>();
      expect(result.results.map((row) => row.token)).toEqual(["old"]); // Prune rolled back too.
    } finally {
      await binding.prepare("DROP TRIGGER fixture_lease_failure").run();
    }
    expect((await first.acquire(acquire, "recovered")).allowed).toBe(true);
  });
}, 30_000);

test("real Workers request assembly uses exact DB/limits instances across requests", async () => {
  await fixture(async ({ origin }) => {
    const responses = await Promise.all(
      Array.from({ length: 25 }, () =>
        fetch(origin, {
          method: "POST",
          body: JSON.stringify(counter),
          headers: { "content-type": "application/json" },
        }).then(async (response) => {
          expect(response.status).toBe(200);
          return (await response.json()) as Decision;
        }),
      ),
    );
    expect(responses.filter((decision) => decision.allowed)).toHaveLength(12);
    expect(responses.every((decision) => decision.remaining !== null)).toBe(true);
  });
}, 30_000);
