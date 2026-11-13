import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { ConsoleClient } from "@lenso/console";

const root = resolve(import.meta.dirname, "../..");
export const identities = [
  { subject: "alice", team: "alpha", role: "admin", token: "alice-local-secret-token" },
  { subject: "bob", team: "alpha", role: "editor", token: "bob-local-secret-token" },
  { subject: "reader", team: "alpha", role: "viewer", token: "reader-local-secret-token" },
  { subject: "eve", team: "beta", role: "admin", token: "eve-local-secret-token" },
];
export async function start(runtime, filename) {
  const command = runtime === "typescript" ? process.execPath : join(root, "rust/target/debug/lenso-team-desk");
  const args = runtime === "typescript" ? [join(root, "typescript/examples/team-desk/dist/server.js")] : [];
  const child = spawn(command, args, { cwd: root, env: { ...process.env, PORT: "0", LENSO_DB: filename, LENSO_IDENTITIES: JSON.stringify(identities) }, stdio: ["ignore", "pipe", "pipe"] });
  let output = "", errors = "";
  child.stderr.on("data", (chunk) => { errors += chunk; });
  const url = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { child.kill(); reject(new Error(`Startup timeout: ${output}\n${errors}`)); }, 15_000);
    child.once("error", (error) => { clearTimeout(timer); reject(error); });
    child.once("exit", (code) => { clearTimeout(timer); reject(new Error(`Startup exit ${code}: ${errors}`)); });
    child.stdout.on("data", (chunk) => {
      output += chunk;
      const port = output.match(/port: (\d+)/)?.[1] ?? output.match(/127\.0\.0\.1:(\d+)/)?.[1];
      if (port) { clearTimeout(timer); resolve(`http://127.0.0.1:${port}`); }
    });
  });
  return { url, client(subject) { return new ConsoleClient(`${url}/console`, () => identities.find((value) => value.subject === subject)?.token ?? "forged-token"); },
    async drain(subject = "alice") {
      const response = await fetch(`${url}/worker/drain`, { method: "POST", headers: { authorization: `Bearer ${identities.find((v) => v.subject === subject)?.token}`, "content-type": "application/json" }, body: "null" });
      const value = await response.json();
      assert.equal(response.status, 200, JSON.stringify(value)); return value.completed;
    },
    async stop() {
      if (child.exitCode !== null) return;
      const exited = new Promise((resolve) => child.once("exit", resolve));
      child.kill("SIGTERM"); const timer = setTimeout(() => child.kill("SIGKILL"), 5000); await exited; clearTimeout(timer);
    },
  };
}
export async function fixture(run) {
  const directory = await mkdtemp(join(tmpdir(), "lenso-team-"));
  const filename = join(directory, "app.sqlite");
  const servers = [];
  try { await run({ filename, async start(runtime, database = filename) { const server = await start(runtime, database); servers.push(server); return server; } }); }
  finally { await Promise.all(servers.map((server) => server.stop())); await rm(directory, { recursive: true, force: true }); }
}
