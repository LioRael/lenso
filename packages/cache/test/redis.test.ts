import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { RedisClient } from "bun";
import { createRedisCacheAdapter } from "../src/redis";
import { createCache, CacheError, type JsonValue } from "../src/index";

const redisServer = Bun.which("redis-server");
let server: ReturnType<typeof Bun.spawn> | undefined;
let firstClient: InstanceType<typeof RedisClient>;
let secondClient: InstanceType<typeof RedisClient>;
let first: ReturnType<typeof createRedisCacheAdapter>;
let second: ReturnType<typeof createRedisCacheAdapter>;
let port: number;

async function reservePort(): Promise<number> {
  const listener = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
  const chosen = listener.port;
  listener.stop();
  return chosen;
}

async function waitForRedis(): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (server?.exitCode !== null) throw new Error("owned Redis server exited during startup");
    try {
      const socket = await Bun.connect({
        hostname: "127.0.0.1",
        port,
        socket: {
          data() {},
          open(connection) {
            connection.end();
          },
        },
      });
      socket.end();
      return;
    } catch {
      await Bun.sleep(50);
    }
  }
  throw new Error("owned Redis server did not become ready");
}

describe.skipIf(!redisServer)("Redis cache adapter", () => {
  beforeAll(async () => {
    port = await reservePort();
    server = Bun.spawn(
      [
        redisServer!,
        "--bind",
        "127.0.0.1",
        "--port",
        String(port),
        "--save",
        "",
        "--appendonly",
        "no",
        "--protected-mode",
        "yes",
      ],
      { stdout: "ignore", stderr: "ignore" },
    );
    try {
      await waitForRedis();
      const connection = {
        autoReconnect: false,
        enableOfflineQueue: false,
        maxRetries: 0,
        connectionTimeout: 500,
      };
      firstClient = new RedisClient(`redis://127.0.0.1:${port}`, connection);
      secondClient = new RedisClient(`redis://127.0.0.1:${port}`, connection);
      await firstClient.connect();
      await secondClient.connect();
      const info = String(await firstClient.send("INFO", ["server"]));
      if (!info.includes(`process_id:${server.pid}\r\n`))
        throw new Error("Redis endpoint does not belong to this test");
      first = createRedisCacheAdapter({ client: firstClient, prefix: "cache-test" });
      second = createRedisCacheAdapter({ client: secondClient, prefix: "cache-test" });
    } catch (error) {
      server.kill();
      throw error;
    }
  }, 10_000);

  afterAll(async () => {
    firstClient?.close();
    secondClient?.close();
    if (server) {
      server.kill();
      await server.exited;
    }
  });

  test("shares values across clients and applies Redis TTL", async () => {
    const generation = await first.generation("shared");
    expect(await first.set("shared", generation, "ttl", "value", 150)).toBe(true);
    expect(await second.getMany("shared", generation, ["ttl"])).toEqual(["value"]);
    await Bun.sleep(200);
    expect(await second.getMany("shared", generation, ["ttl"])).toEqual([null]);
  });

  test("isolates scopes, invalidates by generation, and rejects stale writes", async () => {
    const generation = await first.generation("one");
    const otherGeneration = await first.generation("two");
    await first.set("one", generation, "k", "old", 1_000);
    await first.set("two", otherGeneration, "k", "other", 1_000);

    await first.invalidate("one");
    expect(await first.getMany("one", generation, ["k"])).toEqual([null]);
    expect(await first.set("one", generation, "k", "stale", 1_000)).toBe(false);
    const nextGeneration = await first.generation("one");
    expect(await first.getMany("one", nextGeneration, ["k"])).toEqual([null]);
    expect(await first.set("one", nextGeneration, "k", "new", 1_000)).toBe(true);
    expect(await first.getMany("one", nextGeneration, ["k"])).toEqual(["new"]);
    expect(await first.getMany("one", generation, ["k"])).toEqual([null]);
    expect(await first.getMany("two", otherGeneration, ["k"])).toEqual(["other"]);
    expect(nextGeneration).not.toBe(generation);
  });

  test("preserves duplicates and isolates WRONGTYPE failures to one key", async () => {
    const generation = await first.generation("batch");
    await first.set("batch", generation, "good", "healthy", 1_000);
    // A distinct malformed Redis value is made via the adapter's collision-safe data-key layout.
    const scope = Buffer.from("batch").toString("hex");
    const epoch = Buffer.from(generation).toString("hex");
    const item = Buffer.from("bad").toString("hex");
    const wrongTypeKey = `cache-test:data:${scope.length}:${scope}:${epoch.length}:${epoch}:${item.length}:${item}`;
    await firstClient.send("LPUSH", [wrongTypeKey, "x"]);
    const results = await second.getMany("batch", generation, ["good", "bad", "good"]);
    expect(results[0]).toBe("healthy");
    expect(results[1]).toBeInstanceOf(CacheError);
    expect((results[1] as CacheError).code).toBe("backend");
    expect(results[2]).toBe("healthy");
  });

  test("real driver backs typed null, zero TTL, corrupt JSON and borrowed lifetime", async () => {
    const a = createCache({ adapter: first, namespace: "service" });
    const b = createCache({ adapter: second, namespace: "service" });
    try {
      await a.set("null", null);
      expect(await b.get("null")).toEqual({ status: "hit", value: null });
      expect(await a.set("null", "not stored", { ttlMs: 0 })).toEqual({ outcome: "skipped" });
      expect((await b.get("null")).status).toBe("miss");
      const namespace = JSON.stringify(["service"]);
      const generation = await first.generation(namespace);
      await first.set(namespace, generation, "corrupt", "not JSON", 1000);
      expect(await b.get("corrupt")).toEqual({ status: "miss", reason: "corrupt" });
      a.close();
      expect(await firstClient.send("PING", [])).toBe("PONG");
      await b.set("usable", "borrowed");
      expect(await b.get("usable")).toEqual({ status: "hit", value: "borrowed" });
    } finally {
      a.close();
      b.close();
    }
  });

  test("namespace invalidation fences another instance's in-flight getOrSet load", async () => {
    const a = createCache({ adapter: first, namespace: "loads" });
    const b = createCache({ adapter: second, namespace: "loads" });
    let started!: () => void;
    const start = new Promise<void>((resolve) => {
      started = resolve;
    });
    let finish!: (value: JsonValue) => void;
    const end = new Promise<JsonValue>((resolve) => {
      finish = resolve;
    });
    try {
      const load = a.getOrSet("key", async () => {
        started();
        return end;
      });
      await start;
      await b.invalidate();
      finish("old");
      expect(await load).toBe("old");
      expect((await b.get("key")).status).toBe("miss");
      expect(await b.getOrSet("key", async () => "fresh")).toBe("fresh");
      expect(await a.get("key")).toEqual({ status: "hit", value: "fresh" });
    } finally {
      finish("cleanup");
      a.close();
      b.close();
    }
  });

  test("validates adapter input before sending backend commands", async () => {
    for (const prefix of ["", "unsafe/prefix", "x".repeat(257)]) {
      expect(() => createRedisCacheAdapter({ client: firstClient, prefix })).toThrow(CacheError);
    }
    const generation = await first.generation("limits");
    for (const ttlMs of [-1, 0, 0.5, Infinity, 86_400_001]) {
      await expect(first.set("limits", generation, "k", "value", ttlMs)).rejects.toMatchObject({
        code: "invalid-input",
      });
    }
    await expect(first.getMany("limits", generation, Array(101).fill("k"))).rejects.toMatchObject({
      code: "invalid-input",
    });
  });

  test("sanitizes real backend failure with explicit fail-open and fail-closed policy", async () => {
    const generation = await first.generation("failure");
    const strict = createCache({ adapter: first, namespace: "failure" });
    const open = createCache({ adapter: second, namespace: "failure", failureMode: "fail-open" });
    server!.kill();
    await server!.exited;
    await expect(first.getMany("failure", generation, ["key"])).rejects.toMatchObject({
      name: "CacheError",
      code: "backend",
      message: "Cache operation failed (backend)",
    });
    try {
      await expect(strict.get("key")).rejects.toMatchObject({ code: "backend" });
      expect(await open.get("key")).toEqual({ status: "miss", reason: "backend" });
      expect(await open.set("key", "value")).toEqual({ outcome: "bypassed" });
      expect(await open.getOrSet("key", async () => "authoritative")).toBe("authoritative");
    } finally {
      strict.close();
      open.close();
    }
  });
});
