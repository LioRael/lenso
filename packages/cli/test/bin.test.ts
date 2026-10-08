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
  expect((await run(["inspect", "example", "missing"])).result.error).toMatchObject({
    code: "unknown-operation",
    pluginId: "example",
    operation: "example.missing",
  });
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
  await Bun.write(
    join(root, "lenso.config.ts"),
    `const dependency={id:'database:second',setup(){}};
     export default {plugins:[{id:'notes',source:{file:'src/notes.ts',export:'notes'},requires:[dependency],setup(){throw Error('must not start')}}]};`,
  );
  const missing = await run(["inspect"]);
  expect(missing.code).toBe(3);
  expect(missing.result.error.causes[0]).toMatchObject({
    code: "missing-dependency",
    pluginId: "notes",
    dependencyId: "database:second",
    source: { file: "src/notes.ts", export: "notes" },
  });
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
  const credentialInput = z.object({
    password: z.string().default("secret-default"),
    token: z.string(),
    default: z.string(),
  });
  const credentialOperation = { ...operation, input: credentialInput };
  const credentialSchema = describeOperation(credentialOperation, "lenso.config.ts").inputSchema;
  expect(credentialSchema?.properties).toEqual({
    password: { type: "string" },
    token: { type: "string" },
    default: { type: "string" },
  });
  expect(JSON.stringify(credentialSchema)).not.toContain("secret-default");
  expect(() => stableJson(undefined)).toThrow("Output must");
  const cycle: Record<string, unknown> = {};
  cycle.self = cycle;
  expect(() => stableJson(cycle)).toThrow("Output must");
});

test("inspect and generated manifest share canonical operation semantics without app setup", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-operation-catalog-"));
  directories.push(root);
  await Bun.write(
    join(root, "lenso.config.ts"),
    `const plugin={id:'jobs',setup(){throw Error('must not start')}};
     export const operations=[{
       plugin,method:'cancel',description:'Request job cancellation',
       effect:'write',destructive:false,retry:'safe',cancellation:'request-only',
       outputDescription:'requested is not stopped',
       source:{file:'src/jobs.ts',export:'jobs'},
       input:{'~standard':{version:1,vendor:'test',validate:value=>({value})}}
     }];
     export default {plugins:[plugin]};`,
  );
  const bin = fileURLToPath(new URL("../src/bin.ts", import.meta.url));
  async function run(command: string) {
    const child = Bun.spawn([process.execPath, bin, command, "--root", root, "--json"], {
      stdout: "pipe",
      stderr: "pipe",
    });
    const [out, err, status] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    expect(status).toBe(0);
    expect(err).toBe("");
    return JSON.parse(out);
  }
  const inspection = await run("inspect");
  await run("generate");
  const manifest = await Bun.file(join(root, ".lenso/manifest.json")).json();
  expect(manifest.operations).toEqual(inspection.data.operations);
  expect(manifest.operations[0]).toMatchObject({
    cancellation: "request-only",
    destructive: false,
    retry: "safe",
    inputSchema: null,
    schemaAvailability: "runtime-validation-only",
    source: { file: "src/jobs.ts", export: "jobs" },
  });
});

test("source and built CLI preserve diagnostics from separately bundled application CliError", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-app-diagnostic-"));
  directories.push(root);
  const cli = pathToFileURL(Bun.resolveSync("lenso-cli", import.meta.dir)).href;
  await Bun.write(
    join(root, "lenso.config.ts"),
    `import { CliError } from ${JSON.stringify(cli)};
     const plugin={id:'protected',setup(){return {
       async read(){throw new CliError({code:'FORBIDDEN',phase:'invoke',message:'Access denied'})}
     }}};
     export const operations=[{plugin,method:'read',description:'Protected read',
       input:{'~standard':{version:1,vendor:'test',validate:value=>({value})}}}];
     export default {plugins:[plugin]};`,
  );
  for (const bin of ["../src/bin.ts", "../dist/bin.js"]) {
    const child = Bun.spawn(
      [
        process.execPath,
        fileURLToPath(new URL(bin, import.meta.url)),
        "call",
        "protected",
        "read",
        "{}",
        "--root",
        root,
        "--json",
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    const [out, err, status] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    expect(status).toBe(1);
    expect(err).toBe("");
    expect(JSON.parse(out)).toMatchObject({
      ok: false,
      error: { code: "FORBIDDEN", phase: "invoke", message: "Access denied" },
    });
  }
});

test("Engine diagnostics retain CLI JSON codes and exit-code policy after extraction", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-bin-engine-"));
  directories.push(root);
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]};");
  const bin = fileURLToPath(new URL("../src/bin.ts", import.meta.url));
  for (const [command, config, code, exit] of [
    ["check", "{target:'missing'}", "unknown-engine-target", 3],
    [
      "generate",
      "{plugins:[{name:'collision',setup(c){c.generate('client',()=>[])}}]}",
      "engine-capability-conflict",
      3,
    ],
    [
      "generate",
      "{plugins:[{name:'unsafe',setup(c){c.generate('extra',()=>[{path:'../escape',content:'x'}])}}]}",
      "invalid-generated-file",
      1,
    ],
    ["build", "{}", "invalid-build-entry", 1],
  ] as const) {
    await Bun.write(join(root, "lenso.engine.ts"), `export default ${config};`);
    const child = Bun.spawn(
      [
        process.execPath,
        bin,
        command,
        "--root",
        root,
        "--json",
        ...(command === "build" ? ["--entry", "missing.ts"] : []),
      ],
      {
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const [stdout, stderr, status] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    expect(status).toBe(exit);
    expect(stderr).toBe("");
    expect(JSON.parse(stdout)).toMatchObject({
      schemaVersion: 1,
      ok: false,
      error: { code },
    });
  }
});
