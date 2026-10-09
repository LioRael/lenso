import { afterEach, expect, test } from "bun:test";
import { createRealtime, RealtimeError, resolveRealtimeConfig } from "../src/index";
import { createMemoryProvider } from "../src/memory";
import type {
  Delivery,
  Identity,
  Realtime,
  RealtimeOptions,
  RealtimeProvider,
  Resource,
} from "../src/contracts";
import { cursorToken, resourceTopic } from "../src/wire";

const note: Resource = { scope: "app-a", type: "note", id: "n1" };
const identity = (subject = "reader"): Identity => ({
  scope: "app-a",
  subject,
  principal: { role: "reader" },
  expiresAt: Date.now() + 60000,
});
const instances: Realtime[] = [];
afterEach(async () => {
  await Promise.all(instances.splice(0).map((instance) => instance.close()));
});
async function setup(extra: Partial<RealtimeOptions<unknown>> = {}) {
  const instance = await createRealtime({
    provider: createMemoryProvider(),
    authorize: async () => ({ validUntil: Date.now() + 60000 }),
    ...extra,
  });
  instances.push(instance);
  return instance;
}
function controlled() {
  const base = createMemoryProvider();
  let listener!: (delivery: Delivery) => void;
  let failure!: () => void;
  let last!: Delivery;
  const provider: RealtimeProvider = {
    ...base,
    start(deliver, fail) {
      listener = deliver;
      failure = fail;
      return base.start((event) => {
        last = event;
        deliver(event);
      }, fail);
    },
  };
  return {
    provider,
    emit: (delivery: Delivery) => listener(delivery),
    duplicate: () => listener(last),
    fail: () => failure(),
  };
}

test("server resource topics are collision-free and scope isolated", () => {
  expect(resourceTopic({ scope: "a/b", type: "c", id: "d" })).not.toBe(
    resourceTopic({ scope: "a", type: "b/c", id: "d" }),
  );
  expect(() => resourceTopic({ ...note, id: "" })).toThrow(RealtimeError);
});

test("allow/deny and cross-scope checks, no connection publishing capability", async () => {
  let calls = 0;
  const rt = await setup({
    authorize: async (_, resource) => {
      calls++;
      return resource.id === "n1" ? { validUntil: Date.now() + 10000 } : false;
    },
  });
  const connection = rt.connect(identity());
  expect("publish" in connection).toBe(false);
  await connection.subscribe(note);
  await expect(connection.subscribe({ ...note, id: "private" })).rejects.toMatchObject({
    code: "denied",
  });
  await expect(connection.subscribe({ ...note, scope: "app-b" })).rejects.toMatchObject({
    code: "denied",
  });
  expect(calls).toBe(2);
  expect(rt.stats().subscriptions).toBe(1);
});

test("two connections fan out, a different scope never sees the update", async () => {
  const rt = await setup();
  const a = rt.connect(identity());
  const b = rt.connect(identity("other"));
  const c = rt.connect({ ...identity("third"), scope: "app-b" });
  await a.subscribe(note);
  await b.subscribe(note);
  await c.subscribe({ ...note, scope: "app-b" });
  const ai = a[Symbol.asyncIterator](),
    bi = b[Symbol.asyncIterator](),
    ci = c[Symbol.asyncIterator]();
  await ai.next();
  await bi.next();
  await ci.next();
  const waiting = ci.next();
  await rt.publish(note, "note.updated", { revision: 2 });
  const ae = (await ai.next()).value,
    be = (await bi.next()).value;
  expect(ae.kind).toBe("update");
  expect(ae.cursor).toBe(be.cursor);
  expect(be.data).toEqual({ revision: 2 });
  c.close();
  expect((await waiting).done).toBe(true);
});

test("unsubscribe purges queued private data and removes topic references", async () => {
  const rt = await setup();
  const c = rt.connect(identity());
  const sub = await c.subscribe(note);
  const iterator = c[Symbol.asyncIterator]();
  await iterator.next();
  await rt.publish(note, "note.updated", { secret: "not delivered" });
  sub.unsubscribe();
  sub.unsubscribe();
  expect(rt.stats()).toMatchObject({ subscriptions: 0, topics: 0 });
  expect((await iterator.next()).value).toMatchObject({ kind: "closed", reason: "unsubscribed" });
  c.close();
  expect((await iterator.next()).done).toBe(true);
});

test.each(["signal", "cancel", "iterator", "shutdown"] as const)(
  "cleanup on %s settles a blocked read",
  async (mode) => {
    const rt = await setup();
    const abort = new AbortController();
    const c = rt.connect(identity(), { signal: abort.signal });
    await c.subscribe(note);
    if (mode === "cancel") {
      const reader = c.response().body!.getReader();
      await reader.read();
      const pending = reader.read();
      await reader.cancel();
      expect((await pending).done).toBe(true);
    } else {
      const iterator = c[Symbol.asyncIterator]();
      await iterator.next();
      const waiting = iterator.next();
      if (mode === "signal") abort.abort();
      else if (mode === "iterator") await iterator.return!();
      else await rt.close();
      const result = await waiting;
      expect(mode === "shutdown" ? result.value.reason : result.done).toBe(
        mode === "shutdown" ? "shutdown" : true,
      );
    }
    expect(rt.stats()).toMatchObject({ connections: 0, subscriptions: 0, topics: 0 });
  },
);

test.each(["revoke", "delete"] as const)(
  "%s purges data, closes resource subscription, retains unrelated subscriptions",
  async (kind) => {
    const rt = await setup();
    const c = rt.connect(identity());
    await c.subscribe(note);
    await c.subscribe({ ...note, id: "n2" });
    const iterator = c[Symbol.asyncIterator]();
    await iterator.next();
    await iterator.next();
    await rt.publish(note, "note.updated", { private: true });
    if (kind === "revoke") await rt.revokeResource(note);
    else await rt.deleteResource(note);
    expect((await iterator.next()).value).toMatchObject({
      kind: "closed",
      reason: kind === "revoke" ? "revoked" : "deleted",
    });
    expect(rt.stats().subscriptions).toBe(1);
  },
);

test("subject revocation does not affect another scope, denied renewal removes its subscription", async () => {
  let allowed = true;
  const rt = await setup({
    authorize: async () => (allowed ? { validUntil: Date.now() + 10000 } : false),
  });
  const c = rt.connect(identity());
  const sub = await c.subscribe(note);
  allowed = false;
  await expect(sub.renew()).rejects.toMatchObject({ code: "denied" });
  expect(rt.stats().subscriptions).toBe(0);
  const other = rt.connect({ ...identity(), scope: "app-b" });
  rt.revokeSubject("app-a", "reader");
  expect(rt.stats().connections).toBe(1);
  other.close();
});

test("lease expiry is bounded, no database polling per subscriber/message", async () => {
  let calls = 0;
  const rt = await setup({
    config: { authorizationLeaseMs: 30, sweepMs: 5 },
    authorize: async () => {
      calls++;
      return { validUntil: Date.now() + 10000 };
    },
  });
  const c = rt.connect(identity());
  await c.subscribe(note);
  const i = c[Symbol.asyncIterator]();
  await i.next();
  await rt.publish(note, "note.updated", {});
  await i.next();
  await Bun.sleep(50);
  expect((await i.next()).value).toMatchObject({ kind: "closed", reason: "expired" });
  expect(calls).toBe(1);
  expect(rt.stats().subscriptions).toBe(0);
});

test("renewal permits a new lease; session expiry cannot be renewed", async () => {
  const rt = await setup({ config: { authorizationLeaseMs: 80, sweepMs: 5 } });
  const c = rt.connect({ ...identity(), expiresAt: Date.now() + 45 });
  const sub = await c.subscribe(note);
  const i = c[Symbol.asyncIterator]();
  await i.next();
  await sub.renew();
  await Bun.sleep(65);
  expect((await i.next()).value).toMatchObject({ kind: "closed", reason: "expired" });
  await expect(sub.renew()).rejects.toMatchObject({ code: "closed" });
  expect(rt.stats().connections).toBe(0);
});

test("revoke/disconnect during pending authorization cannot resurrect a handle", async () => {
  let grant!: () => void;
  const rt = await setup({
    authorize: async () => {
      await new Promise<void>((resolve) => {
        grant = resolve;
      });
      return { validUntil: Date.now() + 10000 };
    },
  });
  const c = rt.connect(identity());
  const subscription = c.subscribe(note);
  await Bun.sleep(0);
  await rt.revokeResource(note);
  grant();
  await expect(subscription).rejects.toMatchObject({ code: "closed" });
  expect(rt.stats()).toMatchObject({ subscriptions: 0, topics: 0 });
});

test("slow consumer discards bounded queue, gets a terminal gap and disconnects", async () => {
  const rt = await setup({ config: { maxBufferedEvents: 2 } });
  const c = rt.connect(identity());
  await c.subscribe(note);
  await rt.publish(note, "note.updated", { index: 1 });
  await rt.publish(note, "note.updated", { index: 2 });
  const iterator = c[Symbol.asyncIterator]();
  expect((await iterator.next()).value).toMatchObject({ kind: "gap", reason: "overflow" });
  expect((await iterator.next()).done).toBe(true);
  expect(rt.stats()).toMatchObject({ connections: 0, topics: 0 });
});

test("byte budget also closes a slow consumer", async () => {
  const rt = await setup({ config: { maxPayloadBytes: 256, maxBufferedBytes: 768 } });
  const c = rt.connect(identity());
  await c.subscribe(note);
  for (let n = 0; n < 6; n++) await rt.publish(note, "note.updated", "x".repeat(200));
  expect((await c[Symbol.asyncIterator]().next()).value.reason).toBe("overflow");
});

test("payload limit, lossy JSON, accessors and cyclic payloads are rejected before provider", async () => {
  const rt = await setup({ config: { maxPayloadBytes: 128 } });
  await expect(rt.publish(note, "note.updated", "x".repeat(129))).rejects.toMatchObject({
    code: "payload",
  });
  for (const data of [
    NaN,
    undefined,
    1n,
    new Date(),
    { missing: undefined },
    Array(2),
    {
      get value() {
        throw new Error("private");
      },
    },
  ]) {
    await expect(rt.publish(note, "note.updated", data as never)).rejects.toMatchObject({
      code: "payload",
    });
  }
  const cycle: any = {};
  cycle.cycle = cycle;
  await expect(rt.publish(note, "note.updated", cycle)).rejects.toMatchObject({ code: "payload" });
  let invoked = false;
  const hidden = Object.defineProperty({}, "toJSON", {
    get() {
      invoked = true;
      return () => "private";
    },
  });
  await expect(rt.publish(note, "note.updated", hidden)).rejects.toMatchObject({ code: "payload" });
  expect(invoked).toBe(false);
  const sparse = Array(1) as any;
  sparse.extra = 1;
  await expect(rt.publish(note, "note.updated", sparse)).rejects.toMatchObject({ code: "payload" });
  expect(rt.stats().pending).toBe(0);
});

test("snapshots subscribe first; a racing publication marks snapshot unstable", async () => {
  const rt = await setup();
  const c = rt.connect(identity());
  const sub = await c.subscribe(note);
  const i = c[Symbol.asyncIterator]();
  await i.next();
  const result = await c.snapshot(sub, async () => {
    await rt.publish(note, "note.updated", { revision: 2 });
    return { revision: 1 };
  });
  expect(result.stable).toBe(false);
  expect((await i.next()).value.kind).toBe("update");
  expect((await i.next()).value.reason).toBe("snapshot-race");
  expect((await c.snapshot(sub, async () => ({ revision: 2 }))).stable).toBe(true);
});

test("payload near structural budget fans out completely without counting envelope nodes again", async () => {
  const rt = await setup({ config: { maxPayloadBytes: 32768 } });
  const a = rt.connect(identity()),
    b = rt.connect(identity("b"));
  await a.subscribe(note);
  await b.subscribe(note);
  const ai = a[Symbol.asyncIterator](),
    bi = b[Symbol.asyncIterator]();
  await ai.next();
  await bi.next();
  const data = Array(9996).fill(0);
  await rt.publish(note, "note.updated", data);
  expect((await ai.next()).value.data).toEqual(data);
  expect((await bi.next()).value.data).toEqual(data);
});

test("oversized asynchronous delivery fails closed instead of escaping the provider callback", async () => {
  const backend = controlled();
  const rt = await setup({ provider: backend.provider });
  const c = rt.connect(identity());
  await c.subscribe(note);
  const i = c[Symbol.asyncIterator]();
  await i.next();
  expect(() =>
    backend.emit({
      topic: resourceTopic(note),
      cursor: { generation: "other", sequence: 1 },
      event: { kind: "update", type: "note.updated", data: "x".repeat(20000) },
    }),
  ).not.toThrow();
  expect((await i.next()).value).toMatchObject({ kind: "gap", reason: "provider" });
});

test("snapshot pending across revocation cannot return private data", async () => {
  const rt = await setup();
  const c = rt.connect(identity());
  const sub = await c.subscribe(note);
  let resolve!: (data: string) => void;
  const snapshot = c.snapshot(
    sub,
    () =>
      new Promise<string>((r) => {
        resolve = r;
      }),
  );
  await Bun.sleep(0);
  await rt.revokeResource(note);
  resolve("private");
  await expect(snapshot).rejects.toMatchObject({ code: "closed" });
});

test.each(["revoke", "delete"] as const)(
  "local %s is a barrier even when provider control delivery is deferred",
  async (action) => {
    const base = createMemoryProvider();
    const provider: RealtimeProvider = {
      ...base,
      start(deliver, fail) {
        return base.start((event) => {
          if (event.event.kind === "update") deliver(event);
        }, fail);
      },
    };
    const rt = await setup({ provider });
    const c = rt.connect(identity());
    const sub = await c.subscribe(note);
    let finish!: () => void;
    const snapshot = c.snapshot(sub, async () => {
      await new Promise<void>((r) => {
        finish = r;
      });
      return "private";
    });
    await Bun.sleep(0);
    if (action === "revoke") await rt.revokeResource(note);
    else await rt.deleteResource(note);
    finish();
    await expect(snapshot).rejects.toMatchObject({ code: "closed" });
    expect(rt.stats()).toMatchObject({ subscriptions: 0, topics: 0 });
  },
);

test("reconnect never replays, invalid or old cursor signals expiration", async () => {
  const rt = await setup({ config: { cursorMaxAgeMs: 10 } });
  const token = await rt.publish(note, "note.updated", {});
  const a = rt.connect(identity());
  await a.subscribe(note, { cursor: token });
  const ai = a[Symbol.asyncIterator]();
  await ai.next();
  expect((await ai.next()).value.reason).toBe("reconnect");
  const b = rt.connect(identity("b"));
  await b.subscribe(note, {
    cursor: cursorToken({ generation: "old", sequence: 1 }, Date.now() - 100),
  });
  const bi = b[Symbol.asyncIterator]();
  await bi.next();
  expect((await bi.next()).value.reason).toBe("cursor-expired");
});

test("duplicates suppressed, sequence jump/out-of-order/generation report gaps", async () => {
  const backend = controlled();
  const rt = await setup({ provider: backend.provider });
  const c = rt.connect(identity());
  await c.subscribe(note);
  const i = c[Symbol.asyncIterator]();
  await i.next();
  await rt.publish(note, "note.updated", {});
  const cursor = (await i.next()).value.cursor!;
  backend.duplicate();
  const generation = cursor.split(".")[0];
  backend.emit({
    topic: resourceTopic(note),
    cursor: { generation, sequence: 3 },
    event: { kind: "update", type: "note.updated", data: {} },
  });
  expect((await i.next()).value.reason).toBe("sequence");
  expect((await i.next()).value.kind).toBe("update");
  backend.emit({
    topic: resourceTopic(note),
    cursor: { generation, sequence: 2 },
    event: { kind: "update", type: "note.updated", data: {} },
  });
  expect((await i.next()).value.reason).toBe("out-of-order");
  backend.emit({
    topic: resourceTopic(note),
    cursor: { generation: "restarted", sequence: 1 },
    event: { kind: "update", type: "note.updated", data: {} },
  });
  expect((await i.next()).value.reason).toBe("generation");
  expect((await i.next()).value.kind).toBe("update");
});

test("provider terminal failure closes every connection, rejects future use", async () => {
  const backend = controlled();
  const diagnostics: string[] = [];
  const rt = await setup({
    provider: backend.provider,
    onDiagnostic: (value) => diagnostics.push(value),
  });
  const c = rt.connect(identity());
  await c.subscribe(note);
  const i = c[Symbol.asyncIterator]();
  await i.next();
  backend.fail();
  backend.fail();
  expect((await i.next()).value).toMatchObject({ kind: "gap", reason: "provider" });
  expect((await i.next()).done).toBe(true);
  expect(() => rt.connect(identity())).toThrow(RealtimeError);
  await expect(rt.publish(note, "note.updated", {})).rejects.toMatchObject({ code: "provider" });
  expect(diagnostics).toEqual(["provider"]);
});

test("limits reserve slots before asynchronous authorization", async () => {
  let grant!: () => void;
  const rt = await setup({
    config: { maxConnections: 2, maxConnectionsPerSubject: 1, maxSubscriptionsPerConnection: 1 },
    authorize: async () => {
      await new Promise<void>((r) => {
        grant = r;
      });
      return { validUntil: Date.now() + 10000 };
    },
  });
  const c = rt.connect(identity());
  expect(() => rt.connect(identity())).toThrow(RealtimeError);
  const first = c.subscribe(note);
  await Bun.sleep(0);
  await expect(c.subscribe({ ...note, id: "n2" })).rejects.toMatchObject({ code: "limit" });
  grant();
  await first;
});

test("pending work stays bounded after timeout, and late authorization cannot revive it", async () => {
  let grant!: () => void;
  const rt = await setup({
    config: { maxPendingOperations: 1, snapshotTimeoutMs: 10 },
    authorize: async () => {
      await new Promise<void>((r) => {
        grant = r;
      });
      return { validUntil: Date.now() + 10000 };
    },
  });
  const c = rt.connect(identity());
  await expect(c.subscribe(note)).rejects.toMatchObject({ code: "expired" });
  expect(rt.stats()).toMatchObject({ pending: 1, subscriptions: 0 });
  await expect(c.subscribe(note)).rejects.toMatchObject({ code: "limit" });
  grant();
  await Bun.sleep(0);
  expect(rt.stats().pending).toBe(0);
});

test("memory provider is bounded and independent, watermark expiration changes generation", async () => {
  const first = createMemoryProvider({ maxTopics: 1, watermarkTtlMs: 10 });
  const second = createMemoryProvider();
  let received = 0;
  await first.start(
    () => {},
    () => {},
  );
  await second.start(
    () => {
      received++;
    },
    () => {},
  );
  try {
    const cursor = await first.current("a");
    await expect(first.current("b")).rejects.toMatchObject({ code: "limit" });
    await first.publish("a", { kind: "deleted" });
    expect(received).toBe(0);
    await Bun.sleep(20);
    expect((await first.current("a")).generation).not.toBe(cursor.generation);
  } finally {
    await first.close();
    await second.close();
  }
});

test("configuration rejects unsafe values and unknown keys", () => {
  for (const config of [
    { authorizationLeaseMs: 30001 },
    { sweepMs: 1001 },
    { maxPayloadBytes: 32769 },
    { maxBufferedBytes: 1 },
    { unknown: 1 },
    { retryMs: NaN },
  ]) {
    expect(() => resolveRealtimeConfig(config)).toThrow(RealtimeError);
  }
});

test("heartbeat and retry are configurable SSE frames", async () => {
  const rt = await setup({ config: { heartbeatMs: 10, sweepMs: 5, retryMs: 321 } });
  const c = rt.connect(identity());
  await c.subscribe(note);
  const reader = c.response().body!.getReader();
  expect(new TextDecoder().decode((await reader.read()).value)).toContain("retry: 321");
  expect(new TextDecoder().decode((await reader.read()).value)).toContain("event: heartbeat");
  await reader.cancel();
});

test("scoped subject, topic and instance subscription limits are independent", async () => {
  const rt = await setup({
    config: { maxSubscriptionsPerSubject: 1, maxSubscribersPerTopic: 1, maxTopics: 1 },
  });
  const a = rt.connect(identity()),
    sameSubject = rt.connect(identity()),
    other = rt.connect(identity("other"));
  const sub = await a.subscribe(note);
  await expect(sameSubject.subscribe({ ...note, id: "n2" })).rejects.toMatchObject({
    code: "limit",
  });
  await expect(other.subscribe(note)).rejects.toMatchObject({ code: "limit" });
  await expect(other.subscribe({ ...note, id: "n2" })).rejects.toMatchObject({ code: "limit" });
  sub.unsubscribe();
  await other.subscribe({ ...note, id: "n2" });
  expect(rt.stats()).toMatchObject({ subscriptions: 1, topics: 1 });
});

test("slow authorization does not start a fresh lease after it resolves", async () => {
  const rt = await setup({
    config: { authorizationLeaseMs: 10, sweepMs: 5, snapshotTimeoutMs: 100 },
    authorize: async () => {
      await Bun.sleep(20);
      return { validUntil: Date.now() + 60000 };
    },
  });
  await expect(rt.connect(identity()).subscribe(note)).rejects.toMatchObject({ code: "expired" });
  expect(rt.stats().subscriptions).toBe(0);
});
