import { expect, spyOn, test } from "bun:test";
import { createRealtime } from "../src/index";
import { createMemoryProvider } from "../src/memory";
import { resourceTopic } from "../src/wire";
import type { Delivery, Envelope, RealtimeProvider } from "../src/contracts";

const resource = { scope: "app", type: "note", id: "private" };

for (const transport of ["iterator", "sse"] as const) {
  test.each(["subject", "resource", "unsubscribe", "lease", "session"] as const)(
    `${transport} purges a woken private update on %s invalidation before delivery`,
    async (invalidation) => {
      const now = Date.now();
      const clock = spyOn(Date, "now").mockReturnValue(now);
      let deliver!: (delivery: Delivery) => void;
      const base = createMemoryProvider();
      const provider: RealtimeProvider = {
        ...base,
        start(listener, fail) {
          deliver = listener;
          return base.start(listener, fail);
        },
      };
      const realtime = await createRealtime({
        provider,
        authorize: async () => ({ validUntil: Date.now() + 60000 }),
      });
      try {
        const connection = realtime.connect({
          scope: "app",
          subject: "reader",
          principal: {},
          expiresAt: now + 60000,
        });
        const subscription = await connection.subscribe(resource);
        const iterator = transport === "iterator" ? connection[Symbol.asyncIterator]() : undefined;
        const reader = transport === "sse" ? connection.response().body!.getReader() : undefined;
        const read = () => (iterator ? iterator.next() : reader!.read());
        const ready = await read();
        const token =
          transport === "iterator"
            ? (ready.value as Envelope).cursor!
            : /^id: (.+)$/m.exec(new TextDecoder().decode(ready.value as Uint8Array))![1]!;
        const [generation, sequence] = token.split(".");
        const pending = read();
        await Promise.resolve(); // let SSE pull wait on the service iterator
        deliver({
          topic: resourceTopic(resource),
          cursor: { generation: generation!, sequence: Number(sequence) + 1 },
          event: { kind: "update", type: "note.updated", data: { secret: "PRIVATE-MARKER" } },
        });
        let change: Promise<void> | undefined;
        if (invalidation === "subject") realtime.revokeSubject("app", "reader");
        else if (invalidation === "resource") change = realtime.revokeResource(resource);
        else if (invalidation === "unsubscribe") subscription.unsubscribe();
        else clock.mockReturnValue(now + (invalidation === "lease" ? 30001 : 60001));
        const next = await pending;
        const received =
          transport === "iterator"
            ? JSON.stringify(next.value)
            : new TextDecoder().decode(next.value as Uint8Array);
        expect(received).not.toContain("PRIVATE-MARKER");
        const reason =
          invalidation === "unsubscribe"
            ? "unsubscribed"
            : invalidation === "lease" || invalidation === "session"
              ? "expired"
              : "revoked";
        expect(received).toContain(reason);
        await change;
        connection.close();
        expect((await read()).done).toBe(true);
        expect(realtime.stats()).toEqual({
          connections: 0,
          subscriptions: 0,
          topics: 0,
          pending: 0,
        });
      } finally {
        await realtime.close();
        clock.mockRestore();
      }
    },
  );
}

test("SSE rechecks revocation between dequeue and frame encoding", async () => {
  const realtime = await createRealtime({
    provider: createMemoryProvider(),
    authorize: async () => ({ validUntil: Date.now() + 60000 }),
  });
  try {
    const connection = realtime.connect({
      scope: "app",
      subject: "reader",
      principal: {},
      expiresAt: Date.now() + 60000,
    });
    await connection.subscribe(resource);
    const reader = connection.response().body!.getReader();
    await reader.read();
    await realtime.publish(resource, "note.updated", { secret: "PRIVATE-MARKER" });
    queueMicrotask(() => realtime.revokeSubject("app", "reader"));
    const next = await reader.read();
    const text = new TextDecoder().decode(next.value);
    expect(text).not.toContain("PRIVATE-MARKER");
    expect(text).toContain("revoked");
    expect((await reader.read()).done).toBe(true);
  } finally {
    await realtime.close();
  }
});

test("a second next cannot steal an update from a woken reader", async () => {
  const base = createMemoryProvider();
  let iterator!: AsyncIterator<Envelope>;
  let competing!: Promise<unknown>;
  const realtime = await createRealtime({
    provider: {
      ...base,
      publish(topic, event) {
        const result = base.publish(topic, event);
        // Base delivery has woken the reader, whose continuation has not run.
        competing = iterator.next().catch((error: unknown) => error);
        return result;
      },
    },
    authorize: async () => ({ validUntil: Date.now() + 60000 }),
  });
  try {
    const connection = realtime.connect({
      scope: "app",
      subject: "reader",
      principal: {},
      expiresAt: Date.now() + 60000,
    });
    await connection.subscribe(resource);
    iterator = connection[Symbol.asyncIterator]();
    await iterator.next();
    const pending = iterator.next();
    await realtime.publish(resource, "note.updated", { revision: 2 });
    expect(await competing).toMatchObject({ code: "invalid-input" });
    expect((await pending).value).toMatchObject({ kind: "update", data: { revision: 2 } });
  } finally {
    await realtime.close();
  }
});

test("reconnect snapshots missed changes and rejects a revoked policy without replay", async () => {
  let allowed = true;
  let storedRevision = 1; // simulated business storage, not a D1/PG integration
  const realtime = await createRealtime({
    provider: createMemoryProvider(),
    authorize: async () => (allowed ? { validUntil: Date.now() + 60000 } : false),
  });
  const identity = {
    scope: "app",
    subject: "reader",
    principal: {},
    expiresAt: Date.now() + 60000,
  };
  try {
    const first = realtime.connect(identity);
    const subscription = await first.subscribe(resource);
    const original = await first.snapshot(subscription, async () => storedRevision);
    expect(original).toMatchObject({ value: 1, stable: true });
    first.close();
    storedRevision = 2;
    await realtime.publish(resource, "note.updated", { revision: storedRevision });

    const second = realtime.connect(identity);
    const resumed = await second.subscribe(resource, { cursor: original.cursor });
    const events = second[Symbol.asyncIterator]();
    expect((await events.next()).value.kind).toBe("ready");
    expect((await events.next()).value).toMatchObject({ kind: "gap", reason: "reconnect" });
    expect(await second.snapshot(resumed, async () => storedRevision)).toMatchObject({
      value: 2,
      stable: true,
    });
    allowed = false;
    await realtime.revokeResource(resource);
    expect((await events.next()).value).toMatchObject({ kind: "closed", reason: "revoked" });
    await expect(second.snapshot(resumed, async () => storedRevision)).rejects.toMatchObject({
      code: "denied",
    });
    const denied = realtime.connect(identity);
    await expect(denied.subscribe(resource, { cursor: original.cursor })).rejects.toMatchObject({
      code: "denied",
    });
  } finally {
    await realtime.close();
  }
});
