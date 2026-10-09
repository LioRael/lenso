import { expect, test } from "bun:test";
import { mkdtemp, mkdir, rm, realpath } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createDevSupervisor, type DevSupervisorEvent } from "../src/dev";
import { startEngineDevCycle } from "../src/engine-dev";
import { build } from "../src/engine";

async function until(check: () => boolean) {
  const deadline = Date.now() + 8000;
  while (!check()) {
    if (Date.now() > deadline) throw Error("Selected application did not become ready");
    await Bun.sleep(20);
  }
}

test("dev worker carries config and convention/explicit entry through real selected application startup", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-selected-dev-"));
  const core = Bun.resolveSync("@lenso/core", import.meta.dir);
  const events: DevSupervisorEvent[] = [];
  const selected = join(root, "apps/selected");
  const other = join(root, "apps/other");
  await mkdir(join(selected, "config"), { recursive: true });
  await mkdir(join(selected, "runtime"), { recursive: true });
  await mkdir(other, { recursive: true });
  await Bun.write(join(root, "package.json"), JSON.stringify({ workspaces: ["apps/*"] }));
  await Bun.write(
    join(other, "lenso.config.ts"),
    `await Bun.write(${JSON.stringify(join(other, "imported"))},'yes');export default {plugins:[]};`,
  );
  await Bun.write(
    join(other, "lenso.engine.ts"),
    `await Bun.write(${JSON.stringify(join(other, "engine-imported"))},'yes');export default {};`,
  );
  await Bun.write(join(selected, "lenso.config.ts"), "throw Error('canonical must not load');");
  await Bun.write(
    join(selected, "config/application.ts"),
    `
    import {appendFileSync} from 'node:fs';
    appendFileSync(${JSON.stringify(join(selected, "imports.log"))},'selected\\n');
    export default {plugins:[{id:'selected',setup({onCleanup}){
      appendFileSync(${JSON.stringify(join(selected, "lifetime.log"))},'setup\\n');
      onCleanup(()=>appendFileSync(${JSON.stringify(join(selected, "lifetime.log"))},'stop\\n'));
    }}]};`,
  );
  await Bun.write(
    join(selected, "lenso.engine.ts"),
    `
    import {appendFileSync} from 'node:fs';
    export default {plugins:[{name:'entry',setup(c){
      c.onCleanup(()=>appendFileSync(c.root+'/lifetime.log','engine-cleanup\\n'));
      c.convention(()=>({config:'missing.ts',entry:'runtime/main.ts'}),{replace:'lenso/defaults'});
      c.dev('selected',event=>appendFileSync(c.root+'/lifetime.log',event+'\\n'));
    }}]};`,
  );
  for (const name of ["main", "override"]) {
    await Bun.write(
      join(selected, `runtime/${name}.ts`),
      `
      import {startApp} from ${JSON.stringify(core)};
      import {app} from '../.lenso/server';
      const running=await startApp(app);
      const timer=setInterval(()=>{},1000);
      process.on('SIGTERM',async()=>{clearInterval(timer);await running.stop();process.disconnect?.()});
      process.send?.({type:'lenso:dev-ready',capabilities:['${name}']});`,
    );
  }
  const target = { root: selected, config: "config/application.ts" };
  try {
    const cycle = await startEngineDevCycle(target, "runtime/override.ts");
    try {
      expect(cycle.entry).toBe(join(selected, "runtime/override.ts"));
      expect(cycle.watchFiles).toContain(await realpath(join(selected, "config/application.ts")));
      expect(cycle.watchFiles).not.toContain(await realpath(join(selected, "lenso.config.ts")));
      expect((await Bun.file(join(selected, ".lenso/manifest.json")).json()).source).toBe(
        target.config,
      );
    } finally {
      await cycle.close();
    }
    for (const entry of [undefined, "runtime/override.ts"]) {
      events.length = 0;
      const supervisor = await createDevSupervisor({
        ...target,
        entry,
        stdout: "ignore",
        stderr: "ignore",
        onEvent(event) {
          events.push(event);
        },
      });
      try {
        await until(() =>
          events.some((event) => event.type === "ready" || event.type === "failed"),
        );
        expect(events.filter((event) => event.type === "failed")).toEqual([]);
        expect(events.find((event) => event.type === "ready")).toMatchObject({
          capabilities: [entry ? "override" : "main"],
        });
      } finally {
        await supervisor.close();
        await supervisor.done;
      }
      expect(
        (await Bun.file(join(selected, "lifetime.log")).text()).endsWith("stop\nengine-cleanup\n"),
      ).toBe(true);
    }
    expect(await Bun.file(join(other, "imported")).exists()).toBe(false);
    expect(await Bun.file(join(other, "engine-imported")).exists()).toBe(false);
    expect(await Bun.file(join(other, ".lenso/manifest.json")).exists()).toBe(false);
    await build(target, "runtime/override.ts");
    expect(await Bun.file(join(selected, "dist/override.js")).exists()).toBe(true);
    await expect(build(target, "../outside.ts")).rejects.toThrow("inside application root");
    const supervisor = await createDevSupervisor({
      ...target,
      entry: "../outside.ts",
      onEvent(event) {
        events.push(event);
      },
    });
    try {
      expect(
        events.some(
          (event) => event.type === "failed" && event.diagnostic.code === "dev-entry-missing",
        ),
      ).toBe(true);
    } finally {
      await supervisor.close();
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}, 20000);
