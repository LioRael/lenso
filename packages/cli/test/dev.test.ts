import { expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const cli = resolve(import.meta.dir, "../src/bin.ts");
async function until(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 8000;
  while (!(await check())) {
    if (Date.now() > deadline) throw new Error("Development fixture did not settle");
    await Bun.sleep(20);
  }
}

test("dev restarts fresh Engine resources on content changes and reports only confirmed runtime readiness", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-dev-supervisor-"));
  await mkdir(join(root, "src"));
  await mkdir(join(root, "content"));
  const input = join(root, "content/message.md");
  await Bun.write(input, "first");
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]};");
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
      c.dev('log',event=>log(event));
    }}]};`,
  );
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import message from '../.lenso/message';
    import {appendFile} from 'node:fs/promises';
    const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch:()=>new Response(message)});
    process.send?.({type:'lenso:dev-ready',urls:[server.url.href]});
    process.on('SIGTERM',async()=>{server.stop(true);await appendFile(${JSON.stringify(join(root, "events.log"))},'runtime-stop:'+message+'\\n');process.exit(0)});`,
  );
  let output = "";
  const child = Bun.spawn([process.execPath, cli, "dev", "--root", root], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, NO_COLOR: "1" },
  });
  async function collect(stream: ReadableStream<Uint8Array>) {
    for await (const chunk of stream) output += new TextDecoder().decode(chunk);
  }
  const collected = Promise.all([collect(child.stdout), collect(child.stderr)]);
  try {
    await until(() => (output.match(/Ready in/g)?.length ?? 0) === 1);
    const urls = () =>
      [...output.matchAll(/URL:\s+(http:\/\/127\.0\.0\.1:\d+\/)/g)].map((match) => match[1]!);
    expect(await (await fetch(urls()[0]!)).text()).toBe("first");
    await Bun.write(input, "second");
    await until(() => (output.match(/Ready in/g)?.length ?? 0) === 2);
    expect(await (await fetch(urls()[1]!)).text()).toBe("second");
    expect(await Bun.file(join(root, "events.log")).text()).toBe(
      "beforeStart:first\nready:first\nruntime-stop:first\ncleanup:first\nbeforeStart:second\nready:second\n",
    );
    // Generation itself must not start an endless rebuild loop.
    await Bun.sleep(250);
    expect(output.match(/Ready in/g)).toHaveLength(2);
  } finally {
    child.kill("SIGTERM");
    const forced = setTimeout(() => child.kill("SIGKILL"), 6000);
    await child.exited;
    clearTimeout(forced);
    await collected;
    const events = await Bun.file(join(root, "events.log")).text();
    await rm(root, { recursive: true, force: true });
    expect(events.endsWith("runtime-stop:second\ncleanup:second\n")).toBe(true);
  }
}, 20000);
