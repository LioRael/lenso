import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";
import { z } from "zod";
import { definePlugin } from "lenso";
import { defineOperation, describeOperation } from "../src/operations";
import { stableJson } from "../src/diagnostics";

const directories: string[] = [];
afterEach(async () => {
  for (const root of directories.splice(0)) await rm(root, { recursive: true, force: true });
});

test("JSON CLI: inspect without setup, shared Zod validation, stdout/stderr, input sources and exit codes", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-bin-"));
  directories.push(root);
  const zod = pathToFileURL(Bun.resolveSync("zod", import.meta.dir)).href;
  await Bun.write(
    join(root, "lenso.config.ts"),
    `
    import { z } from ${JSON.stringify(zod)};
    const input = z.object({name:z.string()});
    const plugin = {id:'example',setup({onCleanup}){
      console.log('setup log');
      onCleanup(()=>console.log('cleanup log'));
      return {async greet({name}){return {message:'Hello '+name,token:'private-value'}},async denied(){throw new Error('Bearer forbidden-secret')}};
    }};
    export const operations = ['greet','denied'].map(method=>({plugin,method,input,description:'Shared service',effect:'write'}));
    export default {plugins:[plugin]};
  `,
  );
  const bin = fileURLToPath(new URL("../src/bin.ts", import.meta.url));
  async function run(args: string[], stdin?: string) {
    const child = Bun.spawn([process.execPath, bin, ...args, "--root", root, "--json"], {
      stdout: "pipe",
      stderr: "pipe",
      stdin: stdin === undefined ? "ignore" : "pipe",
    });
    if (stdin !== undefined && child.stdin && typeof child.stdin !== "number") {
      child.stdin.write(stdin);
      child.stdin.end();
    }
    const [out, err, code] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    return { result: JSON.parse(out), err, code };
  }
  const description = await run(["inspect", "example", "greet"]);
  expect(description.code).toBe(0);
  expect(description.err).toBe("");
  expect(description.result.data.operations[0].inputSchema.properties.name.type).toBe("string");
  const success = await run(["call", "example", "greet", "--stdin"], '{"name":"Ada"}');
  expect(success).toMatchObject({
    code: 0,
    result: { schemaVersion: 1, ok: true, data: { message: "Hello Ada", token: "[REDACTED]" } },
  });
  expect(success.err).toContain("setup log");
  expect(success.err).toContain("cleanup log");
  const path = join(root, "input.json");
  await Bun.write(path, '{"name":"File"}');
  expect((await run(["call", "example", "greet", "--input-file", path])).result.data.message).toBe(
    "Hello File",
  );
  const invalid = await run(["call", "example", "greet", '{"name":42}']);
  expect(invalid).toMatchObject({
    code: 2,
    err: "",
    result: {
      ok: false,
      error: {
        code: "invalid-input",
        phase: "input",
        source: { file: join(root, "lenso.config.ts") },
      },
    },
  });
  expect((await run(["call", "example", "helper"])).result.error.code).toBe("unknown-operation");
  expect((await run(["check", "--unknown"])).code).toBe(2);
  expect((await run(["call", "example", "greet", "bad-json"])).result.error.code).toBe(
    "invalid-json",
  );
  const denied = await run(["call", "example", "denied", '{"name":"Ada"}']);
  expect(denied.code).toBe(1);
  expect(denied.result.error.code).toBe("invocation-failed");
  expect(JSON.stringify(denied)).not.toContain("forbidden-secret");
  expect((await run(["dev"])).code).toBe(2);
  await Bun.write(
    join(root, "lenso.config.ts"),
    `export default {plugins:[{id:'same',setup(){}},{id:'same',setup(){}}]}`,
  );
  // Each process loads the current configuration, unlike in-process import caching.
  const assembly = await run(["check"]);
  expect(assembly.code).toBe(3);
  expect(assembly.result.error.causes[0].code).toBe("duplicate-id");
});

test("schema description derives from the validator; non-JSON output fails", () => {
  const input = z.object({ name: z.string() });
  const plugin = definePlugin({
    id: "example",
    setup: () => ({ greet: async (value: { name: string }) => value }),
  });
  const operation = defineOperation({ plugin, method: "greet", input, description: "Same schema" });
  expect(describeOperation(operation, "lenso.config.ts").inputSchema?.properties).toEqual({
    name: { type: "string" },
  });
  expect(() => stableJson(undefined)).toThrow("Output must");
  const cycle: Record<string, unknown> = {};
  cycle.self = cycle;
  expect(() => stableJson(cycle)).toThrow("Output must");
});
