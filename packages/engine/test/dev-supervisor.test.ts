import { expect, test } from "bun:test";
import { chmod, mkdtemp, mkdir, rm } from "node:fs/promises";
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
