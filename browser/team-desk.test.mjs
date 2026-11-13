import assert from "node:assert/strict";
import { resolve } from "node:path";
import { chromium, webkit } from "playwright";
import { preview } from "vite";
import { after, before, test } from "node:test";
import { fixture } from "../integration/helpers/team-desk.mjs";
import { proxyFor } from "../typescript/examples/team-desk-console/vite.config.ts";

const root = resolve(import.meta.dirname, "../typescript/examples/team-desk-console");
const browserType = process.env.CONSOLE_BROWSER === "webkit" ? webkit : chromium;
let browser;
before(async () => { browser = await browserType.launch(process.env.CONSOLE_BROWSER_EXECUTABLE ? { executablePath: process.env.CONSOLE_BROWSER_EXECUTABLE } : browserType === chromium ? { channel: "chromium" } : {}); });
after(async () => { await browser?.close(); });
async function host(targets) {
  const server = await preview({ root, configFile: false, preview: { host: "127.0.0.1", port: 0, proxy: proxyFor(targets) }, logLevel: "silent" });
  const address = server.httpServer.address();
  assert.ok(address && typeof address !== "string");
  return { url: `http://127.0.0.1:${address.port}`, stop: () => new Promise((resolve) => server.httpServer.close(resolve)) };
}
async function connect(page, runtime, account = "alice") {
  await page.getByLabel("Backend", { exact: true }).selectOption(runtime);
  await page.getByLabel("Account", { exact: true }).selectOption(account);
  await page.getByRole("button", { name: "Connect", exact: true }).click();
  await page.getByRole("status").filter({ hasText: `Connected to ${runtime === "rust" ? "Rust" : "TypeScript"}` }).waitFor();
}
async function navigate(page, name) {
  await page.getByRole("navigation", { name: "Workspace pages" }).getByRole("button", { name, exact: true }).click();
  await page.getByRole("heading", { name, exact: true }).waitFor();
}
const pause = () => { let release; const promise = new Promise((resolve) => { release = resolve; }); return {promise,release}; };
for (const runtime of ["typescript", "rust"]) {
  test(`${browserType.name()} / ${runtime}: forms, pending writes, permissions and conflict recovery`, { timeout: 60_000 }, async () => fixture(async (f) => {
    const backend = await f.start(runtime);
    const server = await host({ typescript: backend.url, rust: backend.url });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (event) => { if (["error", "warning"].includes(event.type()) && !/Failed to load resource.*(401|403|409)/.test(event.text())) errors.push(event.text()); });
    try {
      await page.goto(server.url);
      assert.equal(await page.title(), "Team Desk Console");
      assert.equal(await page.locator("vite-error-overlay").count(), 0);
      await connect(page, runtime);
      await page.getByRole("heading", { name: "No projects yet" }).waitFor();
      await page.getByRole("button", { name: "New project", exact: true }).click();
      await page.getByRole("button", { name: "Create project", exact: true }).click();
      await page.getByRole("alert").filter({ hasText: "Check the highlighted fields" }).waitFor();
      assert.equal(await page.getByLabel("Project ID", { exact: true }).getAttribute("aria-invalid"), "true");
      await page.getByLabel("Project ID", { exact: true }).fill("launch");
      await page.getByLabel("Project name", { exact: true }).fill("Browser release");
      const gate = pause(); let submissions = 0;
      await page.route("**/console/actions/projects.create", async (route) => { submissions++; await gate.promise; await route.continue(); });
      await page.getByLabel("Project name", { exact: true }).press("Enter");
      await page.getByRole("button", { name: "Saving…", exact: true }).waitFor();
      assert.equal(await page.getByRole("button", { name: "Saving…", exact: true }).isDisabled(), true);
      gate.release();
      await page.getByText("Created Browser release.", { exact: true }).waitFor();
      assert.equal(submissions, 1);
      await page.unroute("**/console/actions/projects.create");
      await page.getByRole("button", { name: "Browser release", exact: true }).waitFor();
      await navigate(page, "Work items");
      await page.getByText("0 work items", { exact: true }).waitFor();
      await page.getByRole("button", { name: "New work item", exact: true }).click();
      await page.getByLabel("Work item ID", { exact: true }).fill("sdk");
      await page.getByLabel("Title", { exact: true }).fill("Ship browser workflow");
      await page.getByLabel("Project", { exact: true }).selectOption("launch");
      await page.getByLabel("Assignee", { exact: true }).selectOption("bob");
      await page.getByRole("button", { name: "Create work item", exact: true }).click();
      await page.getByRole("button", { name: "Ship browser workflow", exact: true }).click();
      await page.getByLabel("Assign to", { exact: true }).selectOption("alice");
      await backend.client("bob").call("items.transition", { id: "sdk", version: 1, status: "doing" });
      await page.getByRole("button", { name: "Save assignment", exact: true }).click();
      await page.getByRole("heading", { name: "This record changed", exact: true }).waitFor();
      assert.equal(await page.getByLabel("Assign to", { exact: true }).inputValue(), "alice");
      await page.screenshot({ path: `/tmp/lenso-console-${browserType.name()}-${runtime}-conflict.png`, fullPage: true, animations: "disabled" });
      await page.getByRole("button", { name: "Load latest version", exact: true }).click();
      await page.locator("[data-current-version]").filter({ hasText: "2" }).waitFor();
      assert.equal(await page.getByLabel("Assign to", { exact: true }).inputValue(), "alice");
      assert.equal((await backend.client("alice").call("items.detail", { id: "sdk" })).version, 2, "refresh must not resubmit the draft");
      await page.getByRole("button", { name: "Save assignment", exact: true }).click();
      await page.getByText("Assignment saved.", { exact: true }).waitFor();
      assert.equal((await backend.client("alice").call("items.detail", { id: "sdk" })).version, 3);
      await page.getByLabel("Next status", { exact: true }).selectOption("done");
      await page.getByRole("button", { name: "Change status", exact: true }).click();
      await page.getByText("Status saved.", { exact: true }).waitFor();
      assert.equal(await page.getByRole("button", { name: "Change status", exact: true }).isDisabled(), true);
      await page.screenshot({ path: `/tmp/lenso-console-${browserType.name()}-${runtime}-desktop.png`, fullPage: true, animations: "disabled" });
      await navigate(page, "Members");
      await page.getByRole("button", { name: "bob", exact: true }).click();
      await page.getByLabel("Role", { exact: true }).selectOption("viewer");
      await page.getByRole("button", { name: "Save role", exact: true }).click();
      await page.getByText("Role saved. Existing sessions use this permission immediately.", { exact: true }).waitFor();
      await navigate(page, "My notifications");
      await page.getByRole("button", { name: "Run notification worker", exact: true }).click();
      await page.getByText(/Delivered \d+ queued notifications?\./).waitFor();
      assert.ok((await backend.client("alice").call("notifications.list")).length > 0);
      await connect(page, runtime, "reader");
      await navigate(page, "Work items");
      await page.getByRole("button", { name: "Ship browser workflow", exact: true }).click();
      assert.equal(await page.getByRole("button", { name: "New work item", exact: true }).isDisabled(), true);
      assert.equal(await page.getByRole("button", { name: "Save assignment", exact: true }).isDisabled(), true);
      await assert.rejects(backend.client("reader").call("items.assign", { id: "sdk", version: 4, assignee: "alice" }), { code: "forbidden" });
      await connect(page, runtime, "eve");
      await page.getByRole("heading", { name: "No work items yet", exact: true }).waitFor();
      assert.equal(await page.getByRole("button", { name: "Ship browser workflow", exact: true }).count(), 0);
      await connect(page, runtime, "alice");
      await navigate(page, "Projects");
      await page.getByRole("button", { name: "Browser release", exact: true }).click();
      await page.getByRole("button", { name: "Archive project", exact: true }).click();
      await page.getByText("Project archived.", { exact: true }).waitFor();
      await navigate(page, "Work items");
      await page.getByRole("button", { name: "Ship browser workflow", exact: true }).click();
      assert.equal(await page.getByRole("button", { name: "Save assignment", exact: true }).isDisabled(), true);
      await page.setViewportSize({ width: 390, height: 844 });
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), "mobile page must not overflow horizontally");
      await page.screenshot({ path: `/tmp/lenso-console-${browserType.name()}-${runtime}-mobile.png`, fullPage: true, animations: "disabled" });
      assert.deepEqual(errors, []);
    } finally { await context.close(); await server.stop(); }
  }));
}

for (const runtime of ["typescript", "rust"]) {
  test(`${browserType.name()} / ${runtime}: live role revocation preserves a draft and disables every write control`, { timeout: 30_000 }, async () => fixture(async (f) => {
    const backend = await f.start(runtime);
    const alice = backend.client("alice");
    await alice.call("projects.create", { id: "p1", name: "Permission checks" });
    await alice.call("items.create", { id: "i1", project: "p1", title: "Review access", assignee: "bob" });
    const server = await host({ typescript: backend.url, rust: backend.url });
    const context = await browser.newContext();
    const page = await context.newPage();
    const errors = []; page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (event) => { if (["error", "warning"].includes(event.type()) && !/Failed to load resource.*(401|403|409)/.test(event.text())) errors.push(event.text()); });
    try {
      await page.goto(server.url); await connect(page, runtime, "bob");
      await navigate(page, "Work items");
      await page.getByRole("button", { name: "Review access", exact: true }).click();
      await page.getByLabel("Assign to", { exact: true }).selectOption("alice");
      await alice.call("members.setRole", { subject: "bob", role: "viewer" });
      await page.getByRole("button", { name: "Save assignment", exact: true }).click();
      await page.getByText("Your permission changed or this action is not allowed. Your draft has been kept.", { exact: true }).waitFor();
      await page.getByText("alpha · Read-only", { exact: true }).waitFor();
      assert.equal(await page.getByLabel("Assign to", { exact: true }).inputValue(), "alice");
      for (const name of ["Save assignment", "Change status", "New work item"]) assert.equal(await page.getByRole("button", { name, exact: true }).isDisabled(), true);
      assert.equal((await alice.call("items.detail", { id: "i1" })).version, 1);
      await page.screenshot({ path: `/tmp/lenso-console-${browserType.name()}-${runtime}-permission.png`, fullPage: true, animations: "disabled" });
      assert.deepEqual(errors, []);
    } finally { await context.close(); await server.stop(); }
  }));
}

test(`${browserType.name()}: backend reconnect cancels late page responses and keeps native data isolated`, { timeout: 30_000 }, async () => fixture(async (f) => {
  const ts = await f.start("typescript"), rust = await f.start("rust", `${f.filename}.rust`);
  await ts.client("alice").call("projects.create", { id: "ts", name: "TypeScript workspace" });
  await rust.client("alice").call("projects.create", { id: "rust", name: "Rust workspace" });
  const server = await host({ typescript: ts.url, rust: rust.url });
  const context = await browser.newContext(); const page = await context.newPage();
  const gate = pause(), intercepted = pause();
  const errors = []; page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (event) => { if (["error", "warning"].includes(event.type()) && !/Failed to load resource.*(401|403|409)/.test(event.text())) errors.push(event.text()); });
  try {
    await page.goto(server.url); await connect(page, "typescript");
    await page.getByRole("button", { name: "TypeScript workspace", exact: true }).waitFor();
    await page.route("**/backends/typescript/console/actions/projects.list", async (route) => {
      const response = await route.fetch(); intercepted.release(); await gate.promise;
      try { await route.fulfill({ response }); } catch { /* Reconnect intentionally aborts this response. */ }
    });
    await page.getByRole("button", { name: "Refresh list", exact: true }).click();
    await intercepted.promise;
    await connect(page, "rust");
    await page.getByRole("button", { name: "Rust workspace", exact: true }).waitFor();
    gate.release();
    assert.equal(await page.getByRole("button", { name: "TypeScript workspace", exact: true }).count(), 0);
    await page.getByRole("button", { name: "New project", exact: true }).click();
    await page.getByLabel("Project ID", { exact: true }).fill("rust_new");
    await page.getByLabel("Project name", { exact: true }).fill("Created through Rust");
    await page.getByRole("button", { name: "Create project", exact: true }).click();
    await page.getByText("Created Created through Rust.", { exact: true }).waitFor();
    assert.equal((await ts.client("alice").call("projects.list")).length, 1);
    assert.equal((await rust.client("alice").call("projects.list")).length, 2);
    await page.getByLabel("Account", { exact: true }).selectOption("custom");
    await page.getByLabel("Bearer token", { exact: true }).fill("not-a-valid-token");
    await page.getByRole("button", { name: "Connect", exact: true }).click();
    await page.getByRole("heading", { name: "Connection unavailable", exact: true }).waitFor();
    assert.equal(await page.getByRole("button", { name: "Rust workspace", exact: true }).count(), 0);
    assert.deepEqual(errors, []);
  } finally { gate.release(); await context.close(); await server.stop(); }
}));

test(`${browserType.name()}: switching records cannot apply a late mutation result to the new editor`, { timeout: 30_000 }, async () => fixture(async (f) => {
  const backend = await f.start("typescript"), alice = backend.client("alice");
  await alice.call("projects.create", { id: "p1", name: "Editor lifetimes" });
  for (const id of ["first", "second"]) await alice.call("items.create", { id, project: "p1", title: `Record ${id}`, assignee: "bob" });
  const server = await host({ typescript: backend.url, rust: backend.url });
  const context = await browser.newContext(); const page = await context.newPage();
  const gate = pause(), intercepted = pause();
  try {
    await page.goto(server.url); await connect(page, "typescript"); await navigate(page, "Work items");
    await page.getByRole("button", { name: "Record first", exact: true }).click();
    await page.getByLabel("Assign to", { exact: true }).selectOption("alice");
    await page.route("**/console/actions/items.assign", async (route) => {
      const response = await route.fetch(); intercepted.release(); await gate.promise;
      try { await route.fulfill({ response }); } catch { /* The old editor deliberately cancels its request. */ }
    });
    await page.getByRole("button", { name: "Save assignment", exact: true }).click(); await intercepted.promise;
    await page.getByRole("button", { name: "Record second", exact: true }).click();
    gate.release();
    await page.getByRole("heading", { name: "Record second", exact: true }).waitFor();
    assert.equal(await page.locator("[data-current-version]").innerText(), "1");
    assert.equal(await page.getByLabel("Assign to", { exact: true }).inputValue(), "bob");
    assert.equal((await alice.call("items.detail", { id: "first" })).version, 2, "cancelled UI requests do not imply rollback of committed writes");
    assert.equal((await alice.call("items.detail", { id: "second" })).version, 1);
    await page.getByRole("button", { name: "Refresh list", exact: true }).click();
    await page.getByText("2 work items", { exact: true }).waitFor();
    assert.equal(await page.locator("[data-current-version]").innerText(), "1");
  } finally { gate.release(); await context.close(); await server.stop(); }
}));

test(`${browserType.name()}: malformed write results are not reported as user input errors`, { timeout: 30_000 }, async () => fixture(async (f) => {
  const backend = await f.start("typescript"), alice=backend.client("alice");
  await alice.call("projects.create",{id:"p1",name:"Response contracts"});
  await alice.call("items.create",{id:"i1",project:"p1",title:"Preserve the draft",assignee:"bob"});
  const server=await host({typescript:backend.url,rust:backend.url});
  const context=await browser.newContext(), page=await context.newPage();
  try {
    await page.goto(server.url); await connect(page,"typescript"); await navigate(page,"Work items");
    await page.getByRole("button",{name:"Preserve the draft",exact:true}).click();
    await page.getByLabel("Assign to",{exact:true}).selectOption("alice");
    await page.route("**/console/actions/items.assign",async route=>{
      const response=await route.fetch(); await route.fulfill({response,json:{broken:"response"}});
    });
    await page.getByRole("button",{name:"Save assignment",exact:true}).click();
    await page.getByRole("alert").filter({hasText:"Invalid Console action response"}).waitFor();
    assert.equal(await page.getByLabel("Assign to",{exact:true}).inputValue(),"alice");
    assert.equal(await page.locator('[aria-invalid="true"]').count(),0);
    assert.equal(await page.getByText("Check the highlighted fields.",{exact:true}).count(),0);
    assert.equal((await alice.call("items.detail",{id:"i1"})).version,2,"response failure does not roll back the write");
  } finally { await context.close(); await server.stop(); }
}));

test(`${browserType.name()}: an older session refresh cannot replace a newer permission result`, {timeout:30_000}, async()=>fixture(async f=>{
  const backend=await f.start("typescript"), alice=backend.client("alice");
  await alice.call("projects.create",{id:"p1",name:"Session ordering"});
  await alice.call("items.create",{id:"i1",project:"p1",title:"Keep the current role",assignee:"bob"});
  const server=await host({typescript:backend.url,rust:backend.url});
  const context=await browser.newContext(), page=await context.newPage();
  const gate=pause(), captured=pause(), delivered=pause(); let requests=0;
  try {
    await page.goto(server.url); await connect(page,"typescript","bob"); await navigate(page,"Work items");
    await page.getByRole("button",{name:"Keep the current role",exact:true}).click();
    await page.getByLabel("Assign to",{exact:true}).selectOption("alice");
    await page.route("**/console/actions/members.session",async route=>{
      requests++;
      if(requests===1){const response=await route.fetch();captured.release();await gate.promise;await route.fulfill({response});delivered.release();}
      else await route.continue();
    });
    await page.getByRole("button",{name:"Refresh list",exact:true}).click(); await captured.promise;
    await alice.call("members.setRole",{subject:"bob",role:"viewer"});
    await page.getByRole("button",{name:"Save assignment",exact:true}).click();
    await page.getByText("alpha · Read-only",{exact:true}).waitFor();
    gate.release(); await delivered.promise;
    await page.getByRole("button",{name:"Refresh list",exact:true}).waitFor({state:"visible"});
    await page.waitForFunction(()=>!document.querySelector('[data-refresh]').disabled);
    assert.equal(await page.getByText("alpha · Read-only",{exact:true}).count(),1);
    assert.equal(await page.getByRole("button",{name:"Save assignment",exact:true}).isDisabled(),true);
  } finally{gate.release();await context.close();await server.stop();}
}));
