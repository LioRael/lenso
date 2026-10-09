import { expect, test } from "bun:test";
import { chmod, mkdtemp, mkdir, rm, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createDevSupervisor, type DevSupervisorEvent } from "../src/dev";

async function until(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 8000;
  while (!(await check())) {
    if (Date.now() > deadline) throw new Error("Development fixture did not settle");
    await Bun.sleep(20);
  }
}
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "lenso-engine-dev-"));
  await mkdir(join(root, "src"));
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]};");
  return root;
}

test("programmatic dev confirms IPC and hooks, restarts fresh resources, and closes in order", async () => {
  const root = await fixture();
  await mkdir(join(root, "content"));
  const input = join(root, "content/message.md");
  const log = join(root, "events.log");
  await Bun.write(input, "first");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    import {appendFile} from 'node:fs/promises';
    export default {plugins:[{name:'content',setup(c){
      c.watch('content');
      let value;
      const log = event => appendFile(c.root+'/events.log',event+':'+value+'\\n');
      c.onCleanup(()=>log('cleanup'));
      c.generate('content',async()=>{value=await Bun.file(c.root+'/content/message.md').text();return [{path:'message.ts',content:'export default '+JSON.stringify(value)}]});
      c.dev('log',async event=>{if(event==='ready') await Bun.sleep(100);await log(event)});
    }}]};`,
  );
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import message from '../.lenso/message';
    import {appendFile} from 'node:fs/promises';
    import {reportDevReady} from ${JSON.stringify(resolve(import.meta.dir, "../src/dev-ready.ts"))};
    const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch:()=>new Response(message)});
    reportDevReady({urls:[server.url],capabilities:['http']});
    process.on('SIGTERM',async()=>{server.stop(true);await appendFile(${JSON.stringify(log)},'runtime-stop:'+message+'\\n');process.exit(0)});`,
  );
  const events: DevSupervisorEvent[] = [];
  const readyLogs: Promise<string>[] = [];
  const supervisor = await createDevSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    onEvent(event) {
      events.push(event);
      if (event.type === "ready") readyLogs.push(Bun.file(log).text());
      // A broken observer cannot derail scheduling or shutdown.
      throw new Error("observer failed");
    },
  });
  const ready = () => events.filter((event) => event.type === "ready");
  try {
    await until(() => ready().length === 1);
    expect(await (await fetch(ready()[0]!.urls![0]!)).text()).toBe("first");
    expect(ready()[0]!.capabilities).toEqual(["http"]);
    expect(await readyLogs[0]).toBe("beforeStart:first\nready:first\n");
    await Bun.write(input, "second");
    await until(() => ready().length === 2);
    expect(await (await fetch(ready()[1]!.urls![0]!)).text()).toBe("second");
    expect(await readyLogs[1]).toBe(
      "beforeStart:first\nready:first\nruntime-stop:first\ncleanup:first\nbeforeStart:second\nready:second\n",
    );
    await Bun.sleep(250);
    expect(ready()).toHaveLength(2);
    expect(events.filter((event) => event.type === "failed")).toEqual([]);
    const closing = supervisor.close();
    expect(supervisor.close()).toBe(closing);
    await closing;
    await supervisor.done;
    expect((await Bun.file(log).text()).endsWith("runtime-stop:second\ncleanup:second\n")).toBe(
      true,
    );
    await Bun.write(input, "third");
    await Bun.sleep(200);
    expect(ready()).toHaveLength(2);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
  }
}, 20000);

test("failed Engine starts stay unready and recover; unexpected runtime exits continue watching", async () => {
  const root = await fixture();
  const config = join(root, "lenso.engine.ts");
  const entry = join(root, "src/server.ts");
  await Bun.write(
    config,
    "export default {plugins:[{name:'broken',setup(){throw new Error('private')}}]};",
  );
  await Bun.write(entry, "process.exit(7);");
  const events: DevSupervisorEvent[] = [];
  const supervisor = await createDevSupervisor({
    root,
    onEvent: (event) => {
      events.push(event);
    },
  });
  try {
    expect(events.some((event) => event.type === "failed")).toBe(true);
    expect(events.some((event) => event.type === "ready")).toBe(false);
    await Bun.write(config, "export default {plugins:[]};");
    await until(() => events.some((event) => event.type === "exited" && event.code === 7));
    expect(events.some((event) => event.type === "ready")).toBe(false);
    await Bun.write(
      entry,
      `
      process.send?.({type:'lenso:dev-ready',urls:[42]});
      const timer=setInterval(()=>{},1000);
      setTimeout(()=>process.send?.({type:'lenso:dev-ready',capabilities:['recovered']}),100);
      process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
    );
    await until(() => events.some((event) => event.type === "ready"));
    expect(events.filter((event) => event.type === "ready")).toEqual([
      { type: "ready", urls: undefined, capabilities: ["recovered"] },
    ]);
  } finally {
    await supervisor.close();
    await supervisor.done;
    await rm(root, { recursive: true, force: true });
  }
}, 20000);

test("ready hook failure stops runtime before cleanup; cleanup failures survive close and restart", async () => {
  const root = await fixture();
  const log = join(root, "events.log");
  const config = join(root, "lenso.engine.ts");
  await Bun.write(
    config,
    `
    import {appendFile} from 'node:fs/promises';
    export default {plugins:[{name:'broken',setup(c){
      c.onCleanup(async()=>{await appendFile(c.root+'/events.log','cleanup\\n');throw new Error('private cleanup')});
      c.dev('fail',event=>{if(event==='ready') throw new Error('private ready')});
    }}]};`,
  );
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import {appendFile} from 'node:fs/promises';
    const timer=setInterval(()=>{},1000);
    process.on('SIGTERM',async()=>{clearInterval(timer);await appendFile(${JSON.stringify(log)},'runtime-stop\\n');process.disconnect?.()});
    process.send?.({type:'lenso:dev-ready'});`,
  );
  const events: DevSupervisorEvent[] = [];
  const supervisor = await createDevSupervisor({
    root,
    onEvent: (event) => {
      events.push(event);
    },
  });
  try {
    await until(
      async () =>
        (await Bun.file(log).exists()) && (await Bun.file(log).text()).includes("cleanup"),
    );
    expect(await Bun.file(log).text()).toBe("runtime-stop\ncleanup\n");
    expect(events.some((event) => event.type === "ready")).toBe(false);
    await Bun.write(config, "export default {plugins:[]};");
    await until(() => events.some((event) => event.type === "ready"));
    const closing = supervisor.close();
    expect(supervisor.close()).toBe(closing);
    await expect(closing).rejects.toThrow();
    await supervisor.done;
    expect(
      events.some(
        (event) => event.type === "failed" && event.diagnostic.code === "engine-and-cleanup-failed",
      ),
    ).toBe(true);
  } finally {
    await supervisor.close().catch(() => {});
    await rm(root, { recursive: true, force: true });
  }
}, 20000);

test("startup rollback cleanup failure remains observable through supervisor close", async () => {
  const root = await fixture();
  await Bun.write(join(root, "src/server.ts"), "export {};");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    export default {plugins:[{name:'startup-resource',setup(c){
      c.onCleanup(()=>{throw new Error('private cleanup')});
      throw new Error('private startup');
    }}]};`,
  );
  const events: DevSupervisorEvent[] = [];
  const supervisor = await createDevSupervisor({
    root,
    onEvent(event) {
      events.push(event);
    },
  });
  try {
    expect(
      events.some(
        (event) => event.type === "failed" && event.diagnostic.code === "engine-and-cleanup-failed",
      ),
    ).toBe(true);
    const closing = supervisor.close();
    expect(supervisor.close()).toBe(closing);
    await expect(closing).rejects.toThrow("Engine execution and cleanup failed");
    await supervisor.done;
  } finally {
    await supervisor.close().catch(() => {});
    await rm(root, { recursive: true, force: true });
  }
});

test("explicit directory watches invalidate on nested additions and removals", async () => {
  const root = await fixture();
  await mkdir(join(root, "content"));
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    import {readdir} from 'node:fs/promises';
    export default {plugins:[{name:'directories',setup(c){
      c.watch('content');
      c.generate('count',async()=>[{
        path:'count.ts',
        content:'export default '+(await readdir(c.root+'/content',{recursive:true})).length
      }]);
    }}]};`,
  );
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import count from '../.lenso/count';
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready',capabilities:[String(count)]});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  const ready: string[] = [];
  const supervisor = await createDevSupervisor({
    root,
    onEvent(event) {
      if (event.type === "ready") ready.push(event.capabilities![0]!);
    },
  });
  try {
    await until(() => ready.length === 1);
    await mkdir(join(root, "content/nested"));
    await Bun.write(join(root, "content/nested/note.txt"), "new input");
    await until(() => ready.length === 2);
    await rm(join(root, "content/nested"), { recursive: true });
    await until(() => ready.length === 3);
    expect(ready).toEqual(["0", "2", "0"]);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("an unreadable unrelated directory does not prevent dev startup", async () => {
  const root = await fixture();
  const opaque = join(root, "unrelated-data");
  await mkdir(opaque);
  await chmod(opaque, 0);
  await Bun.write(
    join(root, "src/server.ts"),
    `
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready'});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  let supervisor: Awaited<ReturnType<typeof createDevSupervisor>> | undefined;
  let ready = false;
  try {
    supervisor = await createDevSupervisor({
      root,
      onEvent(event) {
        if (event.type === "ready") ready = true;
      },
    });
    await until(() => ready);
    expect(ready).toBe(true);
  } finally {
    await supervisor?.close();
    await chmod(opaque, 0o700);
    await rm(root, { recursive: true, force: true });
  }
});

test("only imported JSON is a source invalidation input", async () => {
  const root = await fixture();
  await Bun.write(join(root, "value.json"), '{"value":"first"}');
  await Bun.write(join(root, "data.json"), '{"ordinary":"data"}');
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import value from '../value.json';
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready',capabilities:[value.value]});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  const ready: string[] = [];
  const supervisor = await createDevSupervisor({
    root,
    onEvent(event) {
      if (event.type === "ready") ready.push(event.capabilities![0]!);
    },
  });
  try {
    await until(() => ready.length === 1);
    await Bun.write(join(root, "data.json"), '{"ordinary":"changed"}');
    await Bun.sleep(500);
    expect(ready).toEqual(["first"]);
    await Bun.write(join(root, "value.json"), '{"value":"second"}');
    await until(() => ready.length === 2);
    expect(ready).toEqual(["first", "second"]);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("linked package source graphs outside the app root invalidate, including nested imported JSON", async () => {
  const root = await fixture();
  const linked = await mkdtemp(join(tmpdir(), "lenso-dev-linked-"));
  await mkdir(join(root, "node_modules"));
  await symlink(linked, join(root, "node_modules/linked"));
  await Bun.write(join(linked, "package.json"), '{"name":"linked","exports":"./code/main.ts"}');
  await Bun.write(join(linked, "code/main.ts"), "export {default} from './value.json'");
  const value = join(linked, "code/value.json");
  await Bun.write(value, '{"message":"first"}');
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import value from 'linked';
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready',capabilities:[value.message]});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  const ready: string[] = [];
  const supervisor = await createDevSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    onEvent(event) {
      if (event.type === "ready") ready.push(event.capabilities![0]!);
    },
  });
  try {
    await until(() => ready.length === 1);
    await Bun.write(value, '{"message":"second"}');
    await until(() => ready.length === 2);
    expect(ready).toEqual(["first", "second"]);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
    await rm(linked, { recursive: true, force: true });
  }
});

test("failed config imports recover on nested relative source and missing bare package repair", async () => {
  for (const kind of ["relative", "package"]) {
    const root = await fixture();
    const specifier = kind === "relative" ? "./new/nested/module" : "repair-package";
    await Bun.write(
      join(root, "lenso.engine.ts"),
      `import ${JSON.stringify(specifier)}; export default {plugins:[]}`,
    );
    await Bun.write(
      join(root, "src/server.ts"),
      `
      const timer=setInterval(()=>{},1000);
      process.send?.({type:'lenso:dev-ready'});
      process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
    );
    const events: DevSupervisorEvent[] = [];
    const supervisor = await createDevSupervisor({
      root,
      stdout: "ignore",
      stderr: "ignore",
      onEvent(event) {
        events.push(event);
      },
    });
    try {
      expect(events.some((event) => event.type === "failed")).toBe(true);
      if (kind === "relative") await Bun.write(join(root, "new/nested/module.ts"), "export {}");
      else {
        await Bun.write(join(root, "node_modules/repair-package/entry.js"), "export {}");
        await Bun.write(
          join(root, "node_modules/repair-package/package.json"),
          '{"name":"repair-package","main":"entry.js"}',
        );
      }
      await until(() => events.some((event) => event.type === "ready"));
      expect(events.filter((event) => event.type === "ready")).toHaveLength(1);
    } finally {
      await supervisor.close();
      await rm(root, { recursive: true, force: true });
    }
  }
}, 20000);

test("explicit inputs acquired before failed generation recover, then revoked watches stay revoked", async () => {
  const root = await fixture();
  const data = join(root, "business.json");
  await Bun.write(data, '{"ready":false}');
  const config = join(root, "lenso.engine.ts");
  await Bun.write(
    config,
    `
    export default {plugins:[{name:'explicit-recovery',setup(c){
      c.watch('business.json');
      c.generate('test',async()=>{if(!(await Bun.file(c.root+'/business.json').json()).ready) throw Error('not ready');return []});
    }}]}`,
  );
  await Bun.write(
    join(root, "src/server.ts"),
    `
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready'});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  const events: DevSupervisorEvent[] = [];
  const supervisor = await createDevSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    onEvent(event) {
      events.push(event);
    },
  });
  const ready = () => events.filter((event) => event.type === "ready");
  try {
    expect(events.some((event) => event.type === "failed")).toBe(true);
    await Bun.write(data, '{"ready":true}');
    await until(() => ready().length === 1);
    await Bun.write(
      config,
      `export default {plugins:[{name:'revoked',setup(c){
      const first=c.watch('business.json'); const second=c.watch('business.json');
      first(); second();
    }}]}`,
    );
    await until(() => ready().length === 2);
    await Bun.write(data, '{"ready":false}');
    await Bun.sleep(400);
    expect(ready()).toHaveLength(2);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("manifest and extended config changes invalidate the current graph", async () => {
  const root = await fixture();
  await Bun.write(join(root, "tsconfig.json"), '{"extends":"./resolution.json"}');
  await Bun.write(join(root, "resolution.json"), '{"compilerOptions":{}}');
  await Bun.write(join(root, "package.json"), '{"name":"dev-fixture","type":"module"}');
  await Bun.write(
    join(root, "src/server.ts"),
    `
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready'});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  let ready = 0;
  const supervisor = await createDevSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    onEvent(event) {
      if (event.type === "ready") ready++;
    },
  });
  try {
    await until(() => ready === 1);
    await Bun.write(join(root, "resolution.json"), '{"compilerOptions":{"strict":true}}');
    await until(() => ready === 2);
    await Bun.write(join(root, "package.json"), '{"name":"dev-fixture-renamed","type":"module"}');
    await until(() => ready === 3);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("changes during generation queue a serial restart, and reentrant close owns all cleanup", async () => {
  const root = await fixture();
  const value = join(root, "value.ts");
  const gate = join(root, ".lenso/generating");
  await Bun.write(value, "export default 'first'");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    import {writeFileSync,unlinkSync} from 'node:fs';
    export default {plugins:[{name:'serial',setup(c){
      writeFileSync(c.root+'/exclusive.lock','owned',{flag:'wx'});
      c.onCleanup(()=>unlinkSync(c.root+'/exclusive.lock'));
      c.generate('slow',async()=>{await Bun.write(c.root+'/.lenso/generating','yes');await Bun.sleep(250);return []});
    }}]}`,
  );
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import value from '../value';
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready',capabilities:[value]});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
  const events: DevSupervisorEvent[] = [];
  const supervisor = await createDevSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    onEvent(event) {
      events.push(event);
    },
  });
  const ready = () => events.filter((event) => event.type === "ready");
  try {
    await until(() => ready().length === 1);
    await rm(gate);
    await Bun.write(value, "export default 'second'");
    await until(() => Bun.file(gate).exists());
    await Bun.write(value, "export default 'third'");
    await until(() => ready().some((event) => event.capabilities?.[0] === "third"));
    await Bun.sleep(400);
    expect(events.filter((event) => event.type === "failed")).toEqual([]);
    const closing = supervisor.close();
    expect(supervisor.close()).toBe(closing);
    await closing;
    expect(await Bun.file(join(root, "exclusive.lock")).exists()).toBe(false);
    const starts = events.filter((event) => event.type === "starting").length;
    await Bun.write(value, "export default 'after-close'");
    await Bun.sleep(200);
    expect(events.filter((event) => event.type === "starting")).toHaveLength(starts);
  } finally {
    await supervisor.close();
    await rm(root, { recursive: true, force: true });
  }
}, 20000);

test("missing declared path aliases and linked wildcard exports recover from new source in arbitrary directories", async () => {
  for (const kind of ["alias", "wildcard"]) {
    const root = await fixture();
    const linked = await mkdtemp(join(tmpdir(), "lenso-dev-wildcard-"));
    const specifier = kind === "alias" ? "@input/created" : "linked/feature/created";
    const leaf =
      kind === "alias"
        ? join(root, "arbitrary/place/created.ts")
        : join(linked, "free/directory/created.ts");
    if (kind === "alias")
      await Bun.write(
        join(root, "tsconfig.json"),
        JSON.stringify({
          compilerOptions: { baseUrl: ".", paths: { "@input/*": ["arbitrary/place/*"] } },
        }),
      );
    else {
      await mkdir(join(root, "node_modules"));
      await symlink(linked, join(root, "node_modules/linked"));
      await Bun.write(
        join(linked, "package.json"),
        JSON.stringify({
          name: "linked",
          type: "module",
          exports: { "./feature/*": { import: "./free/directory/*.ts" } },
        }),
      );
    }
    await Bun.write(
      join(root, "lenso.config.ts"),
      `import ${JSON.stringify(specifier)}; export default {plugins:[]}`,
    );
    await Bun.write(
      join(root, "src/server.ts"),
      `
      import value from ${JSON.stringify(specifier)};
      const timer=setInterval(()=>{},1000);
      process.send?.({type:'lenso:dev-ready',capabilities:[value]});
      process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
    );
    const events: DevSupervisorEvent[] = [];
    const supervisor = await createDevSupervisor({
      root,
      stdout: "ignore",
      stderr: "ignore",
      onEvent(event) {
        events.push(event);
      },
    });
    try {
      expect(events.some((event) => event.type === "failed")).toBe(true);
      await Bun.write(leaf, "export default 'repaired'");
      await until(() => events.some((event) => event.type === "ready"));
      expect(
        events.filter((event) => event.type === "ready").map((event) => event.capabilities),
      ).toEqual([["repaired"]]);
      await Bun.write(leaf, "export default 'current-source'");
      await until(() => events.filter((event) => event.type === "ready").length === 2);
      expect(
        events.filter((event) => event.type === "ready").map((event) => event.capabilities),
      ).toEqual([["repaired"], ["current-source"]]);
    } finally {
      await supervisor.close();
      await rm(root, { recursive: true, force: true });
      await rm(linked, { recursive: true, force: true });
    }
  }
}, 20000);
