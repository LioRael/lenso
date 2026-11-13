import assert from "node:assert/strict";
import { readFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { ActionError, checkCancellation } from "@lenso/app";
import { SqliteQueue } from "@lenso/sqlite";

const fixtures = JSON.parse(await readFile(new URL("../../conformance/fixtures/task.json", import.meta.url)));
const codec = { encode: JSON.stringify, decode: JSON.parse };
const identity = { id: "test.identity", input: codec, output: codec, run: (_context, input) => input };
const leaseMs = 3000;

for (const fixture of fixtures) {
  test(`durable task: ${fixture.name}`, async () => {
    const db = new DatabaseSync(":memory:");
    let now = 1000;
    const queue = new SqliteQueue(db, () => now);
    const task = { ...identity, run(context, input) {
      if (fixture.cancelDuring) queue.cancel(context.jobId);
      if (context.attempt <= fixture.failures) throw new ActionError("internal", "Temporary failure");
      return input;
    } };
    try {
      queue.enqueue(task, { message: "hello" }, { id: "receipt", maxAttempts: fixture.maxAttempts, delayMs: fixture.delayMs, retryMs: fixture.retryMs });
      if (fixture.cancel) assert.equal(queue.cancel("receipt"), true);
      for (const step of fixture.steps) {
        now = step.now;
        const job = await queue.runOne(task, {}, { leaseMs });
        assert.equal(job?.state ?? "idle", step.state);
        assert.equal(queue.get("receipt").attempts, step.attempts);
        if (job?.state === "succeeded") assert.deepEqual(JSON.parse(job.result), { message: "hello" });
      }
    } finally { db.close(); }
  });
}

test("receipt reuse preserves the original policy and rejects a different payload", () => {
  const db = new DatabaseSync(":memory:");
  try {
    const queue = new SqliteQueue(db, () => 1000);
    const first = queue.enqueue(identity, 1, { id: "same", maxAttempts: 2, delayMs: 100 });
    assert.deepEqual(queue.enqueue(identity, 1, { id: "same", maxAttempts: 9, delayMs: 0 }), first);
    assert.throws(() => queue.enqueue(identity, 2, { id: "same" }), { code: "conflict" });
    assert.throws(() => queue.enqueue(identity, 1, { maxAttempts: 0 }), { code: "invalid_input" });
  } finally { db.close(); }
});

test("expired leases recover after reopening; stale and exhausted attempts cannot commit", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-queue-"));
  const filename = join(directory, "jobs.sqlite");
  let db = new DatabaseSync(filename);
  try {
    let queue = new SqliteQueue(db, () => 1000);
    queue.enqueue(identity, 7, { id: "recover", maxAttempts: 2 });
    queue.enqueue(identity, 8, { id: "exhausted", maxAttempts: 1 });
    db.prepare("UPDATE lenso_jobs SET state = 'running', attempts = 1, lease_token = 'dead-worker', lease_until = 999").run();
    db.close();
    db = new DatabaseSync(filename);
    queue = new SqliteQueue(db, () => 1000);
    assert.equal((await queue.runOne(identity, {}, { leaseMs })).state, "succeeded");
    assert.equal(queue.get("recover").attempts, 2);
    assert.equal(queue.get("exhausted").state, "failed");
    assert.equal(db.prepare("UPDATE lenso_jobs SET result = 'stale' WHERE lease_token = 'dead-worker'").run().changes, 0);
  } finally { db.close(); await rm(directory, { recursive: true, force: true }); }
});

test("two connections respect the persisted concurrency limit while a handler waits", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-concurrency-"));
  const filename = join(directory, "jobs.sqlite");
  const first = new DatabaseSync(filename);
  const second = new DatabaseSync(filename);
  let release;
  let started;
  const ready = new Promise((resolve) => { started = resolve; });
  const waiting = new Promise((resolve) => { release = resolve; });
  try {
    const a = new SqliteQueue(first);
    const b = new SqliteQueue(second);
    const task = { ...identity, async run() { started(); await waiting; return 42; } };
    a.enqueue(task, 1, { id: "one" });
    a.enqueue(task, 2, { id: "two" });
    const running = a.runOne(task, {}, { leaseMs: 90 });
    await ready;
    assert.equal(await b.runOne(identity, {}, { leaseMs: 90 }), undefined);
    await new Promise((resolve) => setTimeout(resolve, 140));
    assert.equal(await b.runOne(identity, {}, { leaseMs: 90 }), undefined, "heartbeat prevents reclaim beyond initial lease");
    release();
    assert.equal((await running).state, "succeeded");
    assert.equal((await b.runOne(identity, {}, { leaseMs: 90 })).state, "succeeded");
  } finally { release?.(); first.close(); second.close(); await rm(directory, { recursive: true, force: true }); }
});

test("running cancellation reaches a cooperative handler and is persisted", async () => {
  const db = new DatabaseSync(":memory:");
  try {
    const queue = new SqliteQueue(db);
    let started;
    const ready = new Promise((resolve) => { started = resolve; });
    const task = { ...identity, async run(context) {
      started();
      await new Promise((resolve) => context.signal.addEventListener("abort", resolve, { once: true }));
      checkCancellation(context.signal);
    } };
    queue.enqueue(task, 1, { id: "cancel-running" });
    const running = queue.runOne(task, {}, { leaseMs: 30 });
    await ready;
    assert.equal(queue.cancel("cancel-running"), true);
    assert.equal((await running).state, "cancelled");
    assert.equal(queue.cancel("cancel-running"), false);
  } finally { db.close(); }
});

test("lost leases reject stale results without overwriting a replacement worker", async () => {
  const db = new DatabaseSync(":memory:");
  try {
    const queue = new SqliteQueue(db);
    const task = { ...identity, run() {
      db.prepare("UPDATE lenso_jobs SET lease_token = 'replacement'").run();
      return 42;
    } };
    queue.enqueue(task, 1, { id: "stale" });
    await assert.rejects(queue.runOne(task, {}, { leaseMs }), { code: "conflict" });
    assert.equal(queue.get("stale").state, "running");
    assert.equal(queue.get("stale").result, null);
    assert.equal(queue.attempts("stale")[0].outcome, "running");
    assert.equal(queue.attempts("stale")[0].finishedMs, null);
  } finally { db.close(); }
});

test("event fanout deduplicates subscriptions and rolls back all new jobs on conflict", () => {
  const db = new DatabaseSync(":memory:");
  try {
    const queue = new SqliteQueue(db, () => 1000);
    const deliveries = [
      { subscription: "welcome", task: identity.id, payload: "1" },
      { subscription: "audit", task: identity.id, payload: "2" },
    ];
    const first = queue.publishSerialized("user.created", "event-1", deliveries);
    assert.deepEqual(queue.publishSerialized("user.created", "event-1", deliveries), first);
    assert.throws(() => queue.publishSerialized("user.created", "event-1", [
      { subscription: "new", task: identity.id, payload: "3" }, { ...deliveries[0], payload: "different" },
    ]), { code: "conflict" });
    assert.equal(db.prepare("SELECT count(*) AS n FROM lenso_jobs").get().n, 2);
    assert.throws(() => queue.publishSerialized("user.created", "event-2", [deliveries[0], deliveries[0]]), { code: "conflict" });
  } finally { db.close(); }
});

test("enqueue participates in the application's native database transaction", () => {
  const db = new DatabaseSync(":memory:");
  try {
    const queue = new SqliteQueue(db);
    db.exec("CREATE TABLE invitations (id TEXT PRIMARY KEY); BEGIN IMMEDIATE;");
    db.prepare("INSERT INTO invitations VALUES ('invite-1')").run();
    queue.enqueue(identity, { invitation: "invite-1" }, { id: "invite-mail" });
    db.exec("ROLLBACK");
    assert.equal(db.prepare("SELECT count(*) AS n FROM invitations").get().n, 0);
    assert.throws(() => queue.get("invite-mail"), { code: "not_found" });
  } finally { db.close(); }
});
