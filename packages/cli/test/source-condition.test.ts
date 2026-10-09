import { expect, test } from "bun:test";
import { cp, mkdir, mkdtemp, realpath, rm, symlink } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";

const repository = resolve(import.meta.dir, "../../..");
const packages = [
  ["lenso", "core"],
  ["engine", "engine"],
  ["cli", "cli"],
  ["web", "web"],
] as const;

async function run(args: string[], cwd: string, stdin?: string) {
  const env = { ...process.env };
  delete env.BUN_OPTIONS;
  const child = Bun.spawn(args, {
    cwd,
    env,
    stdout: "pipe",
    stderr: "pipe",
    stdin: stdin === undefined ? "ignore" : "pipe",
  });
  if (stdin !== undefined && child.stdin && typeof child.stdin !== "number") {
    child.stdin.write(stdin);
    child.stdin.end();
  }
  const timeout = setTimeout(() => child.kill("SIGKILL"), 20000);
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (code !== 0 || stderr !== "")
      throw Error(`${args.join(" ")} exited ${code}\n${stdout}\n${stderr}`);
    return stdout;
  } finally {
    clearTimeout(timeout);
  }
}

test("lenso-source runs CLI, types, browser clients and both dev children without dist", async () => {
  // Only the four packages' source is mirrored; third-party dependencies use the installed workspace.
  const root = await mkdtemp(join(repository, ".lenso-source-test-"));
  try {
    for (const [directory, name] of packages) {
      const source = join(repository, "packages", directory);
      const destination = join(root, "node_modules/@lenso", name);
      await mkdir(destination, { recursive: true });
      await cp(join(source, "src"), join(destination, "src"), { recursive: true });
      await cp(join(source, "package.json"), join(destination, "package.json"));
      const manifest = await Bun.file(join(source, "package.json")).json();
      for (const dependency of Object.keys({
        ...manifest.dependencies,
        ...manifest.peerDependencies,
        ...(name === "cli" ? { zod: true } : {}),
      })) {
        if (dependency.startsWith("@lenso/")) continue;
        const link = join(destination, "node_modules", dependency);
        await mkdir(dirname(link), { recursive: true });
        await symlink(await realpath(join(source, "node_modules", dependency)), link);
      }
    }
    await symlink(
      await realpath(join(repository, "packages/cli/node_modules/zod")),
      join(root, "node_modules/zod"),
    );
    await Bun.write(join(root, "package.json"), '{"type":"module","private":true}');
    const bin = join(root, "node_modules/@lenso/cli/src/bin.ts");
    await Bun.write(
      join(root, "lenso.config.ts"),
      `import { defineApp, definePlugin } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { z } from "zod";
const input = z.object({name:z.string()});
const greeting = definePlugin({
  id:"greeting",
  setup(context) {
    context.onCleanup(() => Bun.write(import.meta.dir + "/closed", "yes").then(() => {}));
    return {async greet({name}:{name:string}) {return {message:"Hello " + name};}};
  },
});
export const operations = [defineOperation({plugin:greeting,method:"greet",input,description:"Greet",effect:"read"})];
export default defineApp({plugins:[greeting]});`,
    );
    const inspect = await run(
      [
        process.execPath,
        "--conditions=lenso-source",
        bin,
        "inspect",
        "greeting",
        "greet",
        "--root",
        root,
        "--json",
      ],
      root,
    );
    expect(JSON.parse(inspect).data.operations[0].inputSchema.properties.name.type).toBe("string");
    expect(await Bun.file(join(root, "closed")).exists()).toBe(false);
    const call = await run(
      [
        process.execPath,
        "--conditions=lenso-source",
        bin,
        "call",
        "greeting",
        "greet",
        "--root",
        root,
        "--stdin",
        "--json",
      ],
      root,
      '{"name":"Ada"}',
    );
    expect(JSON.parse(call)).toMatchObject({ ok: true, data: { message: "Hello Ada" } });
    expect(await Bun.file(join(root, "closed")).text()).toBe("yes");
    await Bun.write(
      join(root, "resolve.ts"),
      `const entries = ${JSON.stringify(packages.map(([, name]) => `@lenso/${name}`))};
for (const entry of entries) {
  const path = Bun.resolveSync(entry, import.meta.dir);
  if (!path.endsWith("/src/index.ts")) throw Error(path);
  await import(entry);
}
console.log("source");`,
    );
    expect(await run([process.execPath, "--conditions=lenso-source", "resolve.ts"], root)).toBe(
      "source\n",
    );
    await Bun.write(
      join(root, "browser.ts"),
      `export * from "@lenso/core/browser";
export * from "@lenso/web/client";
export * from "@lenso/web/openapi-client";`,
    );
    const browser = await Bun.build({
      entrypoints: [join(root, "browser.ts")],
      conditions: ["lenso-source"],
      target: "browser",
    });
    expect(browser.success).toBe(true);
    expect(browser.logs).toEqual([]);
    expect(await browser.outputs[0]!.text()).not.toMatch(/(?:from|import)\s*["'](?:bun|node:)/);
    await Bun.write(
      join(root, "lenso.engine.ts"),
      `import { defineEngineConfig, defineEnginePlugin } from "@lenso/engine/authoring";
export default defineEngineConfig({plugins:[defineEnginePlugin({
  name:"condition-check",
  setup(context) {
    if (!Bun.resolveSync("@lenso/core", import.meta.dir).endsWith("/src/index.ts")) throw Error("worker lost conditions");
    context.dev("source-check", async (event) => {
      if (event === "ready") await Bun.write(context.root + "/worker-ready", "source");
    });
  },
})]});`,
    );
    await Bun.write(
      join(root, "src/server.ts"),
      `import { reportDevReady } from "@lenso/engine/dev-ready";
if (!Bun.resolveSync("@lenso/web", import.meta.dir).endsWith("/src/index.ts")) throw Error("runtime lost conditions");
reportDevReady({capabilities:["source"]});
setInterval(() => {}, 1000);`,
    );
    await Bun.write(
      join(root, "dev.ts"),
      `import { createDevSupervisor } from "@lenso/engine";
let complete!: () => void;
let fail!: (error:unknown) => void;
const ready = new Promise<void>((resolve,reject) => {complete=resolve;fail=reject;});
const dev = await createDevSupervisor({
  root:import.meta.dir,
  stdout:"ignore",
  stderr:"inherit",
  onEvent(event) {
    if (event.type === "ready") complete();
    if (event.type === "failed") fail(Error(JSON.stringify(event.diagnostic)));
  },
});
const timeout = setTimeout(() => fail(Error("readiness timeout")), 10000);
try {await ready;} finally {clearTimeout(timeout);await dev.close();}
console.log("ready");`,
    );
    for (const conditions of [["--conditions=lenso-source"], ["--conditions", "lenso-source"]]) {
      expect(await run([process.execPath, ...conditions, "dev.ts"], root)).toBe("ready\n");
      expect(await Bun.file(join(root, "worker-ready")).text()).toBe("source");
    }
    await Bun.write(
      join(root, "tsconfig.json"),
      JSON.stringify({
        extends: join(repository, "tsconfig.json"),
        compilerOptions: { customConditions: ["lenso-source"] },
        include: ["*.ts", "src/**/*.ts"],
      }),
    );
    await run(
      [
        process.execPath,
        join(repository, "node_modules/typescript/bin/tsc"),
        "--noEmit",
        "-p",
        "tsconfig.json",
      ],
      root,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}, 60000);
