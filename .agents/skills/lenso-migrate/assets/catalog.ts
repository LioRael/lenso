import assert from "node:assert/strict";
import { definePlugin, startApp, type Plugin } from "@lenso/core";

// Same ordinary service type, different configured instances. No wire contract.
interface Catalog { lookup(key: string): Promise<string | undefined> }
const closed: string[] = [];
function verifyClosed() {
  assert.deepEqual([...closed].sort(), ["north", "query-north", "query-south", "south"]);
  assert.ok(closed.indexOf("query-north") < closed.indexOf("north"));
  assert.ok(closed.indexOf("query-south") < closed.indexOf("south"));
}
function catalog(id: string, value: string): Plugin<Catalog> {
  return definePlugin({ id, setup(context) {
    const records = new Map([["shared", value]]); // acquired during setup
    let open = true;
    context.onCleanup(() => { open = false; records.clear(); closed.push(id); });
    return { async lookup(key: string) {
      if (!open) throw new Error("catalog closed");
      return records.get(key);
    } };
  } });
}
function query(id: string, source: Plugin<Catalog>, fail: boolean): Plugin<Catalog> {
  return definePlugin({ id, requires: [source], setup(context) {
    const resource = context.get(source); // exact installed object
    let open = true;
    context.onCleanup(() => { open = false; closed.push(id); });
    if (fail) throw new Error("deliberate setup failure");
    return { async lookup(key: string) {
      if (!open) throw new Error("query closed");
      return resource.lookup(key);
    } };
  } });
}

const fail = process.argv.includes("--fail");
const north = catalog("north", "North Ada");
const south = catalog("south", "South Grace");
const a = query("query-north", north, false);
const b = query("query-south", south, fail);
let app: Awaited<ReturnType<typeof startApp>>;
try {
  // Dependents intentionally precede providers.
  app = await startApp({ plugins: [a, b, north, south] });
} catch (error) {
  if (!fail) throw error;
  assert.match(String(error), /deliberate setup failure/);
  verifyClosed();
  console.log(JSON.stringify({ rollback: closed }));
  process.exit(0);
}
let service: Catalog | undefined;
let callFailed = false;
let callError: unknown;
try {
  assert.equal(fail, false, "expected setup failure");
  const active = app.get(a);
  service = active;
  assert.deepEqual(await Promise.all([active.lookup("shared"), app.get(b).lookup("shared")]),
    ["North Ada", "South Grace"]);
  assert.equal(await active.lookup("missing"), undefined);
} catch (error) { callFailed = true; callError = error; }
const failures: unknown[] = callFailed ? [callError] : [];
try { await app.stop(); } catch (error) { failures.push(error); }
if (failures.length) throw new AggregateError(failures, "native example failed");
await assert.rejects(() => service!.lookup("shared"), /query closed/);
verifyClosed();
console.log(JSON.stringify({ values: ["North Ada", "South Grace"], closed }));
