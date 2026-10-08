import { expect, test } from "bun:test";
import { bindConfig, definePlugin, definePluginConfig } from "@lenso/core";
import worker from "./fixtures/config-worker";
import { createWorkerHandler } from "../src/index";

const executionContext = { waitUntil: (_promise: Promise<unknown>) => {} };

test("Workers resolve explicit string bindings and reject invalid configuration before setup", async () => {
  expect(
    await (
      await worker.fetch(
        new Request("https://example.test"),
        { ENABLED: "false" },
        executionContext,
      )
    ).text(),
  ).toBe("false");
  await expect(
    worker.fetch(new Request("https://example.test"), { ENABLED: "yes" }, executionContext),
  ).rejects.toMatchObject({
    diagnostics: [{ code: "config-env-invalid", pluginId: "web", sourceId: "worker-env" }],
  });
});

test("request cancellation during preflight never starts resources", async () => {
  let setups = 0;
  const abort = new AbortController();
  const handler = createWorkerHandler(() => {
    const resource = definePlugin({
      id: "resource",
      setup() {
        setups++;
      },
    });
    const web = bindConfig(
      definePluginConfig({
        schema: {
          "~standard": { version: 1, vendor: "test", validate: (value: unknown) => ({ value }) },
        },
      }),
      [
        {
          descriptor: { id: "cancel", kind: "memory" },
          async read() {
            abort.abort("private cancellation reason");
            return { values: {} };
          },
        },
      ],
      {
        id: "web",
        setup() {
          setups++;
          return { fetch: async () => new Response("must not start") };
        },
      },
    );
    return { plugins: [resource, web], web };
  });
  await expect(
    handler.fetch(
      new Request("https://example.test", { signal: abort.signal }),
      {},
      executionContext,
    ),
  ).rejects.toMatchObject({ diagnostics: [{ code: "config-cancelled" }] });
  expect(setups).toBe(0);
});

test("the configured Workers dependency graph bundles without local filesystem or Bun imports", async () => {
  const result = await Bun.build({
    entrypoints: [joinFixture()],
    target: "browser",
    packages: "bundle",
  });
  expect(result.success).toBe(true);
  const output = await result.outputs[0]!.text();
  expect(output).not.toMatch(/node:fs|node:path|bun:|process\.env|Bun\./);
});

function joinFixture() {
  return `${import.meta.dir}/fixtures/config-worker.ts`;
}
