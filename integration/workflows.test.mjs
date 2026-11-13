import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";

const run = promisify(execFile);
const root = resolve(import.meta.dirname, "..");
const runtimes = {
  typescript: [process.execPath, [join(root, "typescript/examples/workflows/dist/main.js")]],
  rust: [join(root, "rust/target/debug/lenso-workflows"), []],
};

for (const sequence of [["typescript"], ["rust"], ["typescript", "rust"], ["rust", "typescript"]]) {
  test(`${sequence.join(" -> ")}: domain transaction, invitation mail, registration notification and idempotent delivery`, async () => {
    const directory = await mkdtemp(join(tmpdir(), "lenso-workflows-"));
    const filename = join(directory, "app.sqlite");
    try {
      for (const [index, runtime] of [...sequence, sequence.at(-1)].entries()) {
        const [command, args] = runtimes[runtime];
        const { stdout } = await run(command, args, { cwd: root, env: { ...process.env, LENSO_DB: filename }, timeout: 15_000 });
        const output = JSON.parse(stdout);
        assert.equal(output.mailbox.length, 2);
        assert.deepEqual(output.mailbox.map((mail) => mail.recipient).sort(), ["alice@example.test", "invitee@example.test"]);
        assert.equal(output.localNotifications, 1);
        assert.equal(output.calls, 3);
        assert.equal(output.completed, index === 0 ? 2 : 0);
        assert.equal(output.manifest.pages[0].list, "console.mailbox");
      }
      const db = new DatabaseSync(filename);
      try {
        assert.equal(db.prepare("SELECT count(*) AS n FROM users").get().n, 1);
        assert.equal(db.prepare("SELECT count(*) AS n FROM invitations").get().n, 1);
        assert.equal(db.prepare("SELECT count(*) AS n FROM lenso_jobs WHERE state = 'succeeded'").get().n, 2);
        assert.equal(db.prepare("SELECT count(*) AS n FROM mailbox").get().n, 2);
      } finally { db.close(); }
    } finally { await rm(directory, { recursive: true, force: true }); }
  });
}
