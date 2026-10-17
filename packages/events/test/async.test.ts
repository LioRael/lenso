import { expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { defineApp, startApp, definePlugin } from "@lenso/core";
import { createAsyncEvents } from "../src/async";
import { createAsyncEventsPlugin } from "../src/async-plugin";
import { createSqliteEventStore, initializeEvents } from "../src/sqlite";
import type { AsyncEventStore, EventStore } from "../src/contracts";

function gate() {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
}
function fixture() {
  const db = new Database(":memory:");
  initializeEvents(db);
  return { db, store: createSqliteEventStore(db) };
}
const event = { id: "order:1", topic: "order.created", payload: null };

function asynchronous(store: EventStore): AsyncEventStore {
  return {
    register: async (...args) => store.register(...args),
    cancel: async (...args) => store.cancel(...args),
    publish: async (...args) => store.publish(...args),
    claim: async (...args) => store.claim(...args),
    acknowledge: async (...args) => store.acknowledge(...args),
    retry: async (...args) => store.retry(...args),
    status: async (...args) => store.status(...args),
  };
}

test("async registration/cancellation serialize; close drains accepted registration without attaching", async () => {
  const { db, store } = fixture();
  const started = gate();
  const finish = gate();
  const events = createAsyncEvents({
    store: {
      ...asynchronous(store),
      async register(id, topic) {
        started.release();
        await finish.promise;
        store.register(id, topic);
      },
    },
  });
  try {
    const subscription = events.subscribe("receipt", event.topic, () => {});
    await started.promise;
    await expect(events.subscribe("receipt", event.topic, () => {})).rejects.toThrow(
      "already attached",
    );
    const cancellation = events.unsubscribe("receipt");
    const closing = events.close();
    let closed = false;
    void closing.then(() => {
      closed = true;
    });
    await Promise.resolve();
    expect(closed).toBe(false);
    finish.release();
    await Promise.all([subscription, cancellation, closing]);
    expect(() => store.register("receipt", event.topic)).toThrow("cannot be reused");
    expect(store.publish(event)).toEqual({ created: true });
    expect(store.status(event.id)).toEqual([]);
    await expect(events.publish(event)).rejects.toThrow("closed");
  } finally {
    finish.release();
    await events.close();
    db.close();
  }
});

test("close while asynchronous claim is pending never starts a handler and retains recoverable claim", async () => {
  const { db, store } = fixture();
  const claimed = gate();
  const finish = gate();
  let calls = 0;
  const events = createAsyncEvents({
    store: {
      ...asynchronous(store),
      async claim(ids, leaseMs) {
        const delivery = store.claim(ids, leaseMs);
        claimed.release();
        await finish.promise;
        return delivery;
      },
    },
  });
  try {
    await events.subscribe("receipt", event.topic, () => {
      calls++;
    });
    await events.publish(event);
    const dispatch = events.dispatch();
    await claimed.promise;
    const closing = events.close();
    finish.release();
    expect(await dispatch).toEqual({ delivered: 0, retried: 0, lost: 1 });
    await closing;
    expect(calls).toBe(0);
    expect(store.status(event.id)[0].state).toBe("processing");
  } finally {
    finish.release();
    await events.close();
    db.close();
  }
});

test("asynchronous acknowledgement failure rejects dispatch and plugin closes only its service", async () => {
  const { db, store } = fixture();
  const provider = definePlugin({
    id: "events-store",
    setup: () => ({
      ...asynchronous(store),
      async acknowledge() {
        throw new Error("storage unavailable");
      },
    }),
  });
  const plugin = createAsyncEventsPlugin({ id: "events", store: provider });
  const app = await startApp(defineApp({ plugins: [provider, plugin] }));
  try {
    const events = app.get(plugin);
    await events.subscribe("receipt", event.topic, () => {});
    await events.publish(event);
    await expect(events.dispatch()).rejects.toThrow("storage unavailable");
    expect(store.status(event.id)[0].state).toBe("processing");
    await app.stop();
    expect(store.status(event.id)).toHaveLength(1);
    await expect(events.status(event.id)).rejects.toThrow("closed");
  } finally {
    await app.stop();
    db.close();
  }
});
