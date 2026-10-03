// One provider process per corpus. Typed JSON requests delegate to actual SQL.
// stdout is reserved for the bounded protocol; runtime diagnostics go to stderr.
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const selected = process.argv[2];
const began = performance.now();
const phases = { provider: selected, boot_ms: 0, delegated_calls_ms: 0, restart_ms: 0, cleanup_ms: 0, calls: 0 };
let backend;
if (selected === "postgres") {
  const { default: pg } = await import("pg");
  const schema = `store_contract_${process.pid}`;
  let pool;
  const connect = () => {
    pool = new pg.Pool({
      connectionString: process.env.LENSO_STORE_PG_URL,
      max: 2,
      connectionTimeoutMillis: 5000,
      options: `-c search_path=${schema} -c statement_timeout=5000`,
    });
  };
  assert.ok(process.env.LENSO_STORE_PG_URL, "LENSO_STORE_PG_URL is required");
  connect();
  await pool.query(`CREATE SCHEMA ${schema}`);
  await pool.query("CREATE TABLE receipts (id TEXT PRIMARY KEY, amount INTEGER NOT NULL CHECK(amount > 0)); CREATE TABLE effects (id TEXT PRIMARY KEY, amount INTEGER NOT NULL CHECK(amount > 0))");
  backend = {
    async call(command) {
      if (command.action === "version") {
        return (await pool.query("SELECT current_setting('server_version') AS version")).rows[0];
      }
      if (command.action === "restart") {
        await pool.end();
        connect();
        return { ready: true };
      }
      if (command.action === "read") {
        const receipt = (await pool.query("SELECT id, amount FROM receipts WHERE id = $1", [command.id])).rows[0] ?? null;
        const effect = (await pool.query("SELECT id, amount FROM effects WHERE id = $1", [command.id])).rows[0] ?? null;
        return { receipt, effect };
      }
      assert.equal(command.action, "apply");
      const client = await pool.connect();
      let discard = false;
      try {
        await client.query("BEGIN");
        await client.query("INSERT INTO receipts(id, amount) VALUES ($1, $2) ON CONFLICT(id) DO NOTHING", [command.id, command.amount]);
        await client.query("INSERT INTO effects(id, amount) SELECT id, CASE WHEN $1 THEN -1 ELSE amount END FROM receipts WHERE id = $2 ON CONFLICT(id) DO NOTHING", [command.rollback, command.id]);
        await client.query("COMMIT");
        const value = (await client.query("SELECT id, amount FROM receipts WHERE id = $1", [command.id])).rows[0];
        return { kind: "committed", value };
      } catch (error) {
        // ROLLBACK succeeding after a COMMIT transport error does NOT prove the
        // earlier commit failed. Confirm only our deliberate SQLSTATE 23514.
        let rolledBack = false;
        try { await client.query("ROLLBACK"); rolledBack = true; } catch { discard = true; }
        return command.rollback && error.code === "23514" && rolledBack
          ? { kind: "rolled_back", error: "constraint" }
          : { kind: "unknown", error: "backend" };
      } finally {
        client.release(discard);
      }
    },
    async close() {
      await pool.query(`DROP SCHEMA ${schema} CASCADE`);
      await pool.end();
    },
  };
} else if (selected === "d1") {
  for (const key of ["MINIFLARE_WORKERD_PATH", "MINIFLARE_WORKERD_AUTOGATES", "MINIFLARE_WORKERD_V8_FLAGS"]) {
    assert.ok(!process.env[key], `${key} must be unset for the pinned runtime gate`);
  }
  const { Miniflare, convertV4MiniflareOptions } = await import("miniflare");
  const miniflareVersion = JSON.parse(await readFile(new URL("node_modules/miniflare/package.json", import.meta.url))).version;
  const workerdVersion = JSON.parse(await readFile(new URL("node_modules/workerd/package.json", import.meta.url))).version;
  const directory = await mkdtemp(join(tmpdir(), "lenso-store-contract-"));
  const create = () => new Miniflare(convertV4MiniflareOptions({
    resourcePersistencePath: directory,
    workers: [{
      name: "durable-store-contract",
      compatibilityDate: "2026-10-01",
      d1Databases: { DB: "durable-store-contract" },
      modulesRoot: fileURLToPath(new URL(".", import.meta.url)),
      modules: [{ type: "ESModule", path: fileURLToPath(new URL("d1.worker.mjs", import.meta.url)) }],
    }],
  }));
  let runtime = create();
  const call = async (command) => {
    const response = await runtime.dispatchFetch("http://localhost/contract", {
      method: "POST", body: JSON.stringify(command),
    });
    if (response.status !== 200) {
      throw new Error(`D1 fixture ${command.action}: HTTP ${response.status}: ${(await response.text()).slice(0,2048)}`);
    }
    return response.json();
  };
  await call({ action: "migrate" });
  backend = {
    async call(command) {
      if (command.action === "version") {
        // D1 deliberately disallows sqlite_version(); identify the actual local
        // runtime from its installed packages, without adding SQL emulation.
        return { version: `miniflare ${miniflareVersion}; workerd ${workerdVersion}` };
      }
      if (command.action === "restart") {
        await runtime.dispose();
        runtime = create();
        await runtime.ready;
        return { ready: true };
      }
      return call(command);
    },
    async close() { await runtime.dispose(); await rm(directory, { recursive: true, force: true }); },
  };
} else {
  throw new Error("provider must be postgres or d1");
}

phases.boot_ms = performance.now() - began;
process.stdout.write(JSON.stringify({ ready: true, provider: selected }) + "\n");
try {
  for await (const line of createInterface({ input: process.stdin, crlfDelay: Infinity })) {
    assert.ok(line.length < 8192, "bounded fixture request");
    const command = JSON.parse(line);
    if (command.action === "close") break;
    const operationBegan = performance.now();
    let response;
    if (command.action === "race") {
      const replies = await Promise.all(command.amounts.map((amount) => backend.call({ action: "apply", id: command.id, amount, rollback: false })));
      assert.ok(replies.every((reply) => reply.kind === "committed"));
      response = { receipts: replies.map((reply) => reply.value) };
    } else {
      response = await backend.call(command);
    }
    const elapsed = performance.now() - operationBegan;
    if (command.action === "restart") phases.restart_ms += elapsed;
    else phases.delegated_calls_ms += elapsed;
    phases.calls += 1;
    const output = JSON.stringify(response);
    assert.ok(output.length < 8192, "bounded fixture response");
    process.stdout.write(output + "\n");
  }
} finally {
  const cleanupBegan = performance.now();
  await backend.close();
  phases.cleanup_ms = performance.now() - cleanupBegan;
  process.stderr.write(JSON.stringify({ store_fixture_phases: phases }) + "\n");
  process.stdin.destroy();
}
