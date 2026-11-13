import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { ConsoleClient } from "@lenso/console";

const root = resolve(import.meta.dirname, "..");
const runtimes = {
  typescript: [process.execPath, [join(root, "typescript/examples/private-files/dist/server.js")]],
  rust: [join(root, "rust/target/debug/lenso-private-files"), []],
};

async function start(runtime, filename, extraEnv = {}) {
  const [command, args] = runtimes[runtime];
  const child = spawn(command, args, {
    cwd: root, env: { ...process.env, PORT: "0", LENSO_DB: filename, LENSO_CONSOLE_SUBJECT: "", ...extraEnv }, stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  let errors = "";
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
  return {
    url,
    async stop() {
      if (child.exitCode !== null) return;
      const exited = new Promise((resolve) => child.once("exit", resolve));
      child.kill("SIGTERM");
      const timer = setTimeout(() => child.kill("SIGKILL"), 5000);
      await exited;
      clearTimeout(timer);
    },
  };
}

async function request(server, path, { token, body, method = body === undefined ? "GET" : "POST" } = {}) {
  const response = await fetch(`${server.url}${path}`, {
    method,
    headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}) },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  return { status: response.status, body: await response.json() };
}

for (const runtime of Object.keys(runtimes)) {
  test(`${runtime}: real identity, file isolation, persistence and call records`, async () => {
    const directory = await mkdtemp(join(tmpdir(), `lenso-${runtime}-`));
    const filename = join(directory, "app.sqlite");
    let server;
    try {
      server = await start(runtime, filename);
      const password = "long-enough-password";
      const alice = await request(server, "/auth/register", { body: { name: "alice", password } });
      assert.equal(alice.status, 200);
      assert.equal((await request(server, "/auth/register", { body: { name: "bob", password } })).status, 200);
      assert.equal((await request(server, "/auth/register", { body: { name: "alice", password } })).status, 409);
      assert.equal((await request(server, "/auth/login", { body: { name: "alice", password: "incorrect-password" } })).status, 401);
      const aliceLogin = await request(server, "/auth/login", { body: { name: "alice", password } });
      const bobLogin = await request(server, "/auth/login", { body: { name: "bob", password } });
      assert.equal(aliceLogin.status, 200);
      assert.equal(bobLogin.status, 200);
      const token = aliceLogin.body.token;
      assert.equal((await request(server, "/files")).status, 401);
      assert.equal((await request(server, "/files", { token: "forged-token" })).status, 401);
      const bytes = Buffer.from([0, 1, 255, 65]);
      const upload = await request(server, "/files", {
        token, body: { name: "private.bin", content: bytes.toString("base64"), owner: "bob" },
      });
      assert.equal(upload.status, 200);
      assert.equal(upload.body.owner, alice.body.id, "request body must not choose the file owner");
      const path = `/files/${upload.body.id}`;
      const own = await request(server, path, { token });
      assert.equal(own.status, 200);
      assert.deepEqual(Buffer.from(own.body.content, "base64"), bytes);
      const other = await request(server, path, { token: bobLogin.body.token });
      assert.equal(other.status, 403);
      assert.equal(other.body.error.code, "forbidden");
      assert.equal((await request(server, "/files", { token: bobLogin.body.token })).body.length, 0);
      assert.equal((await request(server, "/files", { token })).body.length, 1);
      assert.equal((await request(server, "/files", { token, body: { name: "bad", content: "!" } })).status, 400);
      assert.equal((await request(server, "/auth/login", { body: { name: 42, password } })).status, 400);
      assert.equal((await request(server, "/console/manifest")).status, 401);
      assert.equal((await request(server, "/console/manifest", { token })).status, 403);
      await server.stop();
      server = await start(runtime, filename, { LENSO_CONSOLE_SUBJECT: alice.body.id });
      assert.deepEqual((await request(server, path, { token })).body, own.body, "session and file survive process restart");
      const consoleClient = new ConsoleClient(`${server.url}/console`, () => token);
      const manifest = await consoleClient.manifest();
      assert.deepEqual(manifest.pages.map((page) => page.id), ["users", "files", "requests"]);
      const users = await consoleClient.call("console.users");
      assert.equal(users.length, 2);
      assert.deepEqual(Object.keys(users[0]).sort(), ["id", "name"]);
      assert.equal((await consoleClient.call("console.files")).length, 1);
      assert.ok((await consoleClient.call("console.requests")).some((record) => record.outcome === "forbidden"));
      const forbiddenClient = new ConsoleClient(`${server.url}/console/`, () => bobLogin.body.token);
      await assert.rejects(forbiddenClient.call("console.users"), { code: "forbidden" });
      const db = new DatabaseSync(filename);
      try {
        assert.equal(db.prepare("SELECT count(*) AS n FROM users WHERE password_hash = ?").get(password).n, 0);
        assert.equal(db.prepare("SELECT count(*) AS n FROM sessions WHERE token_hash = ?").get(token).n, 0);
        assert.ok(db.prepare("SELECT count(*) AS n FROM calls WHERE action = 'files.read' AND outcome = 'forbidden'").get().n > 0);
        assert.equal(db.prepare("SELECT count(*) AS n FROM calls WHERE action = 'files.upload' AND outcome = 'invalid_input'").get().n, 1);
        db.prepare("UPDATE sessions SET expires_ms = 0 WHERE subject = ?").run(alice.body.id);
      } finally { db.close(); }
      assert.equal((await request(server, path, { token })).status, 401);
    } finally {
      await server?.stop();
      await rm(directory, { recursive: true, force: true });
    }
  });
}

test("TS-created data, credentials and sessions can be opened by the independent Rust process", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-interoperability-"));
  const filename = join(directory, "app.sqlite");
  let server;
  try {
    server = await start("typescript", filename);
    const credentials = { name: "alice", password: "interoperable-password" };
    assert.equal((await request(server, "/auth/register", { body: credentials })).status, 200);
    const token = (await request(server, "/auth/login", { body: credentials })).body.token;
    const file = (await request(server, "/files", { token, body: { name: "hello.txt", content: "aGVsbG8=" } })).body;
    await server.stop();
    server = await start("rust", filename);
    assert.equal((await request(server, "/auth/login", { body: credentials })).status, 200);
    assert.equal((await request(server, `/files/${file.id}`, { token })).body.content, "aGVsbG8=");
  } finally {
    await server?.stop();
    await rm(directory, { recursive: true, force: true });
  }
});
