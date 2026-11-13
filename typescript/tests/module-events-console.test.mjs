import assert from "node:assert/strict";
import test from "node:test";
import { Catalog, Event, Module, consoleManifest } from "@lenso/app";

test("module collisions fail without partially installing the module", () => {
  const read = { id: "files.read", authorize() {}, run: () => "native" };
  const files = new Module("files", { actions: { read }, resources: ["db"] });
  const catalog = new Catalog();
  catalog.register(files);
  assert.throws(() => catalog.requireResources([]), { code: "not_found" });
  catalog.requireResources(["db"]);
  assert.equal(files.exports.actions.read, read);
  assert.throws(() => catalog.register(new Module("other", { actions: { read } })), { code: "conflict" });
  assert.equal(catalog.describe().length, 1);
  const snapshot = catalog.describe();
  snapshot[0].actions.push("external.mutation");
  assert.deepEqual(catalog.describe()[0].actions, ["files.read"]);
  const manifest = consoleManifest(catalog, [{ id: "files", title: "Files", list: read.id }]);
  assert.equal(manifest.version, 1);
  assert.throws(() => consoleManifest(catalog, [{ id: "bad", title: "Bad", list: "missing" }]), { code: "not_found" });
});

test("subscription changes do not alter an in-flight local publication", async () => {
  const event = new Event("local.snapshot");
  const seen = [];
  let unsubscribe;
  event.subscribe(() => { unsubscribe(); seen.push("first"); });
  unsubscribe = event.subscribe(() => { seen.push("second"); });
  await event.publish(null);
  await event.publish(null);
  assert.deepEqual(seen, ["first", "second", "first"]);
});

test("local events await in order, support removal and stop on failure", async () => {
  const event = new Event("user.created");
  const seen = [];
  const unsubscribe = event.subscribe(async (value) => { await Promise.resolve(); seen.push(value.subject); });
  event.subscribe((value) => { seen.push(value.subject.toUpperCase()); });
  await event.publish({ subject: "alice", callback: () => {} });
  unsubscribe();
  await event.publish({ subject: "bob" });
  assert.deepEqual(seen, ["alice", "ALICE", "BOB"]);
  const failure = new Error("subscriber failed");
  event.subscribe(() => { throw failure; });
  event.subscribe(() => assert.fail("delivery stops at first failure"));
  await assert.rejects(event.publish({ subject: "carol" }), (error) => error === failure);
});
