import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm, symlink, stat } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { build, generate } from "../src/engine";
import { diagnostic } from "../src/diagnostics";

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});
const authoring = resolve(import.meta.dir, "../src/engine-authoring.ts");
async function fixture(plugin: string, config = `export default {plugins: []};`) {
  const root = await mkdtemp(join(tmpdir(), "lenso-extensions-"));
  directories.push(root);
  await Bun.write(join(root, "lenso.config.ts"), config);
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `import {defineEnginePlugin,defineEngineConfig} from ${JSON.stringify(authoring)};\n${plugin}`,
  );
  return root;
}
function failureCode(cause: unknown) {
  return diagnostic(cause).code;
}

test("ordered composed stages, explicit replacements, unchanged output and stale ownership", async () => {
  const root = await fixture(`
    export default defineEngineConfig({plugins:[
      defineEnginePlugin({name:'consumer',after:['producer'],setup(c){c.generate('derived',s=>[{path:'derived.ts',content:JSON.stringify(s.sources.map(p=>p.split('/').at(-1)))}]);}}),
      defineEnginePlugin({name:'producer',setup(c){c.discover('extra',()=>['extra.ts']);c.generate('client',()=>[{path:'client.ts',content:'export const custom = true;'}],{replace:'lenso/defaults'});}})
    ]});`);
  await Bun.write(join(root, "extra.ts"), "export const value = 1;");
  await generate(root);
  expect(await Bun.file(join(root, ".lenso/client.ts")).text()).toContain("custom");
  expect(await Bun.file(join(root, ".lenso/derived.ts")).text()).toContain("extra.ts");
  const before = (await stat(join(root, ".lenso/client.ts"))).mtimeMs;
  await generate(root);
  expect((await stat(join(root, ".lenso/client.ts"))).mtimeMs).toBe(before);
  // A fresh process loads the changed config without depending on the CLI.
  await Bun.write(join(root, "lenso.engine.ts"), "export default {plugins: []};");
  const engine = resolve(import.meta.dir, "../src/engine.ts");
  const child = Bun.spawn(
    [
      process.execPath,
      "-e",
      `import {generate} from ${JSON.stringify(engine)}; await generate(${JSON.stringify(root)});`,
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  expect(await child.exited).toBe(0);
  expect(await Bun.file(join(root, ".lenso/derived.ts")).exists()).toBe(false);
});

test("invalid configuration, duplicate names/capabilities and ordering produce attributed diagnostics", async () => {
  for (const [code, expression] of [
    ["invalid-engine-config", `{plugins:42}`],
    ["duplicate-engine-plugin", `{plugins:[{name:'dup',setup(){}},{name:'dup',setup(){}}]}`],
    [
      "engine-capability-conflict",
      `{plugins:[{name:'collision',source:{file:'plugin.ts',line:7},setup(c){c.generate('client',()=>[]);}}]}`,
    ],
    [
      "engine-capability-conflict",
      `{plugins:[{name:'replacement',setup(c){c.target('missing',()=>'',{replace:'lenso/defaults'});}}]}`,
    ],
    [
      "cyclic-engine-order",
      `{plugins:[{name:'a',after:['b'],setup(){}},{name:'b',after:['a'],setup(){}}]}`,
    ],
    ["missing-engine-order", `{plugins:[{name:'a',after:['absent'],setup(){}}]}`],
  ]) {
    const root = await fixture(`export default ${expression};`);
    const cause = await generate(root).catch((error: unknown) => error);
    const detail = diagnostic(cause);
    expect(detail.code).toBe(code);
    if (detail.pluginId === "collision")
      expect(detail.source).toEqual({ file: "plugin.ts", line: 7 });
    expect(await Bun.file(join(root, ".lenso/manifest.json")).exists()).toBe(false);
  }
});

test("generated file conflicts, traversal, symlink escapes and edited output fail before overwrite", async () => {
  for (const [code, paths] of [
    ["generated-file-conflict", ["same.ts", "same.ts"]],
    ["invalid-generated-file", ["../../outside.ts"]],
  ] as const) {
    const root = await fixture(
      `export default {plugins:[{name:'unsafe',setup(c){c.generate('one',()=>${JSON.stringify(paths.map((path) => ({ path, content: "unsafe" })))});}}]};`,
    );
    expect(failureCode(await generate(root).catch((cause: unknown) => cause))).toBe(code);
  }
  const root = await fixture("export default {plugins: []};");
  const outside = await mkdtemp(join(tmpdir(), "lenso-outside-"));
  directories.push(outside);
  await symlink(outside, join(root, ".lenso"));
  expect(failureCode(await generate(root).catch((cause: unknown) => cause))).toBe(
    "unsafe-engine-output",
  );
  expect(await Bun.file(join(outside, "manifest.json")).exists()).toBe(false);
  await rm(join(root, ".lenso"));
  await generate(root);
  await Bun.write(join(root, ".lenso/server.ts"), "hand edited");
  expect(failureCode(await generate(root).catch((cause: unknown) => cause))).toBe(
    "generated-file-modified",
  );
  expect(await Bun.file(join(root, ".lenso/server.ts")).text()).toBe("hand edited");
});

test("all cleanup runs LIFO after failed setup; errors keep names and omit application secrets", async () => {
  const root = await fixture(
    `export default {plugins:[{name:'resource',source:{file:'resource.ts'},setup(c){c.onCleanup(()=>Bun.write(c.root+'/closed','first'));c.onCleanup(()=>{throw Error('token=cleanup-secret')});throw Error('password=setup-secret');}}]};`,
  );
  const detail = diagnostic(await generate(root).catch((cause: unknown) => cause));
  expect(detail.code).toBe("engine-and-cleanup-failed");
  expect(detail.causes?.[0]?.pluginId).toBe("resource");
  expect(JSON.stringify(detail)).not.toContain("secret");
  expect(await Bun.file(join(root, "closed")).text()).toBe("first");
});

test("custom build convention and target use shared bundler", async () => {
  const root = await fixture(
    `export default {target:'custom',plugins:[{name:'custom',setup(c){c.convention(()=>({config:'assembly.ts',entry:'entry.ts'}),{replace:'lenso/defaults'});c.target('custom',c=>c.bundle({entry:c.entry,platform:'browser',packages:'bundle'}));}}]};`,
  );
  await Bun.write(join(root, "assembly.ts"), "export default {plugins:[]};");
  await Bun.write(join(root, "entry.ts"), "export const fetch = () => new Response('custom');");
  expect(await build(root)).toBe(join(root, "dist"));
  expect(await Bun.file(join(root, "dist/entry.js")).text()).toContain("custom");
  expect(await Bun.file(join(root, ".lenso/server.ts")).text()).toContain("assembly");
});
