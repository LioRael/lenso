import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { call, inspect } from "../src/engine";
import { type ApplicationTarget, createEngineSession, withEngine } from "@lenso/engine";

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "lenso-target-"));
  directories.push(root);
  return root;
}
async function run(root: string, args: string[], built = false) {
  const bin = resolve(import.meta.dir, built ? "../dist/bin.js" : "../src/bin.ts");
  const child = Bun.spawn([process.execPath, bin, ...args, "--root", root, "--json"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  return { result: JSON.parse(out), err, code };
}
async function app(root: string, id: string, config = "lenso.config.ts") {
  await Bun.write(
    join(root, config),
    `
    import {appendFileSync} from 'node:fs';
    appendFileSync(${JSON.stringify(join(root, "imports.log"))},'${id}\\n');
    const plugin={id:'${id}',setup({onCleanup}){
      appendFileSync(${JSON.stringify(join(root, "setup.log"))},'setup\\n');
      onCleanup(()=>appendFileSync(${JSON.stringify(join(root, "setup.log"))},'cleanup\\n'));
      return {read:async()=>({application:'${id}'})};
    }};
    export const operations=[{plugin,method:'read',description:'Read selected app',
      input:{'~standard':{version:1,vendor:'test',validate:value=>({value})}}}];
    export default {plugins:[plugin]};`,
  );
}

test("explicit noncanonical config is shared by inspect/call/check/generate/build and sessions", async () => {
  const root = await fixture();
  await mkdir(join(root, "config"));
  await mkdir(join(root, "runtime"));
  await app(root, "selected", "config/application.ts");
  await app(root, "canonical");
  await Bun.write(join(root, "runtime/main.ts"), "export const selected = true;");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    import {appendFileSync} from 'node:fs';
    appendFileSync(${JSON.stringify(join(root, "engine.log"))},'import\\n');
    export default {plugins:[{name:'entry',setup(c){
      c.onCleanup(()=>appendFileSync(${JSON.stringify(join(root, "engine.log"))},'cleanup\\n'));
      c.convention(()=>({config:'missing.ts',entry:'runtime/main.ts'}),{replace:'lenso/defaults'});
    }}]};`,
  );
  const target: ApplicationTarget = { root, config: "config/application.ts" };
  expect((await inspect(target)).plugins.map((plugin) => plugin.id)).toEqual(["selected"]);
  expect(await Bun.file(join(root, "engine.log")).exists()).toBe(false);
  expect(await call(target, "selected", "read", {})).toEqual({ application: "selected" });
  expect(await Bun.file(join(root, "engine.log")).exists()).toBe(false);
  expect(await Bun.file(join(root, "setup.log")).text()).toBe("setup\ncleanup\n");
  const engine = createEngineSession(target, "check");
  await withEngine(engine.session, async () => {
    expect((await engine.prepare()).configPath).toBe(join(root, target.config!));
    expect(engine.session.snapshot().convention.config).toBe(target.config!);
  });
  await rm(join(root, "lenso.config.ts"));
  for (const built of [false, true]) {
    for (const command of ["inspect", "call", "check", "generate", "build"]) {
      const args = command === "call" ? [command, "selected", "read"] : [command];
      const result = await run(root, [...args, "--config", target.config!], built);
      expect(result.code).toBe(0);
      expect(result.err).toBe("");
      expect(result.result.ok).toBe(true);
      if (["inspect", "check"].includes(command))
        expect(result.result.data.configPath).toBe(join(root, target.config!));
    }
  }
  const manifest = await Bun.file(join(root, ".lenso/manifest.json")).json();
  expect(manifest.source).toBe(target.config!);
  expect(manifest.plugins.map((plugin: { id: string }) => plugin.id)).toEqual(["selected"]);
  expect(await Bun.file(join(root, ".lenso/server.ts")).text()).toContain("../config/application");
  expect(await Bun.file(join(root, "dist/main.js")).exists()).toBe(true);
  expect(await Bun.file(join(root, "imports.log")).text()).not.toContain("canonical");
});

test("workspace candidates are inert and explicit app selects exactly one root", async () => {
  const root = await fixture();
  await Bun.write(
    join(root, "package.json"),
    JSON.stringify({ workspaces: ["apps/first", "apps/second"] }),
  );
  await Bun.write(join(root, "lenso.engine.ts"), "throw Error('root Engine must not import');");
  for (const id of ["first", "second"]) {
    const directory = join(root, "apps", id);
    await mkdir(directory, { recursive: true });
    await app(directory, id);
    await Bun.write(
      join(directory, "lenso.engine.ts"),
      `
      await Bun.write(${JSON.stringify(join(directory, "engine-imported"))},'yes');
      export default {};`,
    );
  }
  for (const args of [["inspect"], ["check"], ["generate"], ["build"], ["call", "first", "read"]]) {
    const result = await run(root, args);
    expect(result.code).toBe(3);
    expect(result.result.error).toMatchObject({
      code: "ambiguous-application-target",
      details: { candidates: ["apps/first", "apps/second"] },
    });
    expect(result.result.error.message).toContain("--app");
  }
  for (const id of ["first", "second"]) {
    const directory = join(root, "apps", id);
    expect(await Bun.file(join(directory, "imports.log")).exists()).toBe(false);
    expect(await Bun.file(join(directory, "engine-imported")).exists()).toBe(false);
    expect(await Bun.file(join(directory, "setup.log")).exists()).toBe(false);
  }
  const selected = ["--app", "apps/second"];
  expect((await run(root, ["inspect", ...selected])).result.data.plugins[0].id).toBe("second");
  expect((await run(root, ["call", "second", "read", ...selected])).result.data).toEqual({
    application: "second",
  });
  expect(await Bun.file(join(root, "apps/second/engine-imported")).exists()).toBe(false);
  expect((await run(root, ["check", ...selected])).result.data.order).toEqual(["second"]);
  expect(await Bun.file(join(root, "apps/second/engine-imported")).exists()).toBe(true);
  expect(await Bun.file(join(root, "apps/first/imports.log")).exists()).toBe(false);
  expect(await Bun.file(join(root, "apps/first/setup.log")).exists()).toBe(false);
  expect(await Bun.file(join(root, "apps/second/setup.log")).text()).toBe("setup\ncleanup\n");
});

test("missing selection, exact workspace declarations, help, and option diagnostics", async () => {
  const root = await fixture();
  await mkdir(join(root, "apps/only"), { recursive: true });
  await mkdir(join(root, "unlisted"), { recursive: true });
  await app(join(root, "apps/only"), "only");
  await app(join(root, "unlisted"), "unlisted");
  await Bun.write(
    join(root, "package.json"),
    JSON.stringify({ workspaces: { packages: ["apps/*"] } }),
  );
  const single = await run(root, ["inspect"]);
  expect(single.result.error).toMatchObject({
    code: "missing-application-selection",
    details: { candidates: ["apps/only"] },
  });
  await Bun.write(
    join(root, "package.json"),
    JSON.stringify({ workspaces: ["apps/*", "!apps/only"] }),
  );
  expect((await run(root, ["inspect"])).result.error.details.candidates).toEqual([]);
  const help = await run(root, ["help"]);
  expect(help.code).toBe(0);
  expect((await run(root, ["inspect", "--help"])).result.data).toEqual(help.result.data);
  expect(
    help.result.data.commands.find((command: { name: string }) => command.name === "inspect").usage,
  ).toContain("--config");
  expect(help.result.data.errorCodes).toContain("ambiguous-application-target");
  for (const args of [
    ["inspect", "--config"],
    ["inspect", "--app", "apps/only", "--app", "apps/only"],
    ["inspect", "--config", "app.ts", "--config", "other.ts"],
  ]) {
    expect((await run(root, args)).result.error.code).toBe("invalid-arguments");
  }
  expect((await run(root, ["inspect", "--app", "../outside"])).result.error.code).toBe(
    "invalid-application-target",
  );
  expect(
    (await run(root, ["inspect", "--app", "apps/only", "--config", "../../outside.ts"])).result
      .error.code,
  ).toBe("invalid-application-target");
  expect(await Bun.file(join(root, "apps/only/imports.log")).exists()).toBe(false);
  expect(await Bun.file(join(root, "unlisted/imports.log")).exists()).toBe(false);
});

test("CLI dev passes selected root/config and entry through the built Engine worker", async () => {
  const root = await fixture();
  const selected = join(root, "apps/selected");
  const other = join(root, "apps/other");
  await mkdir(join(selected, "runtime"), { recursive: true });
  await mkdir(other, { recursive: true });
  await app(selected, "selected", "app.ts");
  await app(other, "other");
  await Bun.write(join(root, "package.json"), JSON.stringify({ workspaces: ["apps/*"] }));
  await Bun.write(
    join(selected, "lenso.engine.ts"),
    `
    export default {plugins:[{name:'entry',setup(c){
      c.convention(()=>({config:'missing.ts',entry:'runtime/main.ts'}),{replace:'lenso/defaults'});
      c.dev('ready',event=>event==='ready'?Bun.write(c.root+'/ready','yes'):undefined);
      c.onCleanup(()=>Bun.write(c.root+'/closed','yes'));
    }}]};`,
  );
  const core = Bun.resolveSync("@lenso/core", import.meta.dir);
  for (const name of ["main", "override"]) {
    await Bun.write(
      join(selected, `runtime/${name}.ts`),
      `
      import {startApp} from ${JSON.stringify(core)};
      import {app} from '../.lenso/server';
      const running=await startApp(app);
      await Bun.write(${JSON.stringify(join(selected, "entry"))},'${name}');
      const timer=setInterval(()=>{},1000);
      process.on('SIGTERM',async()=>{clearInterval(timer);await running.stop();process.disconnect?.()});
      process.send?.({type:'lenso:dev-ready'});`,
    );
  }
  for (const [built, entry] of [
    [false, undefined],
    [true, "runtime/override.ts"],
  ] as const) {
    const bin = resolve(import.meta.dir, built ? "../dist/bin.js" : "../src/bin.ts");
    const child = Bun.spawn(
      [
        process.execPath,
        bin,
        "dev",
        "--root",
        root,
        "--app",
        "apps/selected",
        "--config",
        "app.ts",
        ...(entry ? ["--entry", entry] : []),
      ],
      { stdout: "pipe", stderr: "pipe" },
    );
    const stdout = new Response(child.stdout).text();
    const stderr = new Response(child.stderr).text();
    const timeout = setTimeout(() => child.kill("SIGKILL"), 10000);
    try {
      const deadline = Date.now() + 8000;
      while (!(await Bun.file(join(selected, "ready")).exists())) {
        if (Date.now() > deadline || child.exitCode !== null)
          throw Error("CLI dev did not become ready");
        await Bun.sleep(20);
      }
      expect(await Bun.file(join(selected, "entry")).text()).toBe(entry ? "override" : "main");
      child.kill("SIGTERM");
      expect(await child.exited).toBe(0);
      expect(await Bun.file(join(selected, "closed")).text()).toBe("yes");
      expect(await stdout).toContain("stopped");
      expect(await stderr).not.toContain("failed");
      expect(await Bun.file(join(other, "imports.log")).exists()).toBe(false);
      expect(await Bun.file(join(other, "setup.log")).exists()).toBe(false);
    } finally {
      clearTimeout(timeout);
      if (child.exitCode === null) {
        child.kill("SIGTERM");
        await child.exited;
      }
      await rm(join(selected, "ready"), { force: true });
    }
  }
  expect(await Bun.file(join(selected, "setup.log")).text()).toBe(
    "setup\ncleanup\nsetup\ncleanup\n",
  );
}, 20000);
