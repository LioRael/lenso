import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fixture, identities } from "./helpers/team-desk.mjs";

const createProject = { id: "p1", name: "  Release planning  " };
const createItem = { id: "i1", project: "p1", title: "  Ship SDK  ", assignee: "bob" };
for (const runtime of ["typescript", "rust"]) {
  test(`${runtime}: independent modules, Console writes, tenancy, live roles, versions and restart`, async () => fixture(async (f) => {
    let server = await f.start(runtime);
    let alice = server.client("alice"), bob = server.client("bob"), reader = server.client("reader"), eve = server.client("eve");
    await assert.rejects(server.client("forged").manifest(), { code: "unauthenticated" });
    const manifest = await alice.manifest();
    assert.deepEqual(manifest.modules.map((module) => module.id), ["members", "projects", "items", "notifications"]);
    assert.deepEqual(manifest.pages.find((page) => page.id === "items").actions, ["items.create", "items.assign", "items.transition"]);
    await assert.rejects(reader.call("projects.create", createProject), { code: "forbidden" });
    const project = await alice.call("projects.create", { ...createProject, team: "beta" });
    assert.equal(project.name, "Release planning");
    assert.deepEqual(await eve.call("projects.list"), []);
    await assert.rejects(eve.call("projects.detail", { id: "p1" }), { code: "not_found" });
    await assert.rejects(alice.call("projects.create", createProject), { code: "conflict" });
    await assert.rejects(alice.call("projects.create", { id: "bad", name: " " }), { code: "invalid_input" });
    await assert.rejects(alice.call("items.create", { ...createItem, assignee: "eve" }), { code: "invalid_input" });
    await assert.rejects(alice.call("items.create", { ...createItem, assignee: "reader" }), { code: "invalid_input" });
    const item = await bob.call("items.create", createItem);
    assert.deepEqual(item, { ...createItem, title: "Ship SDK", status: "open", version: 1 });
    await assert.rejects(reader.call("items.assign", { id: "i1", version: 1, assignee: "alice" }), { code: "forbidden" });
    await assert.rejects(bob.call("items.transition", { id: "i1", version: 1, status: "done" }), { code: "conflict" });
    await assert.rejects(bob.call("items.transition", { id: "i1", version: 1, status: "unknown" }), { code: "invalid_input" });
    const races = await Promise.allSettled([
      bob.call("items.transition", { id: "i1", version: 1, status: "doing" }),
      alice.call("items.assign", { id: "i1", version: 1, assignee: "alice" }),
    ]);
    assert.equal(races.filter((value) => value.status === "fulfilled").length, 1);
    assert.equal(races.find((value) => value.status === "rejected").reason.code, "conflict");
    let current = await alice.call("items.detail", { id: "i1" });
    assert.equal(current.version, 2);
    current = await alice.call("items.assign", { id: "i1", version: current.version, assignee: current.assignee === "alice" ? "bob" : "alice" });
    if (current.status === "open") current = await bob.call("items.transition", { id: "i1", version: current.version, status: "doing" });
    current = await bob.call("items.transition", { id: "i1", version: current.version, status: "done" });
    assert.equal(current.status, "done");
    await assert.rejects(bob.call("members.setRole", { subject: "reader", role: "editor" }), { code: "forbidden" });
    await assert.rejects(alice.call("members.setRole", { subject: "alice", role: "viewer" }), { code: "conflict" });
    await alice.call("members.setRole", { subject: "bob", role: "viewer" });
    await assert.rejects(bob.call("items.assign", { id: "i1", version: current.version, assignee: "alice" }), { code: "forbidden" });
    await alice.call("members.setRole", { subject: "bob", role: "editor" });
    await server.stop();
    server = await f.start(runtime); alice = server.client("alice"); bob = server.client("bob");
    assert.deepEqual(await alice.call("items.detail", { id: "i1" }), current);
    assert.equal(await server.drain(), current.version, "committed publications survive restart without a running worker");
    assert.equal(await server.drain(), 0);
    assert.equal((await bob.call("notifications.list")).length + (await alice.call("notifications.list")).length, current.version);
    assert.deepEqual(await server.client("eve").call("notifications.list"), []);
    const archived = await alice.call("projects.archive", { id: "p1", version: 1 });
    assert.equal(archived.archived, 1);
    await assert.rejects(alice.call("items.assign", { id: "i1", version: current.version, assignee: "alice" }), { code: "conflict" });
    await assert.rejects(alice.call("items.create", { ...createItem, id: "i2" }), { code: "conflict" });
    const db = new DatabaseSync(f.filename);
    try {
      assert.ok(db.prepare("SELECT count(*) AS n FROM desk_calls WHERE action='members.setRole' AND outcome='forbidden'").get().n > 0);
      assert.equal(db.prepare("SELECT count(*) AS n FROM desk_notifications").get().n, current.version);
    } finally { db.close(); }
  }));
  test(`${runtime}: native publication failure rolls back the Console domain write`, async () => fixture(async (f) => {
    const server = await f.start(runtime), alice = server.client("alice");
    await alice.call("projects.create", createProject);
    const db = new DatabaseSync(f.filename);
    try {
      db.exec("CREATE TRIGGER fail_publication BEFORE INSERT ON lenso_jobs BEGIN SELECT RAISE(ABORT,'injected publication failure'); END");
      await assert.rejects(alice.call("items.create", createItem), { code: "internal" });
      assert.equal(db.prepare("SELECT count(*) AS n FROM desk_items").get().n, 0);
      assert.equal(db.prepare("SELECT count(*) AS n FROM lenso_jobs").get().n, 0);
      db.exec("DROP TRIGGER fail_publication");
      assert.equal((await alice.call("items.create", createItem)).version, 1);
      assert.equal(await server.drain(), 1);
    } finally { db.close(); }
  }));
}
for (const [producer, consumer] of [["typescript", "rust"], ["rust", "typescript"]]) {
  test(`${producer} -> ${consumer}: pending events and Console writes use the same persisted contract`, async () => fixture(async (f) => {
    let server = await f.start(producer);
    await server.client("alice").call("projects.create", createProject);
    await server.client("alice").call("items.create", createItem);
    await server.stop();
    server = await f.start(consumer);
    assert.equal(await server.drain(), 1);
    assert.equal((await server.client("bob").call("notifications.list")).length, 1);
    assert.equal((await server.client("bob").call("items.transition", { id: "i1", version: 1, status: "doing" })).version, 2);
    assert.equal(await server.drain(), 1);
  }));
}

test("TypeScript: native module exports stay typed; modules can be installed without the notification module", async () => {
  const { Store } = await import("../typescript/examples/team-desk/dist/store.js");
  const { createApplication } = await import("../typescript/examples/team-desk/dist/app.js");
  const { projectsModule } = await import("../typescript/examples/team-desk/dist/projects.js");
  const { Catalog, ConsoleExtension, Executor } = await import("@lenso/app");
  const store = new Store(":memory:", identities);
  try {
    const app = createApplication(store);
    const context = store.context(identities[0].token, "native", new AbortController().signal);
    context.principal.scopes.unshift("console:read");
    assert.equal((await app.executor.call(app.modules.projects.actions.create, context, createProject)).name, "Release planning");
    assert.equal((await app.executor.call(app.modules.items.actions.create, context, createItem)).status, "open");
    const projects = projectsModule(), catalog = new Catalog();
    catalog.register(projects.module);
    const console = new ConsoleExtension(new Executor(() => {}), projects.pages);
    assert.deepEqual(console.manifest(catalog).modules.map((module) => module.id), ["projects"]);
    assert.equal((await console.call(context, "projects.list", null)).length, 1);
    await assert.rejects(console.call(context, "items.list", null), { code: "not_found" });
  } finally { store.close(); }
});

test("live TS and Rust processes coordinate the same optimistic write and notification queue", async () => fixture(async (f) => {
  const ts = await f.start("typescript"), rust = await f.start("rust");
  await ts.client("alice").call("projects.create", createProject);
  await rust.client("alice").call("items.create", createItem);
  const results = await Promise.allSettled([
    ts.client("bob").call("items.transition", { id: "i1", version: 1, status: "doing" }),
    rust.client("bob").call("items.transition", { id: "i1", version: 1, status: "doing" }),
  ]);
  assert.equal(results.filter((value) => value.status === "fulfilled").length, 1);
  assert.equal(results.find((value) => value.status === "rejected").reason.code, "conflict");
  const delivered = await Promise.all([ts.drain(), rust.drain()]);
  assert.equal(delivered[0] + delivered[1], 2);
  assert.equal((await ts.client("bob").call("notifications.list")).length, 2);
}));

const textFixtures = JSON.parse(await readFile(new URL("../examples/team-desk/text-fixtures.json", import.meta.url)));
for (const runtime of ["typescript", "rust"]) {
  test(`${runtime}: Console text normalization agrees for copied Unicode whitespace`, async () => fixture(async (f) => {
    const server = await f.start(runtime), alice = server.client("alice");
    await alice.call("projects.create", createProject);
    for (const example of textFixtures) {
      const project = { id: example.name, name: example.input };
      const item = { ...createItem, id: example.name, title: example.input };
      if (example.expected === null) {
        await assert.rejects(alice.call("projects.create", project), { code: "invalid_input" });
        await assert.rejects(alice.call("items.create", item), { code: "invalid_input" });
      } else {
        assert.equal((await alice.call("projects.create", project)).name, example.expected, example.name);
        assert.equal((await alice.call("items.create", item)).title, example.expected, example.name);
      }
    }
  }));
}

for (const runtime of ["typescript","rust"]) {
  test(`${runtime}: Console URL query parameters do not change operation routing`, async () => fixture(async(f)=>{
    const server=await f.start(runtime);
    const {ConsoleClient}=await import("@lenso/console");
    const client=new ConsoleClient(`${server.url}/console?region=local`,()=>identities[0].token);
    assert.equal((await client.manifest()).pages.length,4);
    assert.equal((await client.call("members.session")).subject,"alice");
    await client.call("projects.create",{id:"query",name:"Query route"});
    assert.equal((await client.call("projects.list"))[0].id,"query");
  }));
}
