import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm, symlink, stat, realpath } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { build, call, generate, inspect } from "../src/engine";
import { diagnostic } from "../src/diagnostics";
import { startEngineDevCycle } from "../src/engine-dev";

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
  // Remove a previously active generator in a fresh process to invalidate config imports.
  await Bun.write(join(root, "lenso.engine.ts"), "export default {plugins: []};");
  const cli = resolve(import.meta.dir, "../src/bin.ts");
  const child = Bun.spawn([process.execPath, cli, "generate", "--root", root, "--json"], {
    stdout: "pipe",
    stderr: "pipe",
  });
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
    try {
      await generate(root);
      throw new Error("Expected failure");
    } catch (cause) {
      const detail = diagnostic(cause);
      expect(detail.code).toBe(code);
      if (detail.pluginId === "collision")
        expect(detail.source).toEqual({ file: "plugin.ts", line: 7 });
    }
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
    try {
      await generate(root);
      throw new Error("Expected failure");
    } catch (cause) {
      expect(failureCode(cause)).toBe(code);
    }
  }
  const root = await fixture("export default {plugins: []};");
  const outside = await mkdtemp(join(tmpdir(), "lenso-outside-"));
  directories.push(outside);
  await symlink(outside, join(root, ".lenso"));
  try {
    await generate(root);
    throw new Error("Expected failure");
  } catch (cause) {
    expect(failureCode(cause)).toBe("unsafe-engine-output");
  }
  expect(await Bun.file(join(outside, "manifest.json")).exists()).toBe(false);
  await rm(join(root, ".lenso"));
  await generate(root);
  await Bun.write(join(root, ".lenso/server.ts"), "hand edited");
  try {
    await generate(root);
    throw new Error("Expected failure");
  } catch (cause) {
    expect(failureCode(cause)).toBe("generated-file-modified");
  }
  expect(await Bun.file(join(root, ".lenso/server.ts")).text()).toBe("hand edited");
});

test("all cleanup runs LIFO after failed setup; errors keep names and omit application secrets", async () => {
  const root = await fixture(
    `export default {plugins:[{name:'resource',source:{file:'resource.ts'},setup(c){c.onCleanup(()=>Bun.write(c.root+'/closed','first'));c.onCleanup(()=>{throw Error('token=cleanup-secret')});throw Error('password=setup-secret');}}]};`,
  );
  try {
    await generate(root);
    throw new Error("Expected failure");
  } catch (cause) {
    const detail = diagnostic(cause);
    expect(detail.code).toBe("engine-and-cleanup-failed");
    expect(detail.causes?.[0]?.pluginId).toBe("resource");
    expect(JSON.stringify(detail)).not.toContain("secret");
  }
  expect(await Bun.file(join(root, "closed")).text()).toBe("first");
});

test("runtime inspect/call never load engine code; custom build convention and target use shared bundler", async () => {
  const root = await fixture(
    `throw Error('engine must not load');`,
    `
    const p={id:'service',setup(){return {run:()=>42}}};
    export const operations=[{plugin:p,method:'run',description:'Run',input:{'~standard':{version:1,vendor:'test',validate:value=>({value})}}}];
    export default {plugins:[p]};`,
  );
  expect((await inspect(root)).operations).toHaveLength(1);
  expect(await call(root, "service", "run", {})).toBe(42);
  const custom = await fixture(
    `export default {target:'custom',plugins:[{name:'custom',setup(c){c.convention(()=>({config:'assembly.ts',entry:'entry.ts'}),{replace:'lenso/defaults'});c.target('custom',c=>c.bundle({entry:c.entry,platform:'browser',packages:'bundle'}));}}]};`,
  );
  await Bun.write(join(custom, "assembly.ts"), "export default {plugins:[]};");
  await Bun.write(join(custom, "entry.ts"), "export const fetch = () => new Response('custom');");
  expect(await build(custom)).toBe(join(custom, "dist"));
  expect(await Bun.file(join(custom, "dist/entry.js")).text()).toContain("custom");
  expect(await Bun.file(join(custom, ".lenso/server.ts")).text()).toContain("assembly");
});

test("fresh dev cycles invalidate imported plugin sources, track dependencies, run readiness and idempotent cleanup", async () => {
  const root = await fixture(
    `import {plugin} from './build-plugin'; export default {plugins:[plugin]};`,
  );
  await mkdir(join(root, "content"));
  await mkdir(join(root, "src"));
  await Bun.write(join(root, "src/server.ts"), "import '../.lenso/client'; export {}; ");
  const pluginFile = join(root, "build-plugin.ts");
  const plugin = (value: string) =>
    `export const plugin={name:'dev-resource',setup(c){c.watch('content');c.onCleanup(()=>Bun.write(c.root+'/cleanup','${value}'));c.generate('value',()=>[{path:'value.ts',content:'${value}'}]);c.dev('lifecycle',(event)=>Bun.write(c.root+'/'+event,'${value}'));}};`;
  await Bun.write(pluginFile, plugin("first"));
  const first = await startEngineDevCycle(root);
  expect(first.watchFiles).toContain(await realpath(pluginFile));
  expect(first.watchFiles).toContain(await realpath(join(root, "content")));
  expect(await Bun.file(join(root, "beforeStart")).text()).toBe("first");
  await first.ready();
  expect(await Bun.file(join(root, "ready")).text()).toBe("first");
  expect(first.close()).toBe(first.close());
  await first.close();
  expect(await Bun.file(join(root, "cleanup")).text()).toBe("first");
  await Bun.write(pluginFile, plugin("second"));
  const second = await startEngineDevCycle(root);
  try {
    expect(second.watchFiles.some((path) => path.includes("/.lenso/"))).toBe(false);
    expect(await Bun.file(join(root, ".lenso/value.ts")).text()).toBe("second");
  } finally {
    await second.close();
  }
  expect(await Bun.file(join(root, "cleanup")).text()).toBe("second");
});

test("dev hook resources clean up on readiness failure and worker diagnostics retain cleanup errors", async () => {
  const root = await fixture(
    `export default {plugins:[{name:'dev-failure',source:{file:'dev-plugin.ts'},setup(c){c.dev('resource',event=>{if(event==='ready'){c.onCleanup(()=>Bun.write(c.root+'/closed','yes'));c.onCleanup(()=>{throw Error('secret=cleanup')});throw Error('token=ready');}});}}]};`,
  );
  const cycle = await startEngineDevCycle(root);
  await expect(cycle.ready()).rejects.toThrow("Engine execution and cleanup failed");
  try {
    await cycle.close();
    throw new Error("Expected cleanup failure");
  } catch (cause) {
    const detail = diagnostic(cause);
    expect(detail.code).toBe("engine-and-cleanup-failed");
    expect(JSON.stringify(detail)).not.toContain("secret");
    expect(detail.causes?.[0]?.pluginId).toBe("dev-failure");
  }
  expect(await Bun.file(join(root, "closed")).text()).toBe("yes");
});
