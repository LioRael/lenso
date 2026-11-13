import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import {
  ActionError, Catalog, ConsoleExtension, DurableEvent, Event, Executor, Module,
  allOf, anyOf, consoleOperation, defineAction, defineResource, emptyInput, jsonCodec, requireScope, withResources,
} from "@lenso/app";
import { ConsoleClient, ConsolePages } from "@lenso/console";
import { SqliteQueue } from "@lenso/sqlite";

const context = { callId: "review", principal: { subject: "alice", scopes: ["read"] }, resources: { offset: 2 }, signal: new AbortController().signal };
const number = { parse(value) { if (typeof value !== "number" || value < 0) throw new TypeError("Positive number required"); return value; } };

test("one action parser serves native and external calls while retaining native value types", async () => {
  const action = defineAction()({
    id: "review.double", input: number, output: number, authorize() {}, run: (_context, value) => value * 2,
  });
  const executor = new Executor(() => {});
  assert.equal(await executor.call(action, context, 3), 6);
  assert.equal(await executor.callExternal(action, context, 3), 6);
  await assert.rejects(executor.call(action, context, -1), { code: "invalid_input" });
  await assert.rejects(executor.callExternal(action, context, -1), { code: "invalid_input" });
  const input = { callback: () => 42 };
  const identity = { id: "review.identity", input: { parse(value) { return { ...value }; } }, authorize() {}, run: (_context, value) => value };
  assert.equal((await executor.call(identity, context, input)).callback, input.callback);
  await assert.rejects(executor.call({ ...action, run: () => -1 }, context, 3), { code: "internal" });
});

test("declared schema normalization has the same meaning at native and external entrypoints", async () => {
  const trimmed = { parse(value) { if (typeof value !== "string") throw new TypeError("String required"); return value.trim(); } };
  const action = defineAction()({ id: "normalize", input: trimmed, output: trimmed, authorize() {}, run: (_context, value) => ` ${value} ` });
  const executor = new Executor(() => {});
  assert.equal(await executor.call(action, context, " alice "), "alice");
  assert.equal(await executor.callExternal(action, context, " alice "), "alice");
});

test("call completion owns asynchronous record completion", async () => {
  let release;
  let started;
  const ready = new Promise((resolve) => { started = resolve; });
  const pending = new Promise((resolve) => { release = resolve; });
  const executor = new Executor(async () => { started(); await pending; });
  let completed = false;
  const running = executor.call({ id: "record.await", authorize() {}, run: () => 42 }, context, undefined).then((value) => { completed = true; return value; });
  await ready;
  await Promise.resolve();
  assert.equal(completed, false);
  release();
  assert.equal(await running, 42);
});

const policies = JSON.parse(await readFile(new URL("../../conformance/fixtures/policy.json", import.meta.url)));
for (const fixture of policies) {
  test(`policy ${fixture.mode}: ${fixture.first}, ${fixture.second}`, async () => {
    const errors = new Map();
    const check = (code) => { if (code !== "ok") { const error = new ActionError(code, code); errors.set(code, error); throw error; } };
    let secondRuns = 0;
    const composed = (fixture.mode === "all" ? allOf : anyOf)(
      () => check(fixture.first), () => { secondRuns++; check(fixture.second); },
    );
    if (fixture.expected === "ok") await composed(context, undefined);
    else await assert.rejects(composed(context, undefined), (error) => {
      assert.equal(error.code, fixture.expected);
      if (fixture.expected === "internal") assert.equal(error, errors.get("internal"));
      return true;
    });
    assert.equal(secondRuns, fixture.secondRuns);
  });
}

test("resource configuration is validated before open; failed startup unwinds native dependencies", async () => {
  const seen = [];
  const resource = defineResource({
    id: "native", config: number,
    open(config) { seen.push(`open:${config}`); return { native: config }; },
    close(value) { seen.push(`close:${value.native}`); },
  });
  await assert.rejects(withResources({ first: resource.configure(1), second: resource.configure(-1) }, () => assert.fail("must not start")), { code: "invalid_input" });
  assert.deepEqual(seen, ["open:1", "close:1"]);
  const replacement = { open: () => new Map([["native", 7]]), close: () => {} };
  assert.equal(await withResources({ db: replacement }, ({ db }) => db.get("native")), 7);
});

test("each local subscription owns its own lifetime, even for the same function", async () => {
  const event = new Event("same.listener");
  let calls = 0;
  const handler = () => { calls++; };
  const unsubscribe = event.subscribe(handler);
  event.subscribe(handler);
  await event.publish(null);
  assert.equal(calls, 2);
  unsubscribe();
  await event.publish(null);
  assert.equal(calls, 3);
});

test("typed durable subscriptions participate in domain transactions and retain failed attempts", async () => {
  const db = new DatabaseSync(":memory:");
  let now = 1000;
  const queue = new SqliteQueue(db, () => now);
  const codec = jsonCodec(number);
  const task = { id: "typed.mail", input: codec, output: codec, run(context, value) {
    if (context.attempt === 1) throw new ActionError("internal", "Transient mail failure");
    return value;
  } };
  const event = new DurableEvent("typed.registered");
  event.subscribe("welcome", task, (user) => user.number);
  try {
    db.exec("CREATE TABLE users (id INTEGER); BEGIN IMMEDIATE; INSERT INTO users VALUES (1)");
    queue.publish(event, "user-1", { number: 7 }, { retryMs: 10 });
    db.exec("ROLLBACK");
    assert.equal(db.prepare("SELECT count(*) AS n FROM lenso_jobs").get().n, 0);
    const [receipt] = queue.publish(event, "user-1", { number: 7 }, { retryMs: 10 });
    assert.throws(() => queue.publish(event, "invalid-event", { number: -1 }), { code: "invalid_input" });
    await queue.runOne(task, {});
    now = 1010;
    await queue.runOne(task, {});
    assert.deepEqual(queue.attempts(receipt.id), [
      { attempt: 1, startedMs: 1000, finishedMs: 1000, outcome: "failed", error: "Transient mail failure" },
      { attempt: 2, startedMs: 1010, finishedMs: 1010, outcome: "succeeded", error: null },
    ]);
    assert.throws(() => event.subscribe("welcome", task, () => 8), { code: "conflict" });
  } finally { db.close(); }
});

test("unsafe scheduling values fail before creating or claiming work", async () => {
  const db = new DatabaseSync(":memory:");
  const queue = new SqliteQueue(db, () => Number.MAX_SAFE_INTEGER);
  const codec = jsonCodec(number);
  const task = { id: "timing", input: codec, output: codec, run: (_context, input) => input };
  try {
    assert.throws(() => queue.enqueue(task, 1, { maxAttempts: 0x1_0000_0000 }), { code: "invalid_input" });
    assert.throws(() => queue.enqueue(task, 1, { delayMs: 0.5, retryMs: 0 }), { code: "invalid_input" });
    assert.throws(() => queue.enqueue(task, 1, { delayMs: 1, retryMs: 0 }), { code: "invalid_input" });
    queue.enqueue(task, 1, { id: "overflow", retryMs: 0 });
    await assert.rejects(queue.runOne(task, {}, { leaseMs: 3 }), { code: "invalid_input" });
    assert.equal(queue.get("overflow").attempts, 0);
  } finally { db.close(); }
});

test("task payload and result failures retain their actual execution phase", async () => {
  const db = new DatabaseSync(":memory:");
  const queue = new SqliteQueue(db);
  const codec = jsonCodec(number);
  let runs = 0;
  const task = { id: "codecs", input: codec, output: codec, run: () => { runs++; return -1; } };
  try {
    assert.throws(() => queue.enqueue(task, -1), { code: "invalid_input" });
    queue.enqueueSerialized(task.id, "invalid-json", { id: "bad-input", maxAttempts: 1 });
    assert.equal((await queue.runOne(task, {})).error, "Invalid task payload");
    assert.equal(runs, 0);
    queue.enqueue(task, 1, { id: "bad-output", maxAttempts: 1 });
    assert.equal((await queue.runOne(task, {})).error, "Invalid output");
    assert.equal(runs, 1);
  } finally { db.close(); }
});

test("no-argument tasks persist the same JSON unit value as native Rust tasks", async () => {
  const db = new DatabaseSync(":memory:");
  try {
    const queue = new SqliteQueue(db);
    const task = { id: "cleanup", input: jsonCodec(emptyInput), output: jsonCodec(emptyInput), run() {} };
    const receipt = queue.enqueue(task, undefined);
    assert.equal(receipt.payload, "null");
    const result = await queue.runOne(task, {});
    assert.equal(result.state, "succeeded");
    assert.equal(result.result, "null");
  } finally { db.close(); }
});

test("Console descriptions, detail and custom actions invoke their actual authorized actions", async () => {
  const executor = new Executor(() => {});
  const authorize = (context) => requireScope(context.principal, "read");
  const list = { id: "review.list", input: emptyInput, authorize, run: () => [1] };
  const detail = { id: "review.detail", input: number, authorize, run: (_context, value) => ({ id: value }) };
  const update = { id: "review.update", input: number, authorize, run: (_context, value) => value + 1 };
  const catalog = new Catalog();
  catalog.register(new Module("review", { actions: { list, detail, update } }));
  const extension = new ConsoleExtension(executor, [{ id: "review", title: "Review", list: consoleOperation(list), detail: consoleOperation(detail), actions: [consoleOperation(update)] }]);
  assert.equal(extension.manifest(catalog).pages[0].detail, detail.id);
  assert.deepEqual(await extension.call(context, detail.id, 7), { id: 7 });
  assert.equal(await extension.call(context, update.id, 7), 8);
  await assert.rejects(extension.call({ ...context, principal: undefined }, update.id, 7), { code: "unauthenticated" });
  await assert.rejects(extension.call(context, update.id, "bad"), { code: "invalid_input" });
});

test("compiled custom Console pages receive the host client and cannot replace another extension", async () => {
  const client = new ConsoleClient("http://local.test/console", () => "token", async () => Response.json([7]));
  const pages = new ConsolePages(client);
  pages.register({ id: "first", pages: { custom: ({ client, page }) => client.call(page.list) } });
  const page = { id: "custom", title: "Custom", list: "review.list" };
  assert.deepEqual(await pages.render(page), [7]);
  assert.throws(() => pages.register({ id: "other", pages: { custom: () => null, extra: () => null } }), { code: "conflict" });
  assert.throws(() => pages.render({ ...page, id: "extra" }), { code: "not_found" });
});

test("Console accepts native actions directly and reuses one binding across pages", async () => {
  const records = [];
  const executor = new Executor((record) => { records.push(record); });
  const action = { id: "direct.console", input: number, authorize() {}, run: (_context, value) => value + 1 };
  const extension = new ConsoleExtension(executor, [
    { id: "one", title: "One", list: action, detail: action, actions: [action] },
    { id: "two", title: "Two", list: action },
  ]);
  assert.equal(await extension.call(context, action.id, 3), 4);
  await assert.rejects(extension.call(context, action.id, "bad"), { code: "invalid_input" });
  assert.deepEqual(records.map((record) => record.outcome), ["ok", "invalid_input"]);
  assert.throws(() => new ConsoleExtension(executor, [
    { id: "one", title: "One", list: action },
    { id: "two", title: "Two", list: { ...action, run: () => 99 } },
  ]), { code: "conflict" });
});
