import { describe, expect, test } from "bun:test";
import { RedisClient } from "bun";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createRedisProvider } from "../src/redis";
import { createRealtime } from "../src/index";
import { parseCursor } from "../src/wire";
import type { Delivery, RealtimeProvider } from "../src/contracts";

const executable = Bun.which("redis-server");
if (!executable) console.warn("Redis integration skipped: redis-server executable is absent.");

async function until(check: () => boolean | Promise<boolean>, timeout = 5000): Promise<void> {
  const end = Date.now() + timeout;
  while (!(await check())) {
    if (Date.now() >= end) throw new Error("Integration condition timed out");
    await Bun.sleep(10);
  }
}

function freePort(): number {
  const socket = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
  const port = socket.port;
  socket.stop(true);
  return port;
}

async function server() {
  const directory = await mkdtemp(join(tmpdir(), "lenso-redis-"));
  const port = freePort();
  const process = Bun.spawn(
    [
      executable!,
      "--bind",
      "127.0.0.1",
      "--port",
      String(port),
      "--protected-mode",
      "yes",
      "--save",
      "",
      "--appendonly",
      "no",
      "--dir",
      directory,
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  let output = "";
  const readOutput = (async () => {
    for await (const chunk of process.stdout) output += new TextDecoder().decode(chunk);
  })();
  const readError = new Response(process.stderr).text();
  const url = `redis://127.0.0.1:${port}`;
  const admin = new RedisClient(url, { autoReconnect: false, enableOfflineQueue: false });
  const stop = async () => {
    admin.close();
    if (process.exitCode === null) process.kill("SIGTERM");
    await process.exited;
    await readOutput;
    await readError;
    await rm(directory, { recursive: true, force: true });
  };
  try {
    await until(() => output.includes("Ready to accept connections") || process.exitCode !== null);
    if (process.exitCode !== null) throw new Error(`Owned Redis failed to start: ${output}`);
    await admin.connect();
    return { url, admin, process, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}

async function listener(url: string, namespace: string) {
  const process = Bun.spawn(
    [Bun.which("bun")!, join(import.meta.dir, "listener.ts"), url, namespace],
    { stdin: "pipe", stdout: "pipe", stderr: "pipe" },
  );
  const messages: { ready?: boolean; pid?: number; delivery?: Delivery; closed?: boolean }[] = [];
  const read = (async () => {
    let buffer = "";
    const decoder = new TextDecoder();
    for await (const chunk of process.stdout) {
      buffer += decoder.decode(chunk, { stream: true });
      let newline: number;
      while ((newline = buffer.indexOf("\n")) !== -1) {
        messages.push(JSON.parse(buffer.slice(0, newline)));
        buffer = buffer.slice(newline + 1);
      }
    }
  })();
  const errors = new Response(process.stderr).text();
  const stop = async () => {
    if (process.exitCode === null) {
      process.stdin.write("close\n");
      process.stdin.end();
      try {
        await until(() => process.exitCode !== null, 2000);
      } catch {
        process.kill("SIGTERM");
      }
    }
    await process.exited;
    await read;
    const stderr = await errors;
    if (stderr) throw new Error(stderr);
  };
  try {
    await until(() => messages.some((message) => message.ready) || process.exitCode !== null);
    if (process.exitCode !== null) throw new Error(await errors);
    return { process, messages, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}

const namespace = () => `test:${crypto.randomUUID()}`;
const watermarkKey = (ns: string, topic: string) => `${ns.length}:${ns}:watermark:${topic}`;
const noDelivery = () => {};
const noFailure = () => {};
const clients = async (admin: RedisClient) =>
  String(await admin.send("CLIENT", ["LIST"]))
    .trim()
    .split("\n").length;

describe.skipIf(!executable)("owned standalone Redis integration", () => {
  test("independent Realtime instances broadcast updates and resource revocation through Redis", async () => {
    const redis = await server();
    const ns = namespace();
    let a: Awaited<ReturnType<typeof createRealtime>> | undefined;
    let b: Awaited<ReturnType<typeof createRealtime>> | undefined;
    try {
      a = await createRealtime({
        provider: createRedisProvider({ url: redis.url, namespace: ns }),
        authorize: async () => ({ validUntil: Date.now() + 10000 }),
      });
      b = await createRealtime({
        provider: createRedisProvider({ url: redis.url, namespace: ns }),
        authorize: async () => ({ validUntil: Date.now() + 10000 }),
      });
      const resource = { scope: "team", type: "note", id: "n1" };
      const identity = {
        scope: "team",
        subject: "owner",
        principal: null,
        expiresAt: Date.now() + 60000,
      };
      const ca = a.connect(identity),
        cb = b.connect(identity);
      await ca.subscribe(resource);
      await cb.subscribe(resource);
      const ai = ca[Symbol.asyncIterator](),
        bi = cb[Symbol.asyncIterator]();
      await ai.next();
      await bi.next();
      const cursor = await a.publish(resource, "note.updated", { revision: 2 });
      for (const iterator of [ai, bi]) {
        const received = (await iterator.next()).value;
        expect(received).toMatchObject({ kind: "update", data: { revision: 2 } });
        const expected = parseCursor(cursor)!;
        expect(parseCursor(received.cursor)).toMatchObject({
          generation: expected.generation,
          sequence: expected.sequence,
        });
      }
      await a.revokeResource(resource);
      expect((await ai.next()).value.reason).toBe("revoked");
      expect((await bi.next()).value.reason).toBe("revoked");
      expect(a.stats().subscriptions).toBe(0);
      expect(b.stats().subscriptions).toBe(0);
    } finally {
      await a?.close();
      await b?.close();
      await redis.stop();
    }
  }, 15000);

  test("two real listener processes receive identical cursors and ordered events", async () => {
    const redis = await server();
    const ns = namespace();
    const children: Awaited<ReturnType<typeof listener>>[] = [];
    const publisher = createRedisProvider({ url: redis.url, namespace: ns });
    try {
      children.push(await listener(redis.url, ns));
      children.push(await listener(redis.url, ns));
      expect(children[0]!.process.pid).not.toBe(children[1]!.process.pid);
      const local: Delivery[] = [];
      await publisher.start((delivery) => local.push(delivery), noFailure);
      const initial = await publisher.current("resource/team/doc/1");
      expect(initial.sequence).toBe(0);
      const events = [
        { kind: "update" as const, type: "changed", data: { text: 'quoted " 雪', count: 1 } },
        { kind: "update" as const, type: "changed", data: [2, null, true] },
        { kind: "deleted" as const },
        { kind: "revoke" as const },
      ];
      const expected: Delivery[] = [];
      for (const event of events) {
        const cursor = await publisher.publish("resource/team/doc/1", event);
        expected.push({ topic: "resource/team/doc/1", cursor, event });
      }
      await until(() =>
        children.every(
          (child) => child.messages.filter((message) => message.delivery).length === events.length,
        ),
      );
      for (const child of children)
        expect(
          child.messages.flatMap((message) => (message.delivery ? [message.delivery] : [])),
        ).toEqual(expected);
      expect(expected.map((delivery) => delivery.cursor.sequence)).toEqual([1, 2, 3, 4]);
      expect(expected.every((delivery) => delivery.cursor.generation === initial.generation)).toBe(
        true,
      );
      await until(() => local.length === 4);
      expect(local).toEqual(expected);
      expect(await publisher.current("resource/team/doc/1")).toEqual(expected[3]!.cursor);
      await Promise.all(children.map((child) => child.stop()));
      expect(children.every((child) => child.messages.some((message) => message.closed))).toBe(
        true,
      );
      expect(children.every((child) => child.process.exitCode === 0)).toBe(true);
    } finally {
      await publisher.close();
      try {
        await Promise.all(children.map((child) => child.stop()));
      } finally {
        await redis.stop();
      }
    }
  }, 15000);

  test("construction is inert; unavailable startup fails once without retries", async () => {
    let failures = 0;
    const provider = createRedisProvider({
      url: `redis://127.0.0.1:${freePort()}`,
      namespace: namespace(),
      connectionTimeoutMs: 100,
      commandTimeoutMs: 250,
    });
    expect(failures).toBe(0);
    try {
      await expect(provider.start(noDelivery, () => failures++)).rejects.toMatchObject({
        code: "provider",
      });
      await Bun.sleep(300);
      expect(failures).toBe(1);
      await expect(provider.current("topic")).rejects.toMatchObject({ code: "provider" });
    } finally {
      await provider.close();
    }
  });

  test("metadata expires, resets generation, and never replays events", async () => {
    const redis = await server();
    const ns = namespace();
    const provider = createRedisProvider({ url: redis.url, namespace: ns, watermarkTtlMs: 60 });
    let late: RealtimeProvider | undefined;
    try {
      await provider.start(noDelivery, noFailure);
      const first = await provider.publish("topic", { kind: "deleted" });
      await until(
        async () => Number(await redis.admin.send("EXISTS", [watermarkKey(ns, "topic")])) === 0,
      );
      const next = await provider.current("topic");
      expect(next.generation).not.toBe(first.generation);
      expect(next.sequence).toBe(0);
      const deliveries: Delivery[] = [];
      late = createRedisProvider({ url: redis.url, namespace: ns });
      await late.start((delivery) => deliveries.push(delivery), noFailure);
      await Bun.sleep(30);
      expect(deliveries).toEqual([]);
      expect(Number(await redis.admin.send("DBSIZE", []))).toBe(1);
    } finally {
      await late?.close();
      await provider.close();
      await redis.stop();
    }
  });

  test("close releases exactly two clients and suppresses failure notifications", async () => {
    const redis = await server();
    let failures = 0;
    const provider = createRedisProvider({ url: redis.url, namespace: namespace() });
    try {
      const before = await clients(redis.admin);
      expect(await clients(redis.admin)).toBe(before);
      await provider.start(noDelivery, () => failures++);
      expect(await clients(redis.admin)).toBe(before + 2);
      await provider.close();
      await provider.close();
      await until(async () => (await clients(redis.admin)) === before);
      expect(failures).toBe(0);
      await expect(provider.publish("topic", { kind: "revoke" })).rejects.toMatchObject({
        code: "closed",
      });
    } finally {
      await provider.close();
      await redis.stop();
    }
  });

  test("server termination fails once and rejects future commands", async () => {
    const redis = await server();
    let failures = 0;
    const provider = createRedisProvider({ url: redis.url, namespace: namespace() });
    try {
      await provider.start(noDelivery, () => failures++);
      redis.process.kill("SIGTERM");
      await redis.process.exited;
      await until(() => failures === 1);
      await expect(provider.current("topic")).rejects.toMatchObject({ code: "provider" });
      await Bun.sleep(50);
      expect(failures).toBe(1);
    } finally {
      await provider.close();
      await redis.stop();
    }
  });

  test("admission is bounded and command timeout has no publish retry", async () => {
    const redis = await server();
    const ns = namespace();
    let failures = 0;
    const provider = createRedisProvider({
      url: redis.url,
      namespace: ns,
      maxPendingOperations: 1,
      commandTimeoutMs: 100,
    });
    try {
      await provider.start(noDelivery, () => failures++);
      await redis.admin.send("CLIENT", ["PAUSE", "400", "ALL"]);
      const pending = provider.publish("topic", { kind: "revoke" });
      await expect(provider.current("topic")).rejects.toMatchObject({ code: "limit" });
      await expect(pending).rejects.toMatchObject({ code: "provider" });
      expect(failures).toBe(1);
      await Bun.sleep(450);
      // A timed-out publish may commit once or not at all, never be retried.
      const sequence = await redis.admin.send("HGET", [watermarkKey(ns, "topic"), "sequence"]);
      expect(sequence === null || sequence === "1").toBe(true);
      await until(async () => (await clients(redis.admin)) === 1);
    } finally {
      await provider.close();
      await redis.stop();
    }
  });

  test("invalid inbound shape and oversized wire fail closed", async () => {
    const redis = await server();
    try {
      for (const message of [
        "{",
        JSON.stringify({
          topic: "topic",
          cursor: { generation: "abc", sequence: 1 },
          event: { kind: "update", type: "x" },
        }),
        "x".repeat(48 * 1024 + 1),
      ]) {
        const ns = namespace();
        let failures = 0;
        const provider = createRedisProvider({ url: redis.url, namespace: ns });
        try {
          await provider.start(noDelivery, () => failures++);
          await redis.admin.send("PUBLISH", [`${ns}:events`, message]);
          await until(() => failures === 1);
          await expect(provider.current("topic")).rejects.toMatchObject({ code: "provider" });
          await until(async () => (await clients(redis.admin)) === 1);
        } finally {
          await provider.close();
        }
      }
    } finally {
      await redis.stop();
    }
  });

  test("default admission allows 64 operations; close cancels all pending deadlines", async () => {
    const redis = await server();
    let failures = 0;
    const provider = createRedisProvider({ url: redis.url, namespace: namespace() });
    try {
      await provider.start(noDelivery, () => failures++);
      await redis.admin.send("CLIENT", ["PAUSE", "200", "ALL"]);
      const pending = Array.from({ length: 64 }, () => provider.current("topic"));
      await expect(provider.current("topic")).rejects.toMatchObject({ code: "limit" });
      const results = Promise.allSettled(pending);
      await provider.close();
      for (const result of await results) {
        expect(result.status).toBe("rejected");
        if (result.status === "rejected") expect(result.reason).toMatchObject({ code: "closed" });
      }
      await until(async () => (await clients(redis.admin)) === 1);
      expect(failures).toBe(0);
    } finally {
      await provider.close();
      await redis.stop();
    }
  });

  test("Redis command errors fail closed and release both clients", async () => {
    const redis = await server();
    const ns = namespace();
    let failures = 0;
    const provider = createRedisProvider({ url: redis.url, namespace: ns });
    try {
      await provider.start(noDelivery, () => failures++);
      await redis.admin.send("SET", [watermarkKey(ns, "topic"), "wrong-type"]);
      await expect(provider.current("topic")).rejects.toMatchObject({ code: "provider" });
      expect(failures).toBe(1);
      await expect(provider.publish("topic", { kind: "deleted" })).rejects.toMatchObject({
        code: "provider",
      });
      await until(async () => (await clients(redis.admin)) === 1);
    } finally {
      await provider.close();
      await redis.stop();
    }
  });

  test("colon-containing namespaces cannot alias another namespace's topic metadata", async () => {
    const redis = await server();
    const ns = namespace();
    const first = createRedisProvider({ url: redis.url, namespace: ns });
    const second = createRedisProvider({ url: redis.url, namespace: `${ns}:watermark:nested` });
    const seen: Delivery[] = [];
    try {
      await first.start(noDelivery, noFailure);
      await second.start((delivery) => seen.push(delivery), noFailure);
      const a = await first.publish("nested:watermark:topic", { kind: "deleted" });
      const b = await second.current("topic");
      expect(a.generation).not.toBe(b.generation);
      expect(b.sequence).toBe(0);
      const ttl = Number(
        await redis.admin.send("PTTL", [watermarkKey(ns, "nested:watermark:topic")]),
      );
      expect(ttl).toBeGreaterThan(590000);
      expect(ttl).toBeLessThanOrEqual(600000);
      await Bun.sleep(30);
      expect(seen).toEqual([]);
    } finally {
      await first.close();
      await second.close();
      await redis.stop();
    }
  });

  test("unsafe namespace and oversized outbound payload are rejected before publishing", async () => {
    expect(() =>
      createRedisProvider({ url: "redis://127.0.0.1:1", namespace: "bad/channel" }),
    ).toThrow("Realtime invalid-input");
    const redis = await server();
    const provider = createRedisProvider({ url: redis.url, namespace: namespace() });
    try {
      await provider.start(noDelivery, noFailure);
      await expect(
        provider.publish("topic", { kind: "update", type: "x", data: "x".repeat(48 * 1024) }),
      ).rejects.toMatchObject({ code: "payload" });
      expect((await provider.current("topic")).sequence).toBe(0);
    } finally {
      await provider.close();
      await redis.stop();
    }
  });
});
