import assert from "node:assert/strict";
import test from "node:test";
import { withResource } from "@lenso/app/resource";

test("native resources close in reverse order when downstream acquisition fails", async () => {
  const events = [];
  const resource = (name) => ({
    open() { events.push(`open:${name}`); return { name }; },
    close(value) { events.push(`close:${value.name}`); },
  });
  const failure = new Error("connection refused");
  await assert.rejects(withResource(resource("db"), (db) => withResource(resource("storage"), async (storage) => {
    assert.equal(db.name, "db");
    assert.equal(storage.name, "storage");
    await withResource({ open() { throw failure; }, close() { assert.fail("not acquired"); } }, () => {});
  })), (error) => error === failure);
  assert.deepEqual(events, ["open:db", "open:storage", "close:storage", "close:db"]);
});

test("both use and close failures survive", async () => {
  const useError = new Error("query failed");
  const closeError = new Error("close failed");
  await assert.rejects(withResource({
    open: () => ({}), close() { throw closeError; },
  }, () => { throw useError; }), (error) => {
    assert.ok(error instanceof AggregateError);
    assert.deepEqual(error.errors, [useError, closeError]);
    return true;
  });
});

test("close completes before a successful use returns", async () => {
  let closed = false;
  const output = await withResource({ open: () => 7, async close() { await Promise.resolve(); closed = true; } }, (value) => value * 2);
  assert.equal(output, 14);
  assert.equal(closed, true);
});
