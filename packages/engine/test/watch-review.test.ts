import { expect, test } from "bun:test";
import { mkdir, mkdtemp, rename, rm, symlink } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createDevSupervisor, type DevSupervisorEvent } from "../src/dev";

async function fixture(layout = "app") {
  const parent = await mkdtemp(join(tmpdir(), "lenso-watch-review-"));
  const root = join(parent, layout);
  await mkdir(root, { recursive: true });
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]}");
  return { parent, root };
}

async function server(root: string, imports = "", value = "'first'") {
  await Bun.write(
    join(root, "src/server.ts"),
    `
    ${imports}
    const timer=setInterval(()=>{},1000);
    process.send?.({type:'lenso:dev-ready',capabilities:[${value}]});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});`,
  );
}

async function start(root: string) {
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
  return { supervisor, events, ready };
}

async function until(check: () => boolean) {
  const deadline = Date.now() + 4000;
  while (!check()) {
    if (Date.now() > deadline) throw new Error("Reviewed watch input did not invalidate");
    await Bun.sleep(20);
  }
}

test("an application beneath an ancestor named dist watches its source but excludes its own generated outputs", async () => {
  const { parent, root } = await fixture("fixtures/dist/app");
  await server(root);
  const { supervisor, ready } = await start(root);
  try {
    await until(() => ready().length === 1);
    await server(root, "", "'second'");
    await until(() => ready().length === 2);
    expect(ready().map((event) => event.capabilities)).toEqual([["first"], ["second"]]);
    await Bun.write(join(root, "dist/unrelated.ts"), "export {}");
    await Bun.sleep(300);
    expect(ready()).toHaveLength(2);
  } finally {
    await supervisor.close();
    await rm(parent, { recursive: true, force: true });
  }
});

test("missing publication dist entry creation repairs a package import without touching its manifest", async () => {
  const { parent, root } = await fixture();
  await Bun.write(
    join(root, "node_modules/incomplete/package.json"),
    '{"name":"incomplete","main":"./dist/index.js"}',
  );
  await Bun.write(
    join(root, "lenso.config.ts"),
    "import 'incomplete'; export default {plugins:[]}",
  );
  await server(root);
  const { supervisor, ready, events } = await start(root);
  try {
    expect(events.some((event) => event.type === "failed")).toBe(true);
    await Bun.write(join(root, "node_modules/incomplete/dist/index.js"), "export {}");
    await until(() => ready().length === 1);
    expect(ready()[0]!.capabilities).toEqual(["first"]);
  } finally {
    await supervisor.close();
    await rm(parent, { recursive: true, force: true });
  }
});

test("a deliberate local package link inside another node_modules tree follows imported JSON and local source additions", async () => {
  const { parent, root } = await fixture();
  const local = join(parent, "other-project/node_modules/local-project");
  await Bun.write(join(local, "package.json"), '{"name":"linked","exports":"./entry.ts"}');
  await Bun.write(join(local, "entry.ts"), "export {default} from './value.json'");
  await Bun.write(join(local, "value.json"), '"first"');
  await mkdir(join(root, "node_modules"));
  await symlink(local, join(root, "node_modules/linked"));
  await server(root, "import value from 'linked';", "value");
  const { supervisor, ready } = await start(root);
  try {
    await until(() => ready().length === 1);
    await Bun.write(join(local, "value.json"), '"second"');
    await until(() => ready().length === 2);
    expect(ready()[1]!.capabilities).toEqual(["second"]);
    await Bun.write(join(local, "neighbor.ts"), "export {}");
    await until(() => ready().length === 3);
    await rm(join(local, "neighbor.ts"));
    await until(() => ready().length === 4);
  } finally {
    await supervisor.close();
    await rm(parent, { recursive: true, force: true });
  }
});

for (const kind of ["module", "directory", "file", "alias", "export"]) {
  test(`atomic ${kind} symlink redirects invalidate the lexical route`, async () => {
    const { parent, root } = await fixture();
    const first = join(parent, "first");
    const second = join(parent, "second");
    for (const [directory, value] of [
      [first, "first"],
      [second, "second"],
    ] as const) {
      await Bun.write(join(directory, "entry.ts"), `export default ${JSON.stringify(value)}`);
      await Bun.write(join(directory, "value.json"), JSON.stringify(value));
    }
    const fileLink = kind === "file" || kind === "alias" || kind === "export";
    const link =
      kind === "export"
        ? join(root, "node_modules/linked/entry.ts")
        : join(root, fileLink ? "linked.ts" : "linked");
    if (kind === "export")
      await Bun.write(
        join(root, "node_modules/linked/package.json"),
        '{"name":"linked","exports":"./entry.ts"}',
      );
    await symlink(fileLink ? join(first, "entry.ts") : first, link);
    if (kind === "module") await server(root, "import value from '../linked/entry';", "value");
    else if (kind === "file") {
      await Bun.write(join(root, "linked.json"), "{}");
      await server(root, "import value from '../linked';", "value");
    } else if (kind === "alias") {
      await Bun.write(
        join(root, "tsconfig.json"),
        '{"compilerOptions":{"baseUrl":".","paths":{"@linked":["./linked"]}}}',
      );
      await server(root, "import value from '@linked';", "value");
    } else if (kind === "export") {
      await server(root, "import value from 'linked';", "value");
    } else {
      await Bun.write(
        join(root, "lenso.engine.ts"),
        `export default {plugins:[{name:'linked',setup(c){c.watch('linked')}}]}`,
      );
      await server(
        root,
        "const value=await Bun.file(import.meta.dir+'/../linked/value.json').json();",
        "value",
      );
    }
    const { supervisor, ready } = await start(root);
    try {
      await until(() => ready().length === 1);
      if (kind === "file") {
        await Bun.write(join(root, "linked.json"), '{"business":"data"}');
        await Bun.sleep(250);
        expect(ready()).toHaveLength(1);
      }
      await symlink(fileLink ? join(second, "entry.ts") : second, join(root, "linked-next"));
      await rename(join(root, "linked-next"), link);
      await until(() => ready().length === 2);
      expect(ready().map((event) => event.capabilities)).toEqual([["first"], ["second"]]);
      await Bun.write(
        join(first, kind === "directory" ? "value.json" : "entry.ts"),
        kind === "directory" ? '"obsolete"' : "export default 'obsolete'",
      );
      await Bun.sleep(300);
      expect(ready()).toHaveLength(2);
    } finally {
      await supervisor.close();
      await rm(parent, { recursive: true, force: true });
    }
  }, 15000);
}

test("ready hooks publish new explicit watches with their current reference ownership", async () => {
  const { parent, root } = await fixture();
  await Bun.write(join(root, "business.json"), "{}");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    export default {plugins:[{name:'ready-inputs',setup(c){
      c.dev('watch',event=>{if(event==='ready'){
        const first=c.watch('business.json'); c.watch('business.json'); first();
      }});
    }}]}`,
  );
  await server(root);
  const { supervisor, ready } = await start(root);
  try {
    await until(() => ready().length === 1);
    await Bun.write(join(root, "business.json"), '{"changed":true}');
    await until(() => ready().length === 2);
  } finally {
    await supervisor.close();
    await rm(parent, { recursive: true, force: true });
  }
});

test("ready hooks replace obsolete watch snapshots rather than permanently union revoked inputs", async () => {
  const { parent, root } = await fixture();
  await Bun.write(join(root, "revoked.json"), "{}");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    export default {plugins:[{name:'ready-revocation',setup(c){
      const first=c.watch('revoked.json');const second=c.watch('revoked.json');
      c.dev('revoke',event=>{if(event==='ready'){first();second();}});
    }}]}`,
  );
  await server(root);
  const { supervisor, ready } = await start(root);
  try {
    await until(() => ready().length === 1);
    await Bun.write(join(root, "revoked.json"), '{"changed":true}');
    await Bun.sleep(500);
    expect(ready()).toHaveLength(1);
  } finally {
    await supervisor.close();
    await rm(parent, { recursive: true, force: true });
  }
});
