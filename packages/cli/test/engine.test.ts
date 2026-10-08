import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { defineApp, definePlugin } from "@lenso/core";
import { z } from "zod";
import { defineOperation } from "../src/operations";
import { diagnostic } from "../src/diagnostics";
import { generate } from "@lenso/engine";
import { inspect, invoke } from "../src/engine";

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

test("inspect reports static instance metadata, sources, and redacts contributions", async () => {
  const previous = process.env.LENSO_DIAGNOSTICS_SECRET;
  process.env.LENSO_DIAGNOSTICS_SECRET = "inspect-private-value";
  try {
    const root = await fixture(`
      const first = { id: 'first', source: { file: 'one.ts', export: 'first' }, contributions: [{ kind: 'example', label: 'inspect-private-value' }], setup() { throw Error('must not start'); } };
      const second = { id: 'second', source: { file: 'two.ts' }, contributions: [{ kind: 'example', apiKey: 'credential' }], setup() { throw Error('must not start'); } };
      const consumer = { id: 'consumer', requires: [second], setup() { throw Error('must not start'); } };
      export default { plugins: [consumer, first, second] };
    `);
    await Bun.write(join(root, "lenso.engine.ts"), "throw Error('must not load');");
    const result = await inspect(root);
    expect(result.inspection).toBe("static");
    expect(result.plugins.map((plugin) => ({ id: plugin.id, requires: plugin.requires }))).toEqual([
      { id: "second", requires: [] },
      { id: "consumer", requires: ["second"] },
      { id: "first", requires: [] },
    ]);
    expect(result.plugins.find((plugin) => plugin.id === "first")?.source).toMatchObject({
      file: "one.ts",
      export: "first",
    });
    expect(JSON.stringify(result)).not.toContain("inspect-private-value");
    expect(JSON.stringify(result)).not.toContain('"apiKey":"credential"');
    expect(result.limitations.join(" ")).toContain("Does not load Engine config");
  } finally {
    if (previous === undefined) delete process.env.LENSO_DIAGNOSTICS_SECRET;
    else process.env.LENSO_DIAGNOSTICS_SECRET = previous;
  }
});
