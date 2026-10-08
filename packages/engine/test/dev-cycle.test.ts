import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm, realpath } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { startEngineDevCycle } from "../src/engine-dev";
import { diagnostic } from "../src/diagnostics";

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});
async function fixture(engineConfig: string) {
  const root = await mkdtemp(join(tmpdir(), "lenso-dev-cycle-"));
  directories.push(root);
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]};");
  await Bun.write(join(root, "lenso.engine.ts"), engineConfig);
  return root;
}

test("fresh dev cycles track imported sources and explicit directories, with cached cleanup", async () => {
  const root = await fixture(
    "import {plugin} from './build-plugin'; export default {plugins:[plugin]};",
  );
  await mkdir(join(root, "content"));
  await mkdir(join(root, "src"));
  await Bun.write(join(root, "src/server.ts"), "import '../.lenso/client'; export {};");
  const pluginFile = join(root, "build-plugin.ts");
  const plugin = (value: string) =>
    `export const plugin={name:'dev-resource',setup(c){c.watch('content');c.onCleanup(()=>Bun.write(c.root+'/cleanup','${value}'));c.generate('value',()=>[{path:'value.ts',content:'${value}'}]);c.dev('lifecycle',event=>Bun.write(c.root+'/'+event,'${value}'));}};`;
  for (const value of ["first", "second"]) {
    await Bun.write(pluginFile, plugin(value));
    const cycle = await startEngineDevCycle(root);
    try {
      expect(cycle.watchFiles).toContain(await realpath(pluginFile));
      expect(cycle.watchFiles).toContain(await realpath(join(root, "content")));
      expect(cycle.watchFiles.some((path) => path.includes("/.lenso/"))).toBe(false);
      expect(await Bun.file(join(root, ".lenso/value.ts")).text()).toBe(value);
      expect(await Bun.file(join(root, "beforeStart")).text()).toBe(value);
      await cycle.ready();
      expect(await Bun.file(join(root, "ready")).text()).toBe(value);
      expect(cycle.close()).toBe(cycle.close());
    } finally {
      await cycle.close();
    }
    expect(await Bun.file(join(root, "cleanup")).text()).toBe(value);
  }
});

test("ready failures defer cleanup until close and preserve combined cleanup diagnostics", async () => {
  const root = await fixture(
    `export default {plugins:[{name:'dev-failure',source:{file:'dev-plugin.ts'},setup(c){c.dev('resource',event=>{if(event==='ready'){c.onCleanup(()=>Bun.write(c.root+'/closed','yes'));c.onCleanup(()=>{throw Error('secret=cleanup')});throw Error('token=ready');}});}}]};`,
  );
  const cycle = await startEngineDevCycle(root);
  try {
    await expect(cycle.ready()).rejects.toThrow("failed during engine-dev");
    expect(await Bun.file(join(root, "closed")).exists()).toBe(false);
    await cycle.close().then(
      () => {
        throw new Error("Expected cleanup failure");
      },
      (cause) => {
        const detail = diagnostic(cause);
        expect(detail.code).toBe("engine-and-cleanup-failed");
        expect(JSON.stringify(detail)).not.toContain("secret");
        expect(detail.causes?.[0]?.pluginId).toBe("dev-failure");
      },
    );
    expect(await Bun.file(join(root, "closed")).text()).toBe("yes");
  } finally {
    await cycle.close().catch(() => {});
  }
});
