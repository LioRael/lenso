import { expect, test } from "bun:test";
import { resolve } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { testOnlyIdentity } from "../src/host";

test("dedicated stdio borrows one application across writes and reads", async () => {
  const client = new Client({ name: "test-only-stdio-client", version: "1.0.0" });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [resolve(import.meta.dir, "../src/stdio.ts")],
    stderr: "pipe",
  });
  try {
    await client.connect(transport);
    const { tools } = await client.listTools();
    const read = tools.find((tool) => tool.title === "host-notes.read");
    const write = tools.find((tool) => tool.title === "host-notes.write");
    expect(tools).toHaveLength(2);
    if (!read || !write) throw new Error("Expected the explicit Manage selection.");
    const input = { tenant: testOnlyIdentity.tenant, id: "stdio-note" };
    for (const text of ["first", "second"]) {
      expect(
        (await client.callTool({ name: write.name, arguments: { ...input, text } })).isError,
      ).toBeUndefined();
      expect((await client.callTool({ name: read.name, arguments: input })).content).toEqual([
        { type: "text", text: JSON.stringify({ id: input.id, text }) },
      ]);
    }
    expect(
      (
        await client.callTool({
          name: write.name,
          arguments: { ...input, text: "denied", tenant: "test-only-other-tenant" },
        })
      ).isError,
    ).toBe(true);
  } finally {
    await client.close();
  }
}, 10_000);
