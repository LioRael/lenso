import { expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createDevSupervisor } from "@lenso/engine";
import { connectDev } from "../src/dev-connection";
import { call } from "../src/engine";

test("ready supervisor connection borrows one app, fences old generations and cleans owned resources", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-connection-test-"));
  const events: unknown[] = [];
  let supervisor: Awaited<ReturnType<typeof createDevSupervisor>> | undefined;
  try {
    await mkdir(join(root, "node_modules/@lenso"), { recursive: true });
    for (const [name, directory] of [
      ["core", "lenso"],
      ["engine", "engine"],
      ["cli", "cli"],
    ])
      await symlink(
        resolve(import.meta.dir, `../../${directory}`),
        join(root, "node_modules/@lenso", name!),
      );
    await symlink(resolve(import.meta.dir, "../node_modules/zod"), join(root, "node_modules/zod"));
    await writeFile(join(root, "package.json"), JSON.stringify({ type: "module" }));
    const source = `import {definePlugin} from '@lenso/core';
import {appendFileSync} from 'node:fs';
export const plugin=definePlugin({id:'counter',setup(){appendFileSync(${JSON.stringify(join(root, "setups"))},'setup\\n');let count=0;return {async slow(_input,{signal}){appendFileSync(${JSON.stringify(join(root, "slow-started"))},'started');await new Promise((resolve,reject)=>{const timer=setTimeout(resolve,2000);signal.addEventListener('abort',()=>{clearTimeout(timer);reject(signal.reason)},{once:true})});return {message:'done',count:0}},run({name}){return {message:'Hello '+name,count:++count}}}}});`;
    await writeFile(join(root, "service.ts"), source);
    await writeFile(
      join(root, "lenso.config.ts"),
      `import {defineApp} from '@lenso/core';import {defineOperation} from '@lenso/cli';import {z} from 'zod';import {plugin} from './service';export const operations=[defineOperation({plugin,method:'run',description:'Run',input:z.object({name:z.string()}),output:z.object({message:z.string(),count:z.number()})}),defineOperation({plugin,method:'slow',context:true,description:'Slow',input:z.object({name:z.string()})})];export default defineApp({plugins:[plugin]});`,
    );
    await writeFile(
      join(root, "entry.ts"),
      `import {startApp} from '@lenso/core';import {serveDevOperations} from '@lenso/cli';import {reportDevReady} from '@lenso/engine/dev-ready';import app,{operations} from './lenso.config';const running=await startApp(app);const host=await serveDevOperations({running,plugins:app.plugins,operations,binding:(_operation,_input,_running,{signal})=>({context:{signal}})});reportDevReady({connection:host?.connection});process.once('SIGTERM',async()=>{await host?.close();await running.stop();process.exit(0)});`,
    );
    const descriptor = join(root, ".lenso/dev.json");
    const wait = async (predicate: () => boolean | Promise<boolean>) => {
      const until = Date.now() + 12000;
      while (Date.now() < until) {
        if (await predicate()) return;
        await Bun.sleep(20);
      }
      throw new Error(JSON.stringify(events));
    };
    supervisor = await createDevSupervisor({
      root,
      entry: "entry.ts",
      connectionFile: descriptor,
      onEvent: (event) => events.push(event),
      stdout: "ignore",
      stderr: "ignore",
    });
    await wait(async () => {
      try {
        return JSON.parse(await readFile(descriptor, "utf8")).generation;
      } catch {
        return false;
      }
    });
    const inspected = await connectDev(descriptor, "inspect");
    expect(inspected).toMatchObject({
      inspection: "running",
      capabilities: { connectedCall: true, recentCalls: true },
    });
    const generation = inspected.generation;
    for (let count = 1; count <= 3; count++)
      expect(
        await connectDev(descriptor, "call", {
          generation,
          pluginId: "counter",
          method: "run",
          input: { name: "Ada" },
        }),
      ).toEqual({ message: "Hello Ada", count });
    expect((await readFile(join(root, "setups"), "utf8")).trim().split("\n")).toHaveLength(1);
    await expect(
      connectDev(descriptor, "call", {
        pluginId: "counter",
        method: "run",
        input: { name: "Ada" },
      }),
    ).rejects.toMatchObject({
      diagnostic: { code: "generation-required", effectState: "not-started" },
    });
    for (let n = 0; n < 2; n++)
      expect(await call(root, "counter", "run", { name: "Ada" })).toEqual({
        message: "Hello Ada",
        count: 1,
      });
    expect((await readFile(join(root, "setups"), "utf8")).trim().split("\n")).toHaveLength(3);
    const recent = await connectDev(descriptor, "recent", { generation, limit: 64 });
    expect(recent.calls).toHaveLength(3);
    expect(JSON.stringify(recent)).not.toContain("Ada");
    expect(recent.retention).toMatchObject({ maxEntries: 64, payloads: false, durable: false });
    for (let n = 0; n < 65; n++)
      await connectDev(descriptor, "call", {
        generation,
        pluginId: "counter",
        method: "run",
        input: { name: "never-retain-this-payload" },
      });
    const bounded = await connectDev(descriptor, "recent", { generation, limit: 64 });
    expect(bounded.calls).toHaveLength(64);
    expect(JSON.stringify(bounded)).not.toContain("never-retain-this-payload");
    const controller = new AbortController();
    const interrupted = connectDev(descriptor, "call", {
      generation,
      pluginId: "counter",
      method: "slow",
      input: { name: "Ada" },
      signal: controller.signal,
    }).catch((error) => error);
    await wait(() => Bun.file(join(root, "slow-started")).exists());
    await expect(
      connectDev(descriptor, "call", {
        generation,
        pluginId: "counter",
        method: "run",
        input: { name: "Ada" },
      }),
    ).rejects.toMatchObject({ diagnostic: { code: "adapter-busy", effectState: "not-started" } });
    controller.abort();
    expect(await interrupted).toMatchObject({
      diagnostic: { code: "dev-connection-lost", effectState: "unknown" },
    });
    await wait(async () => {
      try {
        return (await connectDev(descriptor, "recent", { generation, limit: 64 })).calls.some(
          (row: { method: string; code: string; effectState: string }) =>
            row.method === "slow" && row.code === "aborted" && row.effectState === "unknown",
        );
      } catch {
        return false;
      }
    });
    await writeFile(
      join(root, "service.ts"),
      source.replace("'Hello '+name", "'Hello '+name.toUpperCase()"),
    );
    await wait(async () => {
      try {
        return JSON.parse(await readFile(descriptor, "utf8")).generation !== generation;
      } catch {
        return false;
      }
    });
    await expect(
      connectDev(descriptor, "call", {
        generation,
        pluginId: "counter",
        method: "run",
        input: { name: "Ada" },
      }),
    ).rejects.toMatchObject({
      diagnostic: { code: "stale-generation", effectState: "not-started" },
    });
    const current = await connectDev(descriptor, "inspect");
    expect(
      await connectDev(descriptor, "call", {
        generation: current.generation,
        pluginId: "counter",
        method: "run",
        input: { name: "Ada" },
      }),
    ).toEqual({ message: "Hello ADA", count: 1 });
    const socket = JSON.parse(await readFile(descriptor, "utf8")).socket;
    await supervisor.close();
    supervisor = undefined;
    expect(await Bun.file(descriptor).exists()).toBe(false);
    expect(await Bun.file(socket).exists()).toBe(false);
  } finally {
    await supervisor?.close();
    await rm(root, { recursive: true, force: true });
  }
}, 30000);
