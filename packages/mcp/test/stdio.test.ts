import { expect, test } from "bun:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";

const server = `${import.meta.dir}/fixtures/server.ts`;

async function connect() {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [server],
    stderr: "pipe",
  });
  let logs = "";
  transport.stderr?.on("data", (chunk) => {
    logs += chunk.toString();
  });
  const client = new Client({ name: "lenso-test", version: "1.0.0" });
  await client.connect(transport);
  return { client, transport, logs: () => logs };
}

async function waitForLog(session: { logs(): string }, text: string) {
  const deadline = Date.now() + 2000;
  while (!session.logs().includes(text)) {
    if (Date.now() >= deadline) throw new Error(`Missing fixture log: ${text}`);
    await Bun.sleep(5);
  }
}

test("official SDK stdio lists only allowed operations and calls shared validation/lifecycle", async () => {
  const session = await connect();
  try {
    const { tools } = await session.client.listTools();
    expect(tools).toHaveLength(6);
    expect(tools[0]!.title).toBe("fixture.echo");
    expect(tools[0]!.inputSchema.properties).toHaveProperty("value");
    expect(tools[0]!.annotations?.destructiveHint).toBe(true);
    const { _meta: metadata } = tools[0]!;
    expect(metadata?.["lenso/operation"]).toMatchObject({
      pluginId: "fixture",
      method: "echo",
      source: { file: `${import.meta.dir}/fixtures/lenso.config.ts` },
      schemaAvailability: "available",
    });
    expect(session.logs()).not.toContain("fixture setup");
    const ok = await session.client.callTool({
      name: tools[0]!.name,
      arguments: { value: "hello" },
    });
    expect(ok.isError).not.toBe(true);
    expect(JSON.stringify(ok.content)).toContain("hello");
    expect(JSON.stringify(ok.content)).toContain("[REDACTED]");
    expect(session.logs()).toContain("fixture cleanup");
    expect(session.logs()).not.toContain("log-secret");
    expect(session.logs()).not.toContain("result-secret");
    const before = session.logs().match(/fixture setup/g)?.length;
    const invalid = await session.client.callTool({
      name: tools[0]!.name,
      arguments: { value: "hello", root: "/", module: "evil", method: "hidden", shell: "sh" },
    });
    expect(JSON.stringify(invalid)).toContain("invalid-input");
    expect(session.logs().match(/fixture setup/g)?.length).toBe(before);
    await expect(
      session.client.callTool({
        name: "fixture.hidden",
        arguments: { value: "hello" },
      }),
    ).rejects.toThrow("Tool is not allowlisted.");
    expect(session.logs().match(/fixture setup/g)?.length).toBe(before);
  } finally {
    await session.client.close();
  }
});

test("auth, runtime, serialization and size failures are safe tool results", async () => {
  const session = await connect();
  try {
    for (const [name, code] of [
      ["operation_2", "authorization-denied"],
      ["operation_3", "invocation-failed"],
      ["operation_4", "serialization-failed"],
      ["operation_5", "output-too-large"],
    ]) {
      const result = await session.client.callTool({ name: name!, arguments: { value: "hello" } });
      expect(result.isError).toBe(true);
      expect(JSON.stringify(result)).toContain(code!);
      expect(JSON.stringify(result)).not.toContain("private-secret");
    }
    const tooLarge = await session.client.callTool({
      name: "operation_0",
      arguments: { value: "x".repeat(300) },
    });
    expect(JSON.stringify(tooLarge)).toContain("input-too-large");
    const largeDiagnostic = await session.client.callTool({
      name: "operation_2",
      arguments: { value: "large-diagnostic" },
    });
    expect(largeDiagnostic.isError).toBe(true);
    expect(JSON.stringify(largeDiagnostic)).toContain("authorization-denied");
    expect(JSON.stringify(largeDiagnostic)).toContain("truncated");
    const content = largeDiagnostic.content as Array<{ type: string; text: string }>;
    expect(Buffer.byteLength(content[0]!.text)).toBeLessThanOrEqual(512);
    expect(content[0]!.text).not.toContain("xxxx");
  } finally {
    await session.client.close();
  }
});

test("cancellation does not bypass cleanup; concurrent requests are rejected, not queued", async () => {
  const session = await connect();
  try {
    const controller = new AbortController();
    const pending = session.client.request(
      {
        method: "tools/call",
        params: { name: "operation_1", arguments: { value: "hello", delay: 200 } },
      },
      CallToolResultSchema,
      { signal: controller.signal },
    );
    const rejected = pending.catch(() => undefined);
    await waitForLog(session, "fixture setup");
    const busy = await session.client.callTool({
      name: "operation_0",
      arguments: { value: "hello" },
    });
    expect(JSON.stringify(busy)).toContain("adapter-busy");
    controller.abort();
    await rejected;
    await waitForLog(session, "fixture cleanup");
    expect(session.logs()).toContain("fixture completed");
    expect(session.logs()).toContain("fixture cleanup");
    const next = await session.client.callTool({
      name: "operation_0",
      arguments: { value: "after" },
    });
    expect(next.isError).not.toBe(true);
  } finally {
    await session.client.close();
  }
});

test("runtime-only schemas reject startup rather than fabricate a schema", async () => {
  const child = Bun.spawn([process.execPath, server], {
    env: { ...process.env, MCP_TEST_RUNTIME_ONLY: "1" },
    stdout: "pipe",
    stderr: "pipe",
  });
  expect(await child.exited).toBe(1);
  expect(await new Response(child.stdout).text()).toBe("");
  expect(await new Response(child.stderr).text()).toContain("adapter startup rejected");
});

test("stdio EOF drains a running operation and its cleanup", async () => {
  const session = await connect();
  const pending = session.client
    .callTool({
      name: "operation_1",
      arguments: { value: "hello", delay: 150 },
    })
    .catch(() => undefined);
  await waitForLog(session, "fixture setup");
  await session.client.close();
  await pending;
  expect(session.logs()).toContain("fixture completed");
  expect(session.logs()).toContain("fixture cleanup");
});

test("SDK frame limit closes transport without running business code", async () => {
  const child = Bun.spawn([process.execPath, server], {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  child.stdin.write("x".repeat(5000));
  child.stdin.flush();
  const exit = child.exited;
  const timer = setTimeout(() => child.kill(), 2000);
  try {
    expect(await exit).toBe(0);
    const logs = await new Response(child.stderr).text();
    expect(logs).toContain("MCP protocol error");
    expect(logs).not.toContain("fixture setup");
    expect(await new Response(child.stdout).text()).toBe("");
  } finally {
    clearTimeout(timer);
    child.stdin.end();
  }
});
