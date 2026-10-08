import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { definePlugin, startApp } from "@lenso/core";
import {
  createAgentTools,
  createManageAdapter,
  describeManage,
  selectManageOperations,
} from "@lenso/manage";
import { createNotesFiles, migrateFiles } from "../src/files";
import { notesAudiences } from "../src/notes";
import { createNotesOperations } from "../src/operations";
import application, { manage, operations, mcpOperations } from "../lenso.config";

test("real Notes config selects the same original declarations for CLI and MCP", () => {
  expect(mcpOperations.map((operation) => operation.method)).toEqual(["list", "read", "remove"]);
  expect(operations.map((operation) => operation.method)).toEqual(
    expect.arrayContaining(["create", "list", "read", "update", "remove"]),
  );
  for (const operation of mcpOperations) {
    expect(operations).toContain(operation);
    expect(manage[0]!.operations).toContain(operation);
    expect(application.plugins).toContain(operation.plugin);
  }
});

test("Notes management borrows the real running app and binds current request evidence", async () => {
  const directory = await mkdtemp(join(await realpath(tmpdir()), "notes-manage-"));
  const filename = join(directory, "notes.sqlite");
  const principals = [
    { subjectId: "alice", key: "06".repeat(32) },
    { subjectId: "bob", key: "07".repeat(32) },
  ];
  let cleanups = 0;
  let bindings = 0;
  let evidence: string | null = null;
  const definition = createNotesFiles({
    filename,
    root: join(directory, "files"),
    principals,
  });
  // No launch credential fallback: every entry must supply its trusted context.
  const notes = createNotesOperations({
    notes: definition.notes,
    authentication: definition.authentication,
  });
  const probe = definePlugin({
    id: "manage-lifetime-probe",
    setup(context) {
      context.onCleanup(() => {
        cleanups++;
      });
      return {};
    },
  });
  const plugins = [...definition.plugins, notes.plugin, probe];
  const cli = selectManageOperations(notes.manage, ["list", "read", "remove"]);
  const mcp = selectManageOperations(notes.manage, ["read"]);
  const agent = selectManageOperations(notes.manage, ["read", "remove"]);
  const originalRead = notes.operations.find((operation) => operation.method === "read")!;
  expect(cli.find((operation) => operation.method === "read")).toBe(originalRead);
  expect(mcp[0]!).toBe(originalRead);
  expect(agent[0]!).toBe(originalRead);
  expect(() => selectManageOperations(notes.manage, ["create"])).toThrow();
  const description = describeManage(notes.manage);
  expect(description.schemaVersion).toBe(1);
  expect(description.operations.map((operation) => operation.method)).toEqual([
    "list",
    "read",
    "remove",
  ]);
  expect(JSON.parse(JSON.stringify(description))).toEqual(description);
  let running: Awaited<ReturnType<typeof startApp>> | undefined;
  try {
    await migrateFiles(filename);
    running = await startApp({ plugins });
    const auth = running.get(definition.authentication);
    const alice = (await auth.issue(principals[0]!.key)).credential;
    const bob = (await auth.issue(principals[1]!.key)).credential;
    const business = running.get(definition.notes);
    const row = await business.create(await auth.for(notesAudiences.create).required(alice), {
      title: "Shared running Notes",
    });
    const adapter = createManageAdapter({
      running,
      plugins,
      operations: agent,
      binding: () => {
        bindings++;
        return { context: { evidence } };
      },
      canList: (operation) => operation.method === "read",
    });
    const tools = await createAgentTools(adapter);
    expect(tools).toHaveLength(1);
    expect((await adapter.catalog()).map((operation) => operation.method)).toEqual(["read"]);
    const read = tools[0]!;
    for (const input of [
      { id: 42 },
      { id: row.id, actor: { subjectId: "alice" } },
      { id: row.id, confirmed: true },
      { id: row.id, evidence: alice },
    ]) {
      await expect(read.invoke(input)).rejects.toMatchObject({
        diagnostic: { code: "invalid-input" },
      });
    }
    expect(bindings).toBe(0);
    await expect(adapter.invoke(notes.plugin.id, "list", {})).rejects.toMatchObject({
      diagnostic: { code: "unknown-operation" },
    });
    evidence = null;
    await expect(read.invoke({ id: row.id })).rejects.toMatchObject({
      diagnostic: { code: "UNAUTHORIZED" },
    });
    evidence = bob;
    await expect(read.invoke({ id: row.id })).rejects.toMatchObject({
      diagnostic: { code: "FORBIDDEN" },
    });
    evidence = alice;
    expect(await read.invoke({ id: row.id })).toMatchObject({ title: row.title, ownerId: "alice" });
    await auth.revoke(alice);
    await expect(read.invoke({ id: row.id })).rejects.toMatchObject({
      diagnostic: { code: "UNAUTHORIZED" },
    });
    evidence = bob;
    expect(await read.invoke({ id: crypto.randomUUID() })).toBeNull();
    expect(cleanups).toBe(0);
    // Agent calls did not stop the app or consume the Web/service owner's resources.
    expect(await business.list(await auth.for(notesAudiences.list).required(bob))).toEqual([]);
  } finally {
    await running?.stop();
    await rm(directory, { recursive: true, force: true });
  }
  expect(cleanups).toBe(1);
});
