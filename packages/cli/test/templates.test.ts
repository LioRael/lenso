import { expect, test } from "bun:test";
import { cp, mkdtemp, mkdir, realpath, rename, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const repository = resolve(import.meta.dir, "../../..");

async function run(cwd: string, args: string[], extraEnv: Record<string, string> = {}) {
  const env = { ...process.env, ...extraEnv };
  delete env.NODE_PATH;
  delete env.BUN_OPTIONS;
  const child = Bun.spawn([process.execPath, ...args], {
    cwd,
    env,
    stdin: "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  const timeout = setTimeout(() => child.kill("SIGKILL"), 90000);
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (code !== 0) throw new Error(`${args.join(" ")} exited ${code}\n${stdout}\n${stderr}`);
    return stdout;
  } finally {
    clearTimeout(timeout);
  }
}

// Uses built dist and real tarballs, never workspace source aliases.
test("templates install externally, retain client types and own their listener lifecycle", async () => {
  const temporary = await realpath(await mkdtemp(join(tmpdir(), "lenso-templates-")));
  try {
    expect(temporary.startsWith((await realpath(repository)) + "/")).toBe(false);
    const vendor = join(temporary, "vendor");
    await mkdir(vendor);
    for (const [directory, name] of [
      ["lenso", "core"],
      ["engine", "engine"],
      ["cli", "cli"],
      ["web", "web"],
    ] as const) {
      await run(join(repository, "packages", directory), [
        "pm",
        "pack",
        "--ignore-scripts",
        "--filename",
        join(vendor, `lenso-${name}.tgz`),
      ]);
    }
    for (const template of ["cli", "bun-web", "workspace"]) {
      const consumer = join(temporary, template);
      await cp(join(repository, "templates", template), consumer, { recursive: true });
      await cp(vendor, join(consumer, "vendor"), { recursive: true });
      await run(consumer, ["install", "--ignore-scripts"]);
      expect(await Bun.file(join(consumer, "bun.lock")).exists()).toBe(true);
      await run(consumer, ["run", "typecheck"]);
      const selection = template === "workspace" ? ["--app", "apps/server"] : [];
      const inspect = JSON.parse(
        await run(consumer, [
          "run",
          "lenso",
          "inspect",
          "greeting",
          "greet",
          ...selection,
          "--json",
        ]),
      );
      expect(inspect.ok).toBe(true);
      const call = JSON.parse(
        await run(
          consumer,
          ["run", "lenso", "call", "greeting", "greet", ...selection, "--json", '{"name":"Ada"}'],
          { GREETING_PREFIX: "Welcome" },
        ),
      );
      expect(call).toMatchObject({ ok: true, data: { message: "Welcome, Ada!" } });
      if (template === "cli") continue;

      const server = template === "workspace" ? "./runtime/main" : "./src/server";
      const loopbackFile = template === "workspace" ? "apps/server/loopback.ts" : "loopback.ts";
      const clientImport =
        template === "workspace"
          ? `import {createAppClient} from '../web/src/client';`
          : `import {createClient} from '@lenso/web/client';
           const createAppClient=(url:string)=>createClient<AppRouter>(url);`;
      const typesImport =
        template === "workspace"
          ? `import type {AppClient,AppRouter} from '@app/server/client';`
          : `import type {AppClient,AppRouter} from './src/server';`;
      const assertions = `
        ${template === "workspace" ? typesImport : `import type {AppClient,AppRouter} from './server';`}
        type IsAny<T> = 0 extends (1 & T) ? true : false;
        type AssertFalse<T extends false> = T;
        type ClientNotAny = AssertFalse<IsAny<AppClient>>;
        type InputNotAny = AssertFalse<IsAny<Parameters<AppClient['greet']>[0]>>;
        type OutputNotAny = AssertFalse<IsAny<Awaited<ReturnType<AppClient['greet']>>>>;
        type MessageNotAny = AssertFalse<IsAny<Awaited<ReturnType<AppClient['greet']>>['message']>>;
        type CountNotAny = AssertFalse<IsAny<Awaited<ReturnType<AppClient['greet']>>['count']>>;
        function check(client:AppClient) {
          const result:Promise<{message:string;count:number}>=client.greet({name:'Ada'});
          // @ts-expect-error A name is required.
          client.greet({});
          // @ts-expect-error Names are strings.
          client.greet({name:42});
          // @ts-expect-error Undeclared routes do not exist.
          client.hidden();
          // @ts-expect-error The inferred output is not an arbitrary scalar.
          const wrong:Promise<string>=client.greet({name:'Ada'});
          return result;
        }
      `;
      const assertionFile =
        template === "workspace" ? "apps/web/src/types-test.ts" : "src/types-test.ts";
      await Bun.write(join(consumer, assertionFile), assertions);
      await run(consumer, ["run", "typecheck"]);
      await Bun.write(
        join(consumer, loopbackFile),
        `
        import assert from 'node:assert/strict';
        import {startServer} from '${server}';
        import {createGreetingService} from '${template === "workspace" ? "@app/greeting" : "./src/greeting"}';
        ${typesImport}
        ${clientImport}
        assert.deepEqual(await createGreetingService('Plain').greet({name:'Ada'}),
          {message:'Plain, Ada!',count:1});
        process.env.GREETING_PREFIX='First';
        const {app,url}=await startServer(0);
        try {
          process.env.GREETING_PREFIX='Changed';
          const client:AppClient=createAppClient(new URL('rpc',url).href);
          assert.deepEqual(await client.greet({name:' Ada '}),{message:'First, Ada!',count:1});
          assert.deepEqual(await client.greet({name:'Ada'}),{message:'First, Ada!',count:2});
          await assert.rejects(()=>client.greet({name:'A'}));
        } finally {
          await app.stop();
          await app.stop();
        }
        await assert.rejects(()=>fetch(url));
      `,
      );
      await run(consumer, [loopbackFile]);
      await run(consumer, ["run", "lenso", "generate", ...selection]);
      for (const file of ["manifest.json", "server.ts", "client.ts"]) {
        expect(
          await Bun.file(
            join(consumer, template === "workspace" ? "apps/server" : "", ".lenso", file),
          ).exists(),
        ).toBe(true);
      }
      await run(consumer, ["run", "build"]);
      if (template === "workspace") {
        for (const directory of ["apps/server", "apps/web", "plugins/greeting"]) {
          expect(await Bun.file(join(consumer, directory, "bun.lock")).exists()).toBe(false);
        }
        const bundle = await Bun.file(join(consumer, "apps/web/dist/client.js")).text();
        for (const forbidden of ["GREETING_PREFIX", "startServer", "Bun.serve", "lenso/defaults"]) {
          expect(bundle).not.toContain(forbidden);
        }
        await run(join(consumer, "apps/web"), [
          "-e",
          `
          import assert from 'node:assert/strict';
          assert.throws(()=>Bun.resolveSync('@app/server/client',process.cwd()));
        `,
        ]);
        await mkdir(join(consumer, "shared", "capabilities"), { recursive: true });
        await rename(
          join(consumer, "plugins/greeting"),
          join(consumer, "shared/capabilities/greeting"),
        );
        const manifest = await Bun.file(join(consumer, "package.json")).json();
        manifest.workspaces = ["apps/*", "shared/capabilities/*"];
        await Bun.write(join(consumer, "package.json"), JSON.stringify(manifest));
        const pluginManifestPath = join(consumer, "shared/capabilities/greeting/package.json");
        const pluginManifest = await Bun.file(pluginManifestPath).json();
        pluginManifest.dependencies["@lenso/core"] = "file:../../../vendor/lenso-core.tgz";
        await Bun.write(pluginManifestPath, JSON.stringify(pluginManifest));
        await run(consumer, ["install", "--ignore-scripts"]);
        await run(consumer, ["run", "typecheck"]);
        await run(consumer, [loopbackFile]);
        await run(consumer, ["run", "call", "--", '{"name":"Ada"}']);
      }
    }
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}, 300000);
