import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { pathToFileURL } from "node:url";
import { defineApp, definePlugin } from "@lenso/core";
import { z } from "zod";
import { defineOperation } from "../src/operations";
import { diagnostic, CliError } from "../src/diagnostics";
import { generate } from "@lenso/engine";
import type { OperationBinding } from "@lenso/engine/operations";
import { inspect, invoke } from "../src/engine";

test("shared invocation preserves an application's explicit CLI error status", async () => {
  let cleanup = 0;
  const plugin = definePlugin({
    id: "known-error",
    setup(context) {
      context.onCleanup(() => {
        cleanup++;
      });
      return {
        run(_input: unknown) {
          throw new CliError(
            { code: "application-refused", phase: "invoke", message: "Refused." },
            2,
          );
        },
      };
    },
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input: z.unknown(),
    description: "Run",
  });
  await expect(
    invoke({ plugins: [plugin], operations: [operation] }, plugin.id, "run", {}),
  ).rejects.toMatchObject({ exitCode: 2, diagnostic: { code: "application-refused" } });
  expect(cleanup).toBe(1);
});

test("unknown selectors are not copied into public CLI diagnostics", async () => {
  const plugin = definePlugin({ id: "declared", setup: () => ({}) });
  const app = { plugins: [plugin], operations: [] };
  for (const [pluginId, method] of [
    ["PRIVATE-plugin", "PRIVATE-method"],
    [plugin.id, "PRIVATE-method"],
  ]) {
    const failure = await invoke(app, pluginId!, method!, {}).catch((error) => error);
    expect(JSON.stringify(diagnostic(failure))).not.toContain("PRIVATE-");
  }
});

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

  test("binds transformed input once to trusted context and the original service", async () => {
    const events: string[] = [];
    const actor = { id: "launch-owner" };
    let transforms = 0;
    let receivedInput: unknown;
    const input = z.object({ name: z.string(), actor: z.string() }).transform((value) => {
      transforms++;
      events.push("validate");
      return { ...value, name: value.name.toUpperCase() };
    });
    const plugin = definePlugin({
      id: "bound",
      setup({ onCleanup }) {
        events.push("setup");
        onCleanup(async () => {
          events.push("cleanup");
          await Bun.sleep(5);
          events.push("closed");
        });
        const service = {
          prefix: "Hello",
          async greet(
            value: z.output<typeof input>,
            context: { actor: typeof actor; service: unknown },
          ): Promise<{ message: string; actor: string; token: string }> {
            events.push("invoke");
            expect(receivedInput).toBe(value);
            expect(context.actor).toBe(actor);
            expect(context.service).toBe(this);
            return {
              message: `${this.prefix} ${value.name}`,
              actor: context.actor.id,
              token: "private-token",
            };
          },
        };
        return service;
      },
    });
    const operation = defineOperation({
      plugin,
      method: "greet",
      description: "Bound greeting",
      input,
      context: true,
      confirmation: "required",
      approval: "required",
    });
    const binding: OperationBinding<typeof operation> = async (
      selected,
      validatedInput,
      running,
    ) => {
      events.push("binding");
      expect(selected).toBe(operation);
      expect(validatedInput).toEqual({ name: "ADA", actor: "input-attacker" });
      receivedInput = validatedInput;
      return {
        context: { actor, service: running.get(plugin) },
        confirm: async () => {
          events.push("confirm");
          return true;
        },
        approve: async () => {
          events.push("approve");
          return true;
        },
      };
    };
    expect(
      await invoke(
        {
          ...defineApp({ plugins: [plugin] }),
          operations: [operation],
          operationBinding: binding,
        },
        "bound",
        "greet",
        { name: "Ada", actor: "input-attacker" },
      ),
    ).toEqual({
      message: "Hello ADA",
      actor: "launch-owner",
      token: "[REDACTED]",
    });
    expect(transforms).toBe(1);
    expect(events).toEqual([
      "validate",
      "setup",
      "binding",
      "confirm",
      "approve",
      "invoke",
      "cleanup",
      "closed",
    ]);
  });

  test.each([
    ["context", "missing", undefined, "missing-context-binding"],
    ["confirmation", "missing", undefined, "confirmation-required"],
    ["confirmation", "false", false, "confirmation-required"],
    ["approval", "missing", undefined, "approval-required"],
    ["approval", "false", false, "approval-required"],
  ] as const)(
    "refuses %s with %s gate and awaits cleanup",
    async (requirement, _label, gate, code) => {
      const events: string[] = [];
      const plugin = definePlugin({
        id: "guarded",
        setup({ onCleanup }) {
          events.push("setup");
          onCleanup(async () => {
            await Bun.sleep(5);
            events.push("closed");
          });
          return {
            async run(_input: unknown, _context?: unknown) {
              events.push("side-effect");
              return {};
            },
          };
        },
      });
      const operation = defineOperation({
        plugin,
        method: "run",
        description: "Guarded call",
        input: z.unknown(),
        context: true,
        ...(requirement === "confirmation" ? { confirmation: "required" as const } : {}),
        ...(requirement === "approval" ? { approval: "required" as const } : {}),
      });
      const binding: OperationBinding<typeof operation> | undefined =
        requirement === "context"
          ? undefined
          : () => ({
              context: {},
              ...(gate === undefined ? {} : { confirm: () => gate, approve: () => gate }),
            });
      try {
        await invoke(
          { plugins: [plugin], operations: [operation] },
          "guarded",
          "run",
          {
            context: { actor: "attacker" },
            confirmed: true,
            approved: true,
          },
          binding,
        );
        throw new Error("Expected refusal");
      } catch (error) {
        expect(diagnostic(error).code).toBe(code);
      }
      expect(events).toEqual(["setup", "closed"]);
    },
  );

  test("explicit binding overrides the app binding; binding failures are opaque and close resources", async () => {
    let closed = 0;
    let calls = 0;
    const plugin = definePlugin({
      id: "binding",
      setup({ onCleanup }) {
        onCleanup(async () => {
          await Bun.sleep(5);
          closed++;
        });
        return {
          async run() {
            calls++;
            return {};
          },
        };
      },
    });
    const app = {
      plugins: [plugin],
      operations: [
        defineOperation({ plugin, method: "run", description: "Run", input: z.unknown() }),
      ],
      operationBinding: () => {
        throw new Error("private-binding-secret");
      },
    };
    expect(await invoke(app, "binding", "run", {}, () => ({}))).toEqual({});
    try {
      await invoke(app, "binding", "run", {});
      throw new Error("Expected binding failure");
    } catch (error) {
      expect(diagnostic(error).code).toBe("invocation-failed");
      expect(JSON.stringify(diagnostic(error))).not.toContain("private-binding-secret");
    }
    expect(calls).toBe(1);
    expect(closed).toBe(2);
  });

  test.each([
    ["undefined", undefined, "serialization-failed"],
    ["infinity", Number.POSITIVE_INFINITY, "serialization-failed"],
    ["nonfinite sensitive field", { token: Number.POSITIVE_INFINITY }, "serialization-failed"],
    ["oversized object", { value: "x".repeat(100) }, "output-too-large"],
  ] as const)(
    "checks finite output before redaction and respects the bound output limit (%s)",
    async (_label, value, code) => {
      let closed = false;
      const plugin = definePlugin({
        id: "output",
        setup({ onCleanup }) {
          onCleanup(async () => {
            await Bun.sleep(5);
            closed = true;
          });
          return {
            async run() {
              return value;
            },
          };
        },
      });
      try {
        await invoke(
          {
            plugins: [plugin],
            operations: [
              defineOperation({ plugin, method: "run", description: "Output", input: z.unknown() }),
            ],
          },
          "output",
          "run",
          {},
          () => ({ maxOutputBytes: 64 }),
        );
        throw new Error("Expected output failure");
      } catch (error) {
        expect(diagnostic(error).code).toBe(code);
      }
      expect(closed).toBe(true);
    },
  );

  test("a declared inherited method is still unavailable and cleanup is awaited", async () => {
    let called = false;
    let closed = false;
    const inherited = {
      async run() {
        called = true;
        return {};
      },
    };
    const plugin = definePlugin({
      id: "inherited",
      setup({ onCleanup }) {
        onCleanup(async () => {
          await Bun.sleep(5);
          closed = true;
        });
        return Object.create(inherited) as typeof inherited;
      },
    });
    try {
      await invoke(
        {
          plugins: [plugin],
          operations: [
            defineOperation({
              plugin,
              method: "run",
              description: "Inherited",
              input: z.unknown(),
            }),
          ],
        },
        "inherited",
        "run",
        {},
      );
      throw new Error("Expected unavailable method");
    } catch (error) {
      expect(diagnostic(error).code).toBe("unavailable-operation");
    }
    expect(called).toBe(false);
    expect(closed).toBe(true);
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

test("inspect describes instance configuration without source reads or business setup", async () => {
  const root = await fixture(`
    import {bindConfig,definePluginConfig} from ${JSON.stringify(pathToFileURL(Bun.resolveSync("@lenso/core", import.meta.dir)).href)};
    const source = {
      descriptor: {
        id:'explicit-env',kind:'env',
        fields:[{path:['credential'],env:'EXPLICIT_CONFIG_KEY',sensitive:true}]
      },
      async read() { throw Error('source must not be read during inspection'); }
    };
    const contract = definePluginConfig({
      schema:{'~standard':{version:1,vendor:'test',validate:value=>({value})}},
      fields:[{path:['credential'],sensitive:true}],
      jsonSchema:()=>({type:'object',properties:{credential:{default:'private-default',examples:['private-example']}}})
    });
    const first=bindConfig(contract,[source],{id:'first',setup(){throw Error('must not start')}});
    const second=bindConfig(contract,{credential:'private-value'},{id:'second',setup(){throw Error('must not start')}});
    export default {plugins:[first,second]};
  `);
  const result = await inspect(root);
  expect(result.plugins[0]?.config).toMatchObject({
    sources: [
      {
        id: "explicit-env",
        kind: "env",
        fields: [{ path: ["credential"], env: "EXPLICIT_CONFIG_KEY", sensitive: true }],
      },
    ],
    inputSchema: { type: "object", properties: { credential: { writeOnly: true } } },
  });
  expect(JSON.stringify(result)).not.toContain("private-");
  await generate(root);
  expect(await Bun.file(join(root, ".lenso/manifest.json")).text()).not.toContain("private-");
});
