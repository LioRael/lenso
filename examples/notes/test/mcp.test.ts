import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { startApp } from "lenso";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { createNotesFiles, migrateFiles } from "../src/files";

test("actual Notes MCP entry shares sessions, validation and object authorization with CLI/Web", async () => {
  const directory = await mkdtemp(join(await realpath(tmpdir()), "notes-mcp-"));
  const filename = join(directory, "notes.sqlite");
  const root = join(directory, "files");
  const principals = [
    { subjectId: "alice", key: "04".repeat(32) },
    { subjectId: "bob", key: "05".repeat(32) },
  ];
  const clients: Client[] = [];
  try {
    await migrateFiles(filename);
    const definition = createNotesFiles({ filename, root, principals });
    const app = await startApp({ plugins: definition.plugins });
    let alice: string;
    let bob: string;
    try {
      const auth = app.get(definition.authentication);
      alice = (await auth.issue(principals[0]!.key)).credential;
      bob = (await auth.issue(principals[1]!.key)).credential;
    } finally {
      await app.stop();
    }
    async function connect(credential: string) {
      const transport = new StdioClientTransport({
        command: process.execPath,
        args: [fileURLToPath(new URL("../src/mcp.ts", import.meta.url))],
        cwd: directory,
        env: {
          DATABASE_URL: "",
          SQLITE_PATH: filename,
          STORAGE_ROOT: root,
          NOTES_LOGIN_KEYS: JSON.stringify(principals),
          NOTES_SESSION: credential,
        },
        stderr: "pipe",
      });
      const client = new Client({ name: "notes-test", version: "1.0.0" });
      clients.push(client);
      await client.connect(transport);
      return client;
    }
    const owner = await connect(alice);
    const other = await connect(bob);
    const { tools } = await owner.listTools();
    expect(tools.map((tool) => tool.title)).toEqual([
      "notes-operations.create",
      "notes-operations.list",
      "notes-operations.read",
      "notes-operations.update",
      "notes-operations.remove",
    ]);
    const create = tools.find((tool) => tool.title === "notes-operations.create")!;
    const read = tools.find((tool) => tool.title === "notes-operations.read")!;
    const decode = (result: unknown) => {
      const parsed = CallToolResultSchema.parse(result);
      const content = parsed.content[0];
      if (content?.type !== "text") throw new Error("Expected text tool content.");
      return JSON.parse(content.text);
    };
    const created = await owner.callTool({ name: create.name, arguments: { title: "MCP note" } });
    expect(created.isError).not.toBe(true);
    const note = decode(created);
    expect(note).toMatchObject({ title: "MCP note", ownerId: "alice" });
    const invalid = await owner.callTool({ name: create.name, arguments: { title: 42 } });
    expect(invalid.isError).toBe(true);
    expect(decode(invalid)).toMatchObject({
      code: "invalid-input",
      details: { paths: [["title"]] },
    });
    const forged = await owner.callTool({
      name: create.name,
      arguments: { title: "Forbidden fields", actor: { subjectId: "bob" }, tenantId: "other" },
    });
    expect(forged.isError).toBe(true);
    expect(decode(forged).code).toBe("invalid-input");
    const denied = await other.callTool({ name: read.name, arguments: { id: note.id } });
    expect(denied.isError).toBe(true);
    expect(decode(denied)).toMatchObject({ code: "FORBIDDEN", message: "Access denied" });
    const allowed = await owner.callTool({ name: read.name, arguments: { id: note.id } });
    expect(decode(allowed).title).toBe("MCP note");
  } finally {
    await Promise.all(clients.map((client) => client.close()));
    await rm(directory, { recursive: true, force: true });
  }
});
