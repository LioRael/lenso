import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm, realpath } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { devConditionArgs, startEngineDevCycle } from "../src/engine-dev";
import { diagnostic } from "../src/diagnostics";

test("dev children inherit only custom condition flags, not eval or preload arguments", () => {
  expect(
    devConditionArgs([
      "--preload",
      "./telemetry.ts",
      "--conditions=lenso-source",
      "--conditions",
      "another-condition",
      "-e",
      "throw Error('parent only')",
    ]),
  ).toEqual(["--conditions=lenso-source", "--conditions=another-condition"]);
  expect(devConditionArgs(["-u", "first,second", "--conditions=second"])).toEqual([
    "--conditions=first,second",
    "--conditions=second",
  ]);
});

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

test("custom conditions reach both fresh Engine workers and application runtimes", async () => {
  const root = await fixture(`
    import marker from 'marker';
    export default {plugins:[{name:'conditions',setup(c){
      c.dev('marker',()=>Bun.write(c.root+'/worker-marker',marker));
    }}]}`);
  await Bun.write(
    join(root, "node_modules/marker/package.json"),
    JSON.stringify({
      name: "marker",
      type: "module",
      exports: { custom: "./source.ts", default: "./dist.js" },
    }),
  );
  await Bun.write(join(root, "node_modules/marker/source.ts"), "export default 'CURRENT_SOURCE'");
  await Bun.write(join(root, "node_modules/marker/dist.js"), "export default 'STALE_DIST'");
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import marker from 'marker';
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready',capabilities:[marker]});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  const driver = join(root, "driver.ts");
  await Bun.write(
    driver,
    `
    import {createDevSupervisor} from ${JSON.stringify(resolve(import.meta.dir, "../src/dev.ts"))};
    let ready;
    const supervisor=await createDevSupervisor({root:${JSON.stringify(root)},stdout:'ignore',stderr:'ignore',onEvent(event){
      if(event.type==='ready') ready=event.capabilities[0];
    }});
    try {
      const deadline=Date.now()+4000;
      while(!ready) {if(Date.now()>deadline) throw Error('no ready');await Bun.sleep(20);}
      console.log(ready+':'+await Bun.file(${JSON.stringify(join(root, "worker-marker"))}).text());
    } finally {await supervisor.close();}
  `,
  );
  const child = Bun.spawn([process.execPath, "--conditions=custom", driver], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const stderr = await new Response(child.stderr).text();
  expect(await child.exited).toBe(0);
  expect(stderr).toBe("");
  expect(await new Response(child.stdout).text()).toBe("CURRENT_SOURCE:CURRENT_SOURCE\n");
});
