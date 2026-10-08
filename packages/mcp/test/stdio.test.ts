import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { call, inspect } from "@lenso/cli";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";

const server = `${import.meta.dir}/fixtures/server.ts`;

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});

async function invocationFixture(
  entry: "separate" | "empty" | "legacy" = "separate",
  binding?: "allow" | "deny",
  allow: string[] | null = entry === "separate"
    ? ["mcpOnly", "secure", "confirm", "approve"]
    : ["cliOnly"],
  maxOutputBytes?: number,
) {
  const root = await mkdtemp(join(import.meta.dir, ".stdio-invocation-"));
  directories.push(root);
  await Bun.write(
    join(root, "lenso.config.ts"),
    `
    import { defineApp, definePlugin } from "@lenso/core";
    import { defineOperation } from "@lenso/cli";
    const input = { "~standard": {
      version: 1, vendor: "invocation-test",
      jsonSchema: { input: () => ({
        type: "object", properties: { name: { type: "string" } }, required: ["name"]
      }) },
      validate(value) {
        console.error("bound validate");
        if (typeof value?.name !== "string" || value.name.endsWith("!"))
          return { issues: [{ message: "Raw name required", path: ["name"] }] };
        return { value: { ...value, name: value.name.toUpperCase() + "!" } };
      }
    }};
    const plugin = definePlugin({
      id: "bound",
      setup({ onCleanup }) {
        console.error("bound setup");
        onCleanup(async () => {
          await Bun.sleep(10);
          console.error("bound closed");
        });
        return {
          marker: "original-service",
          async cliOnly(value) { return { name: value.name }; },
          async mcpOnly(value) {
            console.error("bound side-effect");
            if (value.name === "BIG!") return "z".repeat(1536 * 1024);
            return { name: value.name, marker: this.marker };
          },
          async secure(value, context) {
            console.error("bound side-effect");
            return {
              name: value.name, actor: context.actor.id,
              sameService: context.service === this, marker: this.marker,
              token: "private-result"
            };
          },
          async confirm() { console.error("bound side-effect"); return {}; },
          async approve() { console.error("bound side-effect"); return {}; }
        };
      }
    });
    const operation = (method, extra = {}) => defineOperation({
      plugin, method, description: method, input, ...extra
    });
    export const operations = [operation("cliOnly")];
    ${
      entry === "legacy"
        ? ""
        : `export const mcpOperations = ${
            entry === "empty"
              ? "[]"
              : `[
      operation("mcpOnly"),
      operation("secure", { context: true, confirmation: "required", approval: "required" }),
      operation("confirm", { confirmation: "required" }),
      operation("approve", { approval: "required" })
    ]`
          };`
    }
    export const operationBinding = (_operation, _input, running) => {
      console.error("CLI binding borrowed");
      return {
        context: { actor: { id: "cli-owner" }, service: running.get(plugin) },
        confirm: () => true, approve: () => true
      };
    };
    export default defineApp({ plugins: [plugin] });
  `,
  );
  const launcher = join(root, "server.ts");
  await Bun.write(
    launcher,
    `
    import { serveStdio } from "@lenso/mcp";
    await serveStdio({
      root: import.meta.dir,
      ${maxOutputBytes === undefined ? "" : `maxOutputBytes: ${maxOutputBytes},`}
      ${allow === null ? "" : `allow: ${JSON.stringify(allow.map((method) => ({ pluginId: "bound", method })))},`}
      ${
        binding === undefined
          ? ""
          : `binding(operation, input, running) {
        console.error("launch binding " + input.name);
        return {
          context: { actor: { id: "mcp-owner" }, service: running.get(operation.plugin) },
          confirm: () => ${binding === "allow"},
          approve: () => ${binding === "allow"}
        };
      },`
      }
    }).catch(() => {
      process.stderr.write("adapter startup rejected\\n");
      process.exitCode = 1;
    });
  `,
  );
  return { root, launcher };
}

async function connect(launcher = server) {
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [launcher],
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
    expect(JSON.stringify(largeDiagnostic)).not.toContain("details");
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

test("MCP-only declarations are callable but absent from CLI discovery and invocation", async () => {
  const fixture = await invocationFixture("separate", undefined, ["mcpOnly"]);
  expect((await inspect(fixture.root)).operations.map((operation) => operation.method)).toEqual([
    "cliOnly",
  ]);
  await expect(call(fixture.root, "bound", "mcpOnly", { name: "Ada" })).rejects.toThrow(
    "not explicitly exposed",
  );
  const session = await connect(fixture.launcher);
  try {
    const { tools } = await session.client.listTools();
    expect(tools.map((tool) => tool.title)).toEqual(["bound.mcpOnly"]);
    expect(session.logs()).not.toContain("bound setup");
    const result = await session.client.callTool({
      name: tools[0]!.name,
      arguments: { name: "Ada" },
    });
    expect(result.isError).not.toBe(true);
    expect(JSON.parse((result.content as Array<{ text: string }>)[0]!.text)).toEqual({
      name: "ADA!",
      marker: "original-service",
    });
    expect(session.logs().match(/bound validate/g)).toHaveLength(1);
    expect(session.logs().match(/bound setup/g)).toHaveLength(1);
    expect(session.logs().match(/bound closed/g)).toHaveLength(1);
    expect(session.logs()).not.toContain("CLI binding borrowed");
    await expect(
      session.client.callTool({ name: "bound.cliOnly", arguments: { name: "Ada" } }),
    ).rejects.toThrow("Tool is not allowlisted.");
  } finally {
    await session.client.close();
  }
});

test("stdio startup requires an explicit launch allowlist", async () => {
  const fixture = await invocationFixture("legacy", undefined, null);
  const child = Bun.spawn([process.execPath, fixture.launcher], { stdout: "pipe", stderr: "pipe" });
  const timer = setTimeout(() => child.kill(), 2000);
  try {
    expect(await child.exited).toBe(1);
    expect(await new Response(child.stdout).text()).toBe("");
    const logs = await new Response(child.stderr).text();
    expect(logs).toContain("adapter startup rejected");
    expect(logs).not.toContain("bound setup");
  } finally {
    clearTimeout(timer);
    if (child.exitCode === null) child.kill();
  }
});

test("legacy CLI declarations are a fallback, but an explicit empty MCP list disables it", async () => {
  const legacy = await invocationFixture("legacy");
  const session = await connect(legacy.launcher);
  try {
    const { tools } = await session.client.listTools();
    expect(tools.map((tool) => tool.title)).toEqual(["bound.cliOnly"]);
    expect(
      (await session.client.callTool({ name: tools[0]!.name, arguments: { name: "Ada" } })).isError,
    ).not.toBe(true);
  } finally {
    await session.client.close();
  }
  const empty = await invocationFixture("empty");
  const child = Bun.spawn([process.execPath, empty.launcher], { stdout: "pipe", stderr: "pipe" });
  const timer = setTimeout(() => child.kill(), 2000);
  try {
    expect(await child.exited).toBe(1);
    expect(await new Response(child.stdout).text()).toBe("");
    expect(await new Response(child.stderr).text()).toContain("adapter startup rejected");
  } finally {
    clearTimeout(timer);
    if (child.exitCode === null) child.kill();
  }
  const disabled = await invocationFixture("empty", undefined, []);
  const disabledSession = await connect(disabled.launcher);
  try {
    expect((await disabledSession.client.listTools()).tools).toEqual([]);
    expect(disabledSession.logs()).not.toContain("bound setup");
  } finally {
    await disabledSession.client.close();
  }
});

test.each(["missing", "deny"] as const)(
  "MCP refuses missing context/gates and false launch gates without borrowing CLI binding (%s)",
  async (binding) => {
    const fixture = await invocationFixture(
      "separate",
      binding === "missing" ? undefined : binding,
    );
    const session = await connect(fixture.launcher);
    try {
      for (const [name, code] of [
        [
          "operation_1",
          binding === "missing" ? "missing-context-binding" : "confirmation-required",
        ],
        ["operation_2", "confirmation-required"],
        ["operation_3", "approval-required"],
      ]) {
        const result = await session.client.callTool({
          name: name!,
          arguments: {
            name: "Ada",
            actor: "attacker",
            context: { actor: "attacker" },
            confirmed: true,
            approved: true,
          },
        });
        expect(result.isError).toBe(true);
        expect(JSON.parse((result.content as Array<{ text: string }>)[0]!.text).code).toBe(code);
      }
      expect(session.logs().match(/bound setup/g)).toHaveLength(3);
      expect(session.logs().match(/bound closed/g)).toHaveLength(3);
      expect(session.logs()).not.toContain("bound side-effect");
      expect(session.logs()).not.toContain("CLI binding borrowed");
    } finally {
      await session.client.close();
    }
  },
);

test("only explicit MCP launch binding supplies identity, validated input and required gates", async () => {
  const fixture = await invocationFixture("separate", "allow");
  const session = await connect(fixture.launcher);
  try {
    const result = await session.client.callTool({
      name: "operation_1",
      arguments: {
        name: "Ada",
        actor: "attacker",
        context: { actor: "attacker" },
        confirmed: false,
        approved: false,
      },
    });
    expect(result.isError).not.toBe(true);
    expect(JSON.parse((result.content as Array<{ text: string }>)[0]!.text)).toEqual({
      name: "ADA!",
      actor: "mcp-owner",
      sameService: true,
      marker: "original-service",
      token: "[REDACTED]",
    });
    expect(session.logs()).toContain("launch binding ADA!");
    expect(session.logs().match(/bound validate/g)).toHaveLength(1);
    expect(session.logs().match(/bound setup/g)).toHaveLength(1);
    expect(session.logs().match(/bound closed/g)).toHaveLength(1);
    expect(session.logs()).not.toContain("CLI binding borrowed");
    expect(JSON.stringify(result)).not.toContain("private-result");
  } finally {
    await session.client.close();
  }
});

test("MCP forwards the host output budget above the shared default", async () => {
  const fixture = await invocationFixture("separate", undefined, ["mcpOnly"], 2 * 1024 * 1024);
  const session = await connect(fixture.launcher);
  try {
    const result = await session.client.callTool({
      name: "operation_0",
      arguments: { name: "big" },
    });
    expect(result.isError).not.toBe(true);
    expect(JSON.parse((result.content as Array<{ text: string }>)[0]!.text).length).toBe(
      1536 * 1024,
    );
    expect(session.logs()).toContain("bound closed");
  } finally {
    await session.client.close();
  }
});
