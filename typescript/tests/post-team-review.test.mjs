import assert from "node:assert/strict";
import test from "node:test";
import { DatabaseSync } from "node:sqlite";
import { ActionError, Catalog, ConsoleExtension, DurableEvent, Executor, Module, emptyInput, jsonCodec } from "@lenso/app";
import { SqliteQueue } from "@lenso/sqlite";

const context = { callId: "post-team", resources: {}, principal: undefined, signal: new AbortController().signal };
const codec = jsonCodec(emptyInput);
const task = { id: "review.unit", input: codec, output: codec, run() {} };

test("an old durable unsubscribe handle cannot remove a replacement subscription", () => {
  const event = new DurableEvent("review.change");
  const removeOld = event.subscribe("inbox", task, () => undefined);
  removeOld();
  event.subscribe("inbox", task, () => undefined);
  removeOld();
  assert.equal(event.deliveries(null).length, 1);
});

test("async invariant checks finish before authorization, effects and records", async () => {
  const calls = [];
  let release;
  const pending = new Promise((resolve) => { release = resolve; });
  const executor = new Executor((record) => { calls.push(record.outcome); });
  const action = {
    id: "review.async-invariants",
    async validateInput() { await pending; calls.push("input"); },
    authorize() { calls.push("authorize"); },
    run() { calls.push("run"); return 1; },
    async validateOutput() { await Promise.resolve(); calls.push("output"); throw new ActionError("forbidden", "Broken result"); },
  };
  const running = executor.call(action, context, null);
  const rejected = assert.rejects(running, { code: "internal" });
  await Promise.resolve();
  assert.deepEqual(calls, []);
  release();
  await rejected;
  assert.deepEqual(calls, ["input", "authorize", "run", "output", "internal"]);
  calls.length = 0;
  await assert.rejects(executor.call({ ...action, async validateInput() { throw new ActionError("invalid_input", "Rejected"); } }, context, null), { code: "invalid_input" });
  assert.deepEqual(calls, ["invalid_input"]);
});

test("a native manifest caller cannot mutate later Console descriptions", () => {
  const action = { id: "review.list", input: emptyInput, authorize() {}, run: () => [] };
  const extension = new ConsoleExtension(new Executor(() => {}), [{ id: "review", title: "Review", list: action, actions: [action] }]);
  const catalog = new Catalog();
  catalog.register(new Module("review", { actions: { action } }));
  const first = extension.manifest(catalog);
  first.pages[0].title = "Changed";
  first.pages[0].actions.length = 0;
  first.pages.reverse();
  const next = extension.manifest(catalog);
  assert.equal(next.pages[0].title, "Review");
  assert.deepEqual(next.pages[0].actions, [action.id]);
});

test("one durable publication uses one clock snapshot for every delivery", () => {
  const db = new DatabaseSync(":memory:");
  let now = 1000;
  const queue = new SqliteQueue(db, () => now++);
  const event = new DurableEvent("review.clock");
  event.subscribe("first", task, () => undefined);
  event.subscribe("second", task, () => undefined);
  try { assert.deepEqual(queue.publish(event, "one", null).map((job) => job.availableMs), [1000, 1000]); }
  finally { db.close(); }
});

test("lease loss signals cancellation and keeps ownership of handler cleanup", async () => {
  const db = new DatabaseSync(":memory:");
  const queue = new SqliteQueue(db);
  let cancelObserved, finish;
  const cancelled = new Promise((resolve) => { cancelObserved = resolve; });
  const pending = new Promise((resolve) => { finish = resolve; });
  const cleanup = { ...task, async run(context) {
    db.exec("UPDATE lenso_jobs SET lease_token='replacement'");
    await new Promise((resolve) => context.signal.addEventListener("abort", resolve, { once: true }));
    cancelObserved();
    await pending;
    throw new ActionError("cancelled", "Cleanup complete");
  } };
  const job = queue.enqueue(cleanup, undefined);
  let settled = false;
  const running = queue.runOne(cleanup, {}, { leaseMs: 300 }).finally(() => { settled = true; });
  const rejected = assert.rejects(running, { code: "conflict" });
  try {
    await cancelled;
    assert.equal(settled, false);
    finish();
    await rejected;
    assert.equal(queue.attempts(job.id)[0].outcome, "running");
  } finally { finish(); db.close(); }
});

test("Console standalone operations reuse the executor without inventing a page control", async () => {
  const records = [];
  const action = { id: "review.session", input: emptyInput, authorize(context) {
    if (!context.principal) throw new ActionError("unauthenticated", "Sign in");
  }, run: () => ({ subject: "alice" }) };
  const extension = new ConsoleExtension(new Executor((record) => { records.push(record.outcome); }), [], [action]);
  const catalog = new Catalog(); catalog.register(new Module("session", { actions: { action } }));
  assert.deepEqual(extension.manifest(catalog).pages, []);
  assert.throws(() => extension.manifest(new Catalog()), { code: "not_found" });
  await assert.rejects(extension.call(context, action.id, null), { code: "unauthenticated" });
  assert.deepEqual(await extension.call({ ...context, principal: { subject: "alice", scopes: [] } }, action.id, null), { subject: "alice" });
  assert.deepEqual(records, ["unauthenticated", "ok"]);
  await assert.rejects(extension.call(context, "unknown.operation", null), { code: "not_found" });
  assert.throws(() => new ConsoleExtension(new Executor(() => {}), [], [action, { ...action }]), { code: "conflict" });
});
