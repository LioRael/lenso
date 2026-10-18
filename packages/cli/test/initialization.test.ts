import { expect, test } from "bun:test";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { Database } from "bun:sqlite";
import { createNativeFirstRunStore } from "../examples/first-run/src/native";

const app = resolve(import.meta.dir, "../examples/first-run");
const cwd = resolve(import.meta.dir, "..");
const entry = `import { runAdapter } from './src/adapter.ts'; await runAdapter(process.argv.slice(1));`;
const input = {
  username: "fixture-owner",
  password: "LOCAL-SYNTHETIC-password-123",
  providerKey: "LOCAL-SYNTHETIC-provider-key",
};
async function run(home: string, args: string[], body?: unknown) {
  const child = Bun.spawn([process.execPath, "--eval", entry, ...args, "--root", app, "--json"], {
    cwd,
    env: { ...process.env, BUN_OPTIONS: "", NODE_PATH: "", LENSO_FIRST_RUN_HOME: home },
    stdin: body === undefined ? "ignore" : new Blob([JSON.stringify(body)]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, exit] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  expect(stdout + stderr).not.toContain(input.password);
  expect(stdout + stderr).not.toContain(input.providerKey);
  return { exit, value: JSON.parse(stdout), stderr };
}

test("real CLI first-run persists one owner/config under concurrent claim, closes bootstrap, and survives restart", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-first-run-"));
  const home = join(directory, "state");
  try {
    expect((await run(home, ["setup", "status"])).value.data.state).toBe("uninitialized");
    await expect(stat(home)).rejects.toMatchObject({ code: "ENOENT" }); // status is read-only
    const before = await run(home, ["call", "greeting", "greet", "--stdin"], {});
    expect(before.value.error.code).toBe("initialization-required");
    const request = { requestId: "fixture-attempt-001", input };
    const results = await Promise.all([
      run(home, ["setup", "apply", "--stdin"], request),
      run(home, ["setup", "apply", "--stdin"], request),
    ]);
    for (const result of results) {
      expect(result.exit).toBe(0);
      expect(result.value.data.state).toBe("ready");
    }
    expect(results[0]!.value.data.revision).toBe(results[1]!.value.data.revision);
    const store = createNativeFirstRunStore(home);
    const config = await store.configuration();
    expect(await store.authenticate(input.username, input.password)).toBe(true);
    expect(await store.authenticate(input.username, "incorrect-password")).toBe(false);
    const db = new Database(store.filename, { readonly: true });
    try {
      expect(
        db.query<{ total: number }, []>("SELECT count(*) AS total FROM first_run_owner").get()!
          .total,
      ).toBe(1);
      const owner = db
        .query<{ password_hash: string; role: string }, []>(
          "SELECT password_hash,role FROM first_run_owner",
        )
        .get()!;
      expect(owner.password_hash).not.toBe(input.password);
      expect(owner.role).toBe("owner");
    } finally {
      db.close();
    }
    expect((await stat(home)).mode & 0o077).toBe(0);
    expect((await stat(store.filename)).mode & 0o077).toBe(0);
    const conflict = await run(home, ["setup", "apply", "--stdin"], {
      ...request,
      input: { ...input, username: "replacement-owner" },
    });
    expect(conflict.value.error.code).toBe("initialization-conflict");
    const passwordConflict = await run(home, ["setup", "apply", "--stdin"], {
      ...request,
      input: { ...input, password: "DIFFERENT-SYNTHETIC-password" },
    });
    expect(passwordConflict.value.error.code).toBe("initialization-conflict");
    const closed = await run(home, ["setup", "apply", "--stdin"], {
      requestId: "another-attempt-002",
      input,
    });
    expect(closed.value.error.code).toBe("initialization-closed");
    expect((await run(home, ["setup", "status"])).value.data.state).toBe("ready");
    expect((await run(home, ["call", "greeting", "greet", "--stdin"], {})).value.data).toEqual({
      message: "Hello, fixture-owner!",
    });
    expect(await store.configuration()).toEqual(config); // no key regeneration on retry/restart
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}, 15000);

test("a durable pending setup resumes in a new process; losing its marker never reopens an existing owner", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-existing-owner-"));
  try {
    const store = createNativeFirstRunStore(directory);
    // Create through the same approved transaction, then model a missing/corrupt marker.
    await store.commit({ requestId: "initial-attempt-001", input });
    const pending = await run(directory, ["setup", "status"]);
    expect(pending.value.data.state).toBe("awaiting_activation");
    const values = await store.configuration();
    expect((await run(directory, ["setup", "activate"])).value.data.state).toBe("ready");
    expect(await store.configuration()).toEqual(values);
    const db = new Database(store.filename);
    try {
      db.exec("DROP TABLE first_run_state");
    } finally {
      db.close();
    }
    const result = await run(directory, ["setup", "apply", "--stdin"], {
      requestId: "replacement-attempt",
      input,
    });
    expect(result.value.error.code).toBe("initialization-closed");
    expect(await store.authenticate(input.username, input.password)).toBe(true);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("a failed configuration insert rolls back owner/marker and the same request recovers after repair", async () => {
  const directory = await mkdtemp(join(tmpdir(), "lenso-setup-rollback-"));
  try {
    const store = createNativeFirstRunStore(directory);
    await store.commit({ requestId: "fixture-schema-only", input });
    const db = new Database(store.filename);
    try {
      db.exec(
        "DELETE FROM first_run_owner; DELETE FROM first_run_config; DELETE FROM first_run_state;",
      );
      db.exec(
        "CREATE TRIGGER fail_configuration BEFORE INSERT ON first_run_config BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;",
      );
      const request = { requestId: "rollback-attempt-001", input };
      const failed = await run(directory, ["setup", "apply", "--stdin"], request);
      expect(failed.value.error.code).toBe("initialization-outcome-unknown");
      expect(
        db.query<{ total: number }, []>("SELECT count(*) AS total FROM first_run_owner").get()!
          .total,
      ).toBe(0);
      expect(
        db.query<{ total: number }, []>("SELECT count(*) AS total FROM first_run_state").get()!
          .total,
      ).toBe(0);
      expect((await run(directory, ["setup", "status"])).value.data.state).toBe("uninitialized");
      db.exec("DROP TRIGGER fail_configuration");
      expect((await run(directory, ["setup", "apply", "--stdin"], request)).value.data.state).toBe(
        "ready",
      );
    } finally {
      db.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
