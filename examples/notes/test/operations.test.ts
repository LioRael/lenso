import { expect, test } from "bun:test";
import { mkdtemp, rm, access, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";
import { definePlugin, startApp } from "@lenso/core";
import { defineOperation, invoke } from "@lenso/cli";
import { z } from "zod";
import { createSqliteFileQueries } from "@lenso/storage/sqlite";
import {
  createNotesFiles,
  migrateFiles,
  notesFileTenant,
  type NotesFileAccess,
} from "../src/files";
import { notesAudiences } from "../src/notes";
import { createNotesOperations, createNotesFileOperations } from "../src/operations";

test("Notes registry validates before setup and authenticates all business/file calls", async () => {
  const directory = await mkdtemp(join(await realpath(tmpdir()), "notes-operations-"));
  const filename = join(directory, "notes.sqlite");
  const key = "01".repeat(32);
  const otherKey = "02".repeat(32);
  let credential: string | null = null;
  const definition = createNotesFiles({
    filename,
    root: join(directory, "files"),
    principals: [
      { subjectId: "alice", key },
      { subjectId: "bob", key: otherKey },
    ],
  });
  const notes = createNotesOperations({
    notes: definition.notes,
    authentication: definition.authentication,
    credential: () => credential,
  });
  const files = createNotesFileOperations({
    files: definition.files,
    authentication: definition.authentication,
    credential: () => credential,
  });
  defineOperation({
    plugin: notes.plugin,
    // @ts-expect-error A companion factory retains the service method constraints.
    method: "issue",
    input: z.object({}),
    description: "Not a Notes operation",
  });
  defineOperation({
    plugin: notes.plugin,
    // @ts-expect-error The schema output must satisfy the create method's input.
    method: "create",
    input: z.object({ title: z.number() }),
    description: "Not a compatible input",
  });
  let setups = 0;
  let cleanups = 0;
  const probe = definePlugin({
    id: "setup-probe",
    setup(context) {
      setups++;
      context.onCleanup(() => {
        cleanups++;
      });
      return {};
    },
  });
  const appDefinition = {
    plugins: [...definition.plugins, notes.plugin, files.plugin, probe],
    operations: [...notes.operations, ...files.operations],
  };
  expect(notes.operations.every((operation) => operation.plugin === notes.plugin)).toBe(true);
  expect(files.operations.every((operation) => operation.plugin === files.plugin)).toBe(true);
  const replacement = definePlugin({
    id: notes.plugin.id,
    setup: () => ({}),
  });
  await expect(
    invoke(
      { plugins: [...definition.plugins, replacement], operations: notes.operations },
      notes.plugin.id,
      "create",
      { title: "not allowed" },
    ),
  ).rejects.toMatchObject({ diagnostic: { code: "invalid-operations" } });
  await expect(
    invoke(
      { plugins: [...definition.plugins, notes.plugin], operations: [notes.operations[0]!] },
      notes.plugin.id,
      "list",
      {},
    ),
  ).rejects.toMatchObject({ diagnostic: { code: "unknown-operation" } });
  const run = (method: string, input: unknown) =>
    invoke(appDefinition, notes.plugin.id, method, input);
  const runFile = (method: string, input: unknown) =>
    invoke(appDefinition, files.plugin.id, method, input);
  try {
    for (const input of [
      { title: " " },
      { title: 2 },
      { title: "x".repeat(201) },
      { title: "ok", body: "x".repeat(20_001) },
      { title: "ok", actor: { subjectId: "alice" } },
      { title: "ok", ownerId: "alice" },
    ])
      await expect(run("create", input)).rejects.toMatchObject({
        diagnostic: { code: "invalid-input" },
      });
    await expect(run("read", { id: "not-a-uuid" })).rejects.toMatchObject({
      diagnostic: { code: "invalid-input" },
    });
    await expect(run("list", { actor: {} })).rejects.toMatchObject({
      diagnostic: { code: "invalid-input" },
    });
    await expect(runFile("metadata", { fileId: "bad" })).rejects.toMatchObject({
      diagnostic: { code: "invalid-input" },
    });
    await expect(run("issue", {})).rejects.toMatchObject({
      diagnostic: { code: "unknown-operation" },
    });
    expect(setups).toBe(0);
    await expect(access(filename)).rejects.toBeDefined();
    await migrateFiles(filename);

    const running = await startApp({ plugins: definition.plugins });
    let otherCredential: string;
    let fileId: string;
    let foreignTenantId: string;
    try {
      const auth = running.get(definition.authentication);
      credential = (await auth.issue(key)).credential;
      otherCredential = (await auth.issue(otherKey)).credential;
      const service = running.get(definition.files);
      const actor = await auth.for(notesAudiences.create).required(credential);
      const record = await service.upload(actor, {
        ownerId: "alice",
        tenantId: notesFileTenant,
        storageId: definition.privateFiles.id,
        filename: "note.txt",
        contentType: "text/plain",
        maxBytes: 100,
        body: new Blob(["private"]).stream(),
      });
      fileId = record.fileId;
      foreignTenantId = crypto.randomUUID();
      await createSqliteFileQueries(running.get(definition.database)).insert({
        ...record,
        fileId: foreignTenantId,
        objectKey: `files/${crypto.randomUUID()}`,
        tenantId: "another-tenant",
      });
      const forged = {
        realmId: "notes",
        subjectId: "alice",
        audience: "notes:file-metadata",
        kind: "user",
      } as NotesFileAccess;
      await expect(service.metadata(forged, fileId)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      await expect(
        service.delete({ ...forged, audience: "notes:file-delete" } as NotesFileAccess, fileId),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    } finally {
      await running.stop();
    }
    const row = (await run("create", { title: "  Owned  ", body: "text" })) as {
      id: string;
      ownerId: string;
    };
    expect(row.ownerId).toBe("alice");
    expect(await run("read", { id: row.id })).toMatchObject({ title: "Owned" });
    expect(await run("update", { id: row.id, title: "Changed" })).toMatchObject({
      title: "Changed",
      body: "",
    });
    expect(await runFile("metadata", { fileId: fileId! })).toMatchObject({
      filename: "note.txt",
      size: 7,
    });
    for (const method of ["metadata", "delete"])
      await expect(runFile(method, { fileId: foreignTenantId! })).rejects.toMatchObject({
        diagnostic: { code: "FORBIDDEN" },
      });
    await expect(
      runFile("metadata", { fileId: fileId!, actor: { subjectId: "alice" } }),
    ).rejects.toMatchObject({ diagnostic: { code: "invalid-input" } });
    credential = otherCredential!;
    expect(await run("list", {})).toEqual([]);
    for (const [method, input] of [
      ["read", { id: row.id }],
      ["update", { id: row.id, title: "stolen" }],
      ["remove", { id: row.id }],
    ] as const)
      await expect(run(method, input)).rejects.toMatchObject({ diagnostic: { code: "FORBIDDEN" } });
    for (const method of ["metadata", "delete"])
      await expect(runFile(method, { fileId: fileId! })).rejects.toMatchObject({
        diagnostic: { code: "FORBIDDEN" },
      });
    credential = null;
    await expect(run("list", {})).rejects.toMatchObject({ diagnostic: { code: "UNAUTHORIZED" } });
    await expect(runFile("metadata", { fileId: fileId! })).rejects.toMatchObject({
      diagnostic: { code: "UNAUTHORIZED" },
    });
    const restarted = await startApp({ plugins: definition.plugins });
    try {
      credential = (await restarted.get(definition.authentication).issue(key)).credential;
    } finally {
      await restarted.stop();
    }
    expect(await runFile("delete", { fileId: fileId! })).toMatchObject({ state: "deleted" });
    expect(await run("remove", { id: row.id })).toEqual({ removed: true });
    expect(setups).toBe(cleanups);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("real inspect/call CLI uses the Notes config without actor input or implicit migration", async () => {
  const directory = await mkdtemp(join(await realpath(tmpdir()), "notes-cli-"));
  const filename = join(directory, "notes.sqlite");
  const cli = resolve(import.meta.dir, "../../../packages/cli/src/bin.ts");
  const root = resolve(import.meta.dir, "..");
  const key = "03".repeat(32);
  const env = {
    ...process.env,
    DATABASE_URL: "",
    SQLITE_PATH: relative(root, filename),
    STORAGE_ROOT: relative(root, join(directory, "files")),
    NOTES_LOGIN_KEYS: JSON.stringify([{ subjectId: "alice", key }]),
    NOTES_SESSION: "",
    LENSO_PORT: "not-a-port",
  };
  const spawn = async (args: string[], input = "", environment = env) => {
    const child = Bun.spawn([process.execPath, cli, ...args, "--root", root, "--json"], {
      env: environment,
      cwd: directory,
      stdin: new Blob([input]),
      stdout: "pipe",
      stderr: "pipe",
    });
    const [exit, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    return { exit, output: JSON.parse(stdout), stderr };
  };
  try {
    const inspection = await spawn(["inspect", "notes-operations", "create"], "", {
      ...env,
      NOTES_LOGIN_KEYS: "",
    });
    expect(inspection.exit).toBe(0);
    expect(inspection.output.data.operations[0].inputSchema.additionalProperties).toBe(false);
    expect(inspection.output.data.plugins.map((plugin: { id: string }) => plugin.id)).not.toContain(
      "notes-listener",
    );
    await expect(access(filename)).rejects.toBeDefined();
    const invalid = await spawn(
      ["call", "notes-operations", "create", "--stdin"],
      '{"title":"x","actor":{}}',
    );
    expect(invalid.exit).toBe(2);
    expect(invalid.output.error.code).toBe("invalid-input");
    await expect(access(filename)).rejects.toBeDefined();
    await migrateFiles(filename);
    const login = Bun.spawn([process.execPath, resolve(root, "dist/cli.js"), "login"], {
      env: { ...env, NOTES_LOGIN_KEY: key },
      cwd: directory,
      stdout: "pipe",
      stderr: "pipe",
    });
    const [exit, output] = await Promise.all([login.exited, new Response(login.stdout).json()]);
    expect(exit).toBe(0);
    const created = await spawn(
      ["call", "notes-operations", "create", "--stdin"],
      '{"title":"From CLI"}',
      { ...env, NOTES_SESSION: (output as { credential: string }).credential },
    );
    expect(created.exit).toBe(0);
    expect(created.output.data).toMatchObject({ title: "From CLI", ownerId: "alice" });
    const denied = await spawn(["call", "notes-operations", "list", "--stdin"], "{}");
    expect(denied.output.error.code).toBe("UNAUTHORIZED");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
