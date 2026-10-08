import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { defineApp, definePlugin } from "lenso";
import { z } from "zod";
import { defineOperation } from "../src/operations";
import { diagnostic } from "../src/diagnostics";
import { discover, generate, inspect, invoke } from "../src/engine";

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0))
    await rm(directory, { recursive: true, force: true });
});
async function fixture(config: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "lenso-engine-"));
  directories.push(root);
  await Bun.write(join(root, "lenso.config.ts"), config);
  return root;
}

describe("static assembly", () => {
  test("discovers topological order without initializing plugin business code", async () => {
    const root = await fixture(`
      const dependency = { id: 'dependency', setup() { throw new Error('setup must never run'); } };
      const dependent = { id: 'dependent', requires: [dependency], setup() { throw new Error('setup must never run'); } };
      export default { plugins: [dependent, dependency] };
    `);
    expect((await discover(root)).ordered.map((plugin) => plugin.id)).toEqual([
      "dependency",
      "dependent",
    ]);
  });

  test.each([
    [
      "duplicate identities",
      `export default { plugins: [{ id: 'a', setup() {} }, { id: 'a', setup() {} }] };`,
    ],
    [
      "missing dependencies",
      `const b = { id: 'b', setup() {} }; export default { plugins: [{ id: 'a', requires: [b], setup() {} }] };`,
    ],
    [
      "cycles",
      `const a = { id: 'a', requires: [], setup() {} }; const b = { id: 'b', requires: [a], setup() {} }; a.requires.push(b); export default { plugins: [a,b] };`,
    ],
  ])("rejects %s at discovery time", async (_name, config) => {
    const root = await fixture(config);
    await expect(discover(root)).rejects.toThrow();
    expect(await Bun.file(join(root, ".lenso/manifest.json")).exists()).toBe(false);
  });

  test("generates separate browser client and server entries", async () => {
    const root = await fixture(
      `export default { plugins: [{ id: 'greeting', setup() {}, contributions: [{ kind: 'example.metadata', label: 'Greeting' }] }] };`,
    );
    await mkdir(join(root, "src"));
    await Bun.write(join(root, "src/router.ts"), "export type AppRouter = {};");
    await generate(root);
    const client = await Bun.file(join(root, ".lenso/client.ts")).text();
    expect(client).toContain("from '@lenso/web/client'");
    expect(client).toContain("import type { AppRouter }");
    expect(client).not.toContain("lenso.config");
    expect(client).not.toContain("startApp");
    expect(await Bun.file(join(root, ".lenso/server.ts")).text()).toContain("../lenso.config");
  });
});

describe("direct service invocation", () => {
  test("calls async business methods and always closes acquired resources", async () => {
    const events: string[] = [];
    const greeting = definePlugin({
      id: "greeting",
      setup(context) {
        events.push("start");
        context.onCleanup(() => {
          events.push("close");
        });
        return {
          async greet(input: unknown) {
            return { message: `Hello ${String(input)}!` };
          },
        };
      },
    });
    expect(
      await invoke(
        {
          ...defineApp({ plugins: [greeting] }),
          operations: [
            defineOperation({
              plugin: greeting,
              method: "greet",
              description: "Greet",
              input: z.unknown(),
            }),
          ],
        },
        "greeting",
        "greet",
        "Ada",
      ),
    ).toEqual({
      message: "Hello Ada!",
    });
    expect(events).toEqual(["start", "close"]);
  });

  test("closes resources on business failure and rejects prototype methods", async () => {
    let closed = 0;
    const plugin = definePlugin({
      id: "failure",
      setup(context) {
        context.onCleanup(() => {
          closed++;
        });
        return {
          async run() {
            throw new Error("business failure");
          },
        };
      },
    });
    const app = {
      ...defineApp({ plugins: [plugin] }),
      operations: [
        defineOperation({ plugin, method: "run", description: "Run", input: z.unknown() }),
      ],
    };
    await expect(invoke(app, "failure", "run", {})).rejects.toThrow("Service invocation failed");
    await expect(invoke(app, "failure", "toString", {})).rejects.toThrow("not explicitly exposed");
    expect(closed).toBe(1);
  });
});

test("explicit discovery/schema validation never starts resources; dual failures retain both causes", async () => {
  let starts = 0;
  const plugin = definePlugin({
    id: "resource",
    setup({ onCleanup }) {
      starts++;
      onCleanup(() => {
        throw new Error("password=cleanup-secret");
      });
      return {
        async run(_input: { name: string }) {
          throw new Error("token=business-secret");
        },
      };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    description: "Run shared service",
    input: z.object({ name: z.string() }),
  });
  const app = { plugins: [plugin], operations: [operation] };
  await expect(invoke(app, "resource", "run", { name: 42 })).rejects.toThrow(
    "shared service schema",
  );
  await expect(invoke(app, "resource", "helper", {})).rejects.toThrow("not explicitly exposed");
  expect(starts).toBe(0);
  try {
    await invoke(app, "resource", "run", { name: "Ada" });
    throw new Error("Expected failure");
  } catch (error) {
    const detail = diagnostic(error);
    expect(detail.code).toBe("invocation-and-cleanup-failed");
    expect(detail.causes?.map((cause) => cause.phase)).toEqual(["invoke", "cleanup"]);
    expect(detail.causes?.[1]?.causes?.[0]?.pluginId).toBe("resource");
    expect(JSON.stringify(detail)).not.toContain("secret");
  }
  expect(starts).toBe(1);
  const root = await fixture(`
    const plugin = { id: 'static', setup() { throw new Error('must not start'); } };
    export const operations = [{plugin, method:'run', description:'Runtime input only', input:{'~standard':{version:1,vendor:'test',validate:value=>({value})}}}];
    export default {plugins:[plugin]};
  `);
  expect((await inspect(root)).operations[0]?.schemaAvailability).toBe("runtime-validation-only");
  await generate(root);
  const first = await Bun.file(join(root, ".lenso/manifest.json")).text();
  await generate(root);
  expect(await Bun.file(join(root, ".lenso/manifest.json")).text()).toBe(first);
  expect(JSON.parse(first).schemaVersion).toBe(1);
});
