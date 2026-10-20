import { afterEach, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, unlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createEngineSession } from "../src/engine";
import { diagnostic } from "../src/diagnostics";

const roots: string[] = [];
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});
async function fixture(source = "export default {plugins:[]};") {
  const root = await mkdtemp(join(tmpdir(), "lenso-output-lease-"));
  roots.push(root);
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]};");
  await Bun.write(join(root, "lenso.engine.ts"), source);
  return root;
}
async function failure(promise: Promise<unknown>, code: string) {
  const cause = await promise.then(
    () => undefined,
    (rejected: unknown) => rejected,
  );
  expect(diagnostic(cause).code).toBe(code);
}

test("dev/build claim before extension setup; other roots and released claims work", async () => {
  const root = await fixture(`import {appendFileSync} from 'node:fs';
    export default {plugins:[{name:'setup',setup(c){appendFileSync(c.root+'/setups','setup\\n')}}]};`);
  const owner = createEngineSession(root, "dev");
  const blocked = createEngineSession(root, "build");
  const other = createEngineSession(await fixture(), "dev");
  try {
    await owner.prepare();
    await owner.session.generate();
    const before = await readFile(join(root, ".lenso/.engine-files.json"), "utf8");
    await failure(blocked.prepare(), "engine-output-in-use");
    await blocked.session.close();
    expect(await readFile(join(root, "setups"), "utf8")).toBe("setup\n");
    expect(await readFile(join(root, ".lenso/.engine-files.json"), "utf8")).toBe(before);
    await other.prepare();
    await owner.session.close();
    const next = createEngineSession(root, "build");
    try {
      await next.prepare();
      await owner.session.close();
      expect(await Bun.file(join(root, ".lenso/.engine-output.lock")).exists()).toBe(true);
      await next.session.generate();
    } finally {
      await next.session.close();
    }
  } finally {
    await Promise.all([owner.session.close(), blocked.session.close(), other.session.close()]);
  }
});

test("cleanup holds ownership and a cleanup failure still releases it", async () => {
  const root = await fixture(`export default {plugins:[{name:'cleanup',setup(c){
    c.onCleanup(async()=>{await Bun.write(c.root+'/cleaning','yes');
      while(!(await Bun.file(c.root+'/release').exists()))await Bun.sleep(5);
      throw Error('fixture cleanup failure');});}}]};`);
  const owner = createEngineSession(root, "build");
  await owner.prepare();
  const closing = owner.session.close();
  const end = Date.now() + 2000;
  while (!(await Bun.file(join(root, "cleaning")).exists())) {
    if (Date.now() > end) throw Error("cleanup deadline");
    await Bun.sleep(5);
  }
  const blocked = createEngineSession(root, "dev");
  await failure(blocked.prepare(), "engine-output-in-use");
  await blocked.session.close();
  await Bun.write(join(root, "release"), "yes");
  await failure(closing, "engine-cleanup-failed");
  expect(await Bun.file(join(root, ".lenso/.engine-output.lock")).exists()).toBe(false);
});

test("a killed owner leaves a conservative claim, never automatically stolen", async () => {
  const root = await fixture();
  const engine = resolve(import.meta.dir, "../src/engine.ts");
  const child = Bun.spawn(
    [
      process.execPath,
      "-e",
      `import {createEngineSession} from ${JSON.stringify(engine)};
    const e=createEngineSession(${JSON.stringify(root)},'build');await e.prepare();console.log('claimed');setInterval(()=>{},1000);`,
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  try {
    const reader = child.stdout.getReader();
    const first = await reader.read();
    expect(new TextDecoder().decode(first.value)).toContain("claimed");
    await reader.cancel();
    child.kill("SIGKILL");
    await child.exited;
    const path = join(root, ".lenso/.engine-output.lock");
    const before = await readFile(path, "utf8");
    const next = createEngineSession(root, "dev");
    await failure(next.prepare(), "engine-output-in-use");
    await next.session.close();
    expect(await readFile(path, "utf8")).toBe(before);
  } finally {
    if (child.exitCode === null) child.kill("SIGKILL");
    await child.exited;
  }
});

test("close preserves a replaced claim instead of removing another owner", async () => {
  const root = await fixture();
  const owner = createEngineSession(root, "dev");
  await owner.prepare();
  const path = join(root, ".lenso/.engine-output.lock");
  await unlink(path);
  await Bun.write(path, "replacement owner");
  await failure(owner.session.close(), "engine-cleanup-failed");
  expect(await readFile(path, "utf8")).toBe("replacement owner");
});
