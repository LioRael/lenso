import { afterEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { createEvents } from "../src/index";
import { createEventsPlugin } from "../src/plugin";
import { createSqliteEventStore, initializeEvents } from "../src/sqlite";

const cleanup: (() => void)[] = [];
afterEach(() => {
  for (const fn of cleanup.splice(0).reverse()) fn();
});
function fixture(path = ":memory:") {
  const db = new Database(path);
  cleanup.push(() => db.close());
  initializeEvents(db);
  let time = 1_000;
  const store = createSqliteEventStore(db, { now: () => time });
  const service = createEvents({ store, leaseMs: 100, retryDelayMs: 10 });
  return {
    db,
    store,
    service,
    advance: (ms: number) => {
      time += ms;
    },
  };
}
const event = { id: "order:1", topic: "order.created", payload: { amount: 10 } };
function deferred() {
  return Promise.withResolvers<void>();
}

test("event and fanout rollback together; application can share its SQLite transaction", () => {
  const { db, store } = fixture();
  store.register("a", event.topic);
  store.register("b", event.topic);
  db.run(`CREATE TRIGGER fail_fanout BEFORE INSERT ON lenso_event_deliveries
    WHEN NEW.subscription_id = 'b' BEGIN SELECT RAISE(ABORT, 'fault'); END`);
  expect(() => store.publish(event)).toThrow("fault");
  expect(store.status(event.id)).toEqual([]);
  db.run("DROP TRIGGER fail_fanout");
  db.run("CREATE TABLE orders(id TEXT PRIMARY KEY)");
  expect(() =>
    db.transaction(() => {
      db.run("INSERT INTO orders VALUES ('one')");
      store.publish(event);
      throw new Error("rollback business operation");
    })(),
  ).toThrow("rollback business operation");
  expect(db.query("SELECT * FROM orders").all()).toEqual([]);
  expect(store.publish(event)).toEqual({ created: true });
  expect(store.status(event.id)).toHaveLength(2);
});

test("idempotent publish validates content and never backfills newly registered subscribers", async () => {
  const { service } = fixture();
  let calls = 0;
  service.subscribe("a", event.topic, () => {
    calls++;
  });
  const input = { ...event, payload: { a: 1, b: 2 } };
  expect(service.publish(input).created).toBe(true);
  service.subscribe("later", event.topic, () => {
    throw new Error("must not backfill");
  });
  expect(service.publish({ ...input, payload: { b: 2, a: 1 } }).created).toBe(false);
  expect(() => service.publish(event)).toThrow("conflicts");
  await service.dispatch();
  expect(service.publish(input).created).toBe(false);
  await service.dispatch();
  expect(calls).toBe(1);
  expect(service.status(event.id)).toHaveLength(1);
});

test("a failed subscription retries independently; other events continue", async () => {
  const { service, advance } = fixture();
  const good: string[] = [];
  const bad: string[] = [];
  let fail = true;
  service.subscribe("good", event.topic, (e) => {
    good.push(e.id);
  });
  service.subscribe("bad", event.topic, (_e, context) => {
    bad.push(context.idempotencyKey);
    if (fail) throw new Error("temporary destination failure");
  });
  service.publish(event);
  service.publish({ ...event, id: "order:2" });
  expect(await service.dispatch()).toEqual({ delivered: 2, retried: 2, lost: 0 });
  fail = false;
  advance(10);
  expect(await service.dispatch()).toEqual({ delivered: 2, retried: 0, lost: 0 });
  expect(good).toEqual(["order:1", "order:2"]);
  expect(bad.slice(0, 2)).toEqual(bad.slice(2));
});

test("file reopen recovers expired claims and fences stale acknowledgement/retry", () => {
  const directory = mkdtempSync(join(tmpdir(), "lenso-events-"));
  cleanup.push(() => rmSync(directory, { recursive: true, force: true }));
  const path = join(directory, "events.sqlite");
  const first = new Database(path);
  initializeEvents(first);
  const oldStore = createSqliteEventStore(first, { now: () => 1_000 });
  oldStore.register("worker", event.topic);
  oldStore.publish(event);
  const oldClaim = oldStore.claim(["worker"], 100)!;
  first.close(); // Process dies before acknowledgement.
  const second = fixture(path);
  expect(second.store.claim(["worker"], 100)).toBeNull();
  second.advance(100);
  expect(second.store.acknowledge(oldClaim)).toBe(false);
  const recovered = second.store.claim(["worker"], 100)!;
  expect(recovered.attempt).toBe(2);
  expect(recovered.idempotencyKey).toBe(oldClaim.idempotencyKey);
  expect(second.store.acknowledge(oldClaim)).toBe(false);
  expect(second.store.retry(oldClaim, 1)).toBe(false);
  expect(second.store.acknowledge(recovered)).toBe(true);
  expect(second.store.claim(["worker"], 100)).toBeNull();
});

test("acknowledgement failure repeats a stable key so an idempotent fake avoids duplicate effects", async () => {
  const { store, advance } = fixture();
  let broken = true;
  const service = createEvents({
    store: {
      ...store,
      acknowledge(delivery) {
        if (broken) throw new Error("database unavailable after effect");
        return store.acknowledge(delivery);
      },
    },
    leaseMs: 100,
  });
  const effects = new Set<string>();
  let attempts = 0;
  service.subscribe("fake", event.topic, (_e, { idempotencyKey }) => {
    attempts++;
    effects.add(idempotencyKey);
  });
  service.publish(event);
  await expect(service.dispatch()).rejects.toThrow("database unavailable");
  expect(service.status(event.id)[0].state).toBe("processing");
  broken = false;
  advance(100);
  expect((await service.dispatch()).delivered).toBe(1);
  expect(attempts).toBe(2);
  expect(effects.size).toBe(1);
});

test("competing connections do not claim a live lease; an expired handler cannot confirm", async () => {
  const directory = mkdtempSync(join(tmpdir(), "lenso-event-race-"));
  cleanup.push(() => rmSync(directory, { recursive: true, force: true }));
  const path = join(directory, "events.sqlite");
  const first = fixture(path);
  const second = fixture(path);
  const entered = deferred();
  const finish = deferred();
  first.service.subscribe("worker", event.topic, async () => {
    entered.resolve();
    await finish.promise;
  });
  second.service.subscribe("worker", event.topic, () => {});
  first.service.publish(event);
  const run = first.service.dispatch();
  await entered.promise;
  expect((await second.service.dispatch()).delivered).toBe(0);
  first.advance(100);
  second.advance(100);
  expect((await second.service.dispatch()).delivered).toBe(1);
  finish.resolve();
  expect((await run).lost).toBe(1);
  expect(first.service.status(event.id)[0].attempts).toBe(2);
});

test("unsubscribe cancels pending and active claims, aborts locally, and cannot revive old delivery", async () => {
  const { service, store } = fixture();
  const entered = deferred();
  const finish = deferred();
  let signal!: AbortSignal;
  let calls = 0;
  const unsubscribe = service.subscribe("worker", event.topic, async (_e, context) => {
    calls++;
    signal = context.signal;
    entered.resolve();
    await finish.promise;
  });
  service.publish(event);
  service.publish({ ...event, id: "order:2" });
  const run = service.dispatch();
  await entered.promise;
  unsubscribe();
  unsubscribe();
  expect(signal.aborted).toBe(true);
  finish.resolve();
  expect((await run).lost).toBe(1);
  expect(calls).toBe(1);
  expect(service.status("order:2")[0].state).toBe("cancelled");
  expect(() => store.register("worker", event.topic)).toThrow("cannot be reused");
  expect(store.claim(["worker"], 100)).toBeNull();
});

test("shutdown drains handlers, preserves subscription/backlog, and blocks a queued dispatch", async () => {
  const { service, store } = fixture();
  const entered = deferred();
  const finish = deferred();
  service.subscribe("worker", event.topic, async (_e, { signal }) => {
    entered.resolve();
    await finish.promise;
    expect(signal.aborted).toBe(true);
  });
  service.publish(event);
  service.publish({ ...event, id: "order:2" });
  const run = service.dispatch();
  await entered.promise;
  let closed = false;
  const stopping = service.close().then(() => {
    closed = true;
  });
  await Promise.resolve();
  expect(closed).toBe(false);
  finish.resolve();
  await stopping;
  expect((await run).delivered).toBe(1);
  const replacement = createEvents({ store });
  let calls = 0;
  replacement.subscribe("worker", event.topic, () => {
    calls++;
  });
  expect((await replacement.dispatch()).delivered).toBe(1);
  expect(calls).toBe(1);
  replacement.publish({ ...event, id: "order:3" });
  const queued = replacement.dispatch();
  await replacement.close();
  expect((await queued).delivered).toBe(0);
  expect(store.status("order:3")[0].state).toBe("pending");
  expect(() => replacement.publish(event)).toThrow("closed");
});

test("plugin binds the exact provider and drains before its owned connection closes", async () => {
  const db = new Database(":memory:");
  const provider = definePlugin({
    id: "event-store",
    setup(context) {
      context.onCleanup(() => db.close());
      initializeEvents(db);
      return createSqliteEventStore(db);
    },
  });
  const plugin = createEventsPlugin({ id: "events", store: provider });
  expect(plugin.requires?.[0]).toBe(provider);
  const app = await startApp(defineApp({ plugins: [provider, plugin] }));
  const service = app.get(plugin);
  const entered = deferred();
  const finish = deferred();
  service.subscribe("worker", event.topic, async () => {
    entered.resolve();
    await finish.promise;
  });
  service.publish(event);
  const run = service.dispatch();
  await entered.promise;
  const stopping = app.stop();
  finish.resolve();
  expect((await run).delivered).toBe(1);
  await stopping;
  expect(() => service.publish(event)).toThrow("closed");
});

test("lossy payloads cannot reserve an event ID", () => {
  const { store } = fixture();
  for (const payload of [NaN, undefined, [undefined], new Date(), Array(2)]) {
    expect(() => store.publish({ ...event, payload: payload as never })).toThrow();
  }
  expect(store.publish(event).created).toBe(true);
});
