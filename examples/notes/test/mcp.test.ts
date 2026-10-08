import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { startApp } from "@lenso/core";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";
import { createNotesFiles, migrateFiles } from "../src/files";
import { notesAudiences } from "../src/notes";

test("actual Notes MCP entry shares sessions, validation and object authorization with CLI/Web", async () => {
  const directory = await mkdtemp(join(await realpath(tmpdir()), "notes-mcp-"));
  const filename = join(directory, "notes.sqlite");
  const root = join(directory, "files");
  const principals = [
    { subjectId: "alice", key: "04".repeat(32) },
    { subjectId: "bob", key: "05".repeat(32) },
  ];
  const clients: Client[] = [];
  let app: Awaited<ReturnType<typeof startApp>> | undefined;
  try {
    await migrateFiles(filename);
    const definition = createNotesFiles({ filename, root, principals });
    app = await startApp({ plugins: definition.plugins });
    const auth = app.get(definition.authentication);
    const alice = (await auth.issue(principals[0]!.key)).credential;
    const bob = (await auth.issue(principals[1]!.key)).credential;
    const note = await app
      .get(definition.notes)
      .create(await auth.for(notesAudiences.create).required(alice), { title: "MCP note" });
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
          NOTES_SESSION: bob,
          NOTES_MCP_SESSION: credential,
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
      "notes-operations.list",
      "notes-operations.read",
      "notes-operations.remove",
    ]);
    const read = tools.find((tool) => tool.title === "notes-operations.read")!;
    function decodeToolResult(result: unknown) {
      const parsed = CallToolResultSchema.parse(result);
      const content = parsed.content[0];
      if (content?.type !== "text") throw new Error("Expected text tool content.");
      return JSON.parse(content.text);
    }
    const invalid = await owner.callTool({ name: read.name, arguments: { id: 42 } });
    expect(invalid.isError).toBe(true);
    expect(decodeToolResult(invalid)).toMatchObject({
      code: "invalid-input",
      details: { paths: [["id"]] },
    });
    const forged = await owner.callTool({
      name: read.name,
      arguments: { id: note.id, actor: { subjectId: "bob" }, confirmed: true },
    });
    expect(forged.isError).toBe(true);
    expect(decodeToolResult(forged).code).toBe("invalid-input");
    const denied = await other.callTool({ name: read.name, arguments: { id: note.id } });
    expect(denied.isError).toBe(true);
    expect(decodeToolResult(denied)).toMatchObject({ code: "FORBIDDEN", message: "Access denied" });
    const allowed = await owner.callTool({ name: read.name, arguments: { id: note.id } });
    expect(decodeToolResult(allowed).title).toBe("MCP note");
    await auth.revoke(alice);
    const revoked = await owner.callTool({ name: read.name, arguments: { id: note.id } });
    expect(revoked.isError).toBe(true);
    expect(decodeToolResult(revoked).code).toBe("UNAUTHORIZED");
  } finally {
    await Promise.all(clients.map((client) => client.close()));
    await app?.stop();
    await rm(directory, { recursive: true, force: true });
  }
});
