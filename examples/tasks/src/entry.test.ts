import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { CallToolResultSchema } from "@modelcontextprotocol/sdk/types.js";

test.skipIf(!process.env.TASK_TEST_DATABASE_URL)(
  "actual task CLI and MCP entries share durable ownership and safe diagnostics",
  async () => {
    const directory = await mkdtemp(join(tmpdir(), "tasks-entry-"));
    const clients: Client[] = [];
    try {
      const sourceFile = join(directory, "test-auth.ts");
      const authModule = pathToFileURL(Bun.resolveSync("@lenso/auth", import.meta.dir)).href;
      await Bun.write(
        sourceFile,
        `import {defineSource} from ${JSON.stringify(authModule)};
         export async function connectTaskAuth() {
           return {source:defineSource({
             realmId:'task-example',
             async verify(evidence) {
               return ['fixture-owner','fixture-other'].includes(evidence)
                 ? {status:'verified',subjectId:evidence,kind:'user'}
                 : {status:'rejected'};
             }
           })};
         }`,
      );
      const root = fileURLToPath(new URL("../", import.meta.url));
      const env = {
        DATABASE_URL: process.env.TASK_TEST_DATABASE_URL!,
        TASK_QUEUE_NAME: "authorization-test",
        TASK_AUTH_SOURCE_MODULE: sourceFile,
        TASK_SESSION: "fixture-owner",
      };
      async function cli(args: string[], input = "") {
        const child = Bun.spawn(
          [
            process.execPath,
            fileURLToPath(new URL("../../../packages/cli/dist/bin.js", import.meta.url)),
            ...args,
            "--root",
            root,
            "--json",
          ],
          { env, stdin: new Blob([input]), stdout: "pipe", stderr: "pipe" },
        );
        const [status, out, err] = await Promise.all([
          child.exited,
          new Response(child.stdout).text(),
          new Response(child.stderr).text(),
        ]);
        return { status, result: JSON.parse(out), err };
      }
      const invalid = await cli(
        ["call", "tasks", "submit", "--stdin"],
        '{"reportId":"invalid","rows":[],"actor":{"subjectId":"fixture-other"}}',
      );
      expect(invalid.status).toBe(2);
      expect(invalid.result.error.code).toBe("invalid-input");
      const submitted = await cli(
        ["call", "tasks", "submit", "--stdin"],
        JSON.stringify({
          reportId: `entry-${crypto.randomUUID()}`,
          rows: [1, 2],
          runAt: new Date(Date.now() + 60_000).toISOString(),
        }),
      );
      expect(submitted.status).toBe(0);
      expect(submitted.result.ok).toBe(true);
      const { jobId } = submitted.result.data;
      async function connect(credential: string) {
        const client = new Client({ name: "task-entry-test", version: "1.0.0" });
        clients.push(client);
        await client.connect(
          new StdioClientTransport({
            command: process.execPath,
            args: [fileURLToPath(new URL("./mcp.ts", import.meta.url))],
            cwd: directory,
            env: { ...env, TASK_SESSION: credential },
            stderr: "pipe",
          }),
        );
        return client;
      }
      const owner = await connect("fixture-owner");
      const other = await connect("fixture-other");
      const { tools } = await owner.listTools();
      expect(tools.map((tool) => tool.title)).toEqual([
        "tasks.submit",
        "tasks.query",
        "tasks.cancel",
        "tasks.retry",
        "tasks.report",
      ]);
      const query = tools.find((tool) => tool.title === "tasks.query")!;
      const cancel = tools.find((tool) => tool.title === "tasks.cancel")!;
      function decode(value: unknown) {
        const block = CallToolResultSchema.parse(value).content[0];
        if (block?.type !== "text") throw new Error("Expected text tool content.");
        return JSON.parse(block.text);
      }
      const owned = await owner.callTool({ name: query.name, arguments: { jobId } });
      expect(decode(owned)).toMatchObject({ state: "pending", cancelRequested: false });
      const denied = await other.callTool({ name: query.name, arguments: { jobId } });
      expect(denied.isError).toBe(true);
      expect(decode(denied).code).toBe("FORBIDDEN");
      const cancelled = await owner.callTool({ name: cancel.name, arguments: { jobId } });
      expect(decode(cancelled)).toBe("cancelled");
      const again = await cli(["call", "tasks", "query", "--stdin"], JSON.stringify({ jobId }));
      expect(again.result.data.state).toBe("cancelled");
    } finally {
      await Promise.all(clients.map((client) => client.close()));
      await rm(directory, { recursive: true, force: true });
    }
  },
  15_000,
);
