import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { bindConfig, definePluginConfig, valuesSource } from "@lenso/core/config";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import { createAuthorizationPlugin } from "../src/plugin";
import { createAuthorization } from "../src/core";

test("plugin setup borrows exact providers; does not close them or acquire resources at declaration", async () => {
  let setups = 0;
  let closes = 0;
  const provider = definePlugin({
    id: "facts",
    setup(context) {
      setups++;
      context.onCleanup(() => {
        closes++;
      });
      return { available: true };
    },
  });
  const plugin = createAuthorizationPlugin({
    id: "authorization",
    requires: [provider],
    setup(context) {
      const facts = context.get(provider);
      return createAuthorization({
        actions: ["read"],
        policies: [{ evaluate: () => (facts.available ? "allow" : "deny") }],
      });
    },
  });
  expect(setups).toBe(0);
  const app = await startApp({ plugins: [provider, plugin] });
  expect(
    await app.get(plugin).can({
      principal: null,
      action: "read",
      context: {},
      resource: { type: "note", id: "one", scope: { type: "app", id: "one" } },
    }),
  ).toBe(true);
  await app.stop();
  expect(closes).toBe(1);
  await app.stop();
  expect(closes).toBe(1);
  const impostor = definePlugin({ id: "facts", setup: () => ({ available: true }) });
  await expect(startApp({ plugins: [impostor, plugin] })).rejects.toThrow();
});

test("existing Config binds finite settings without a new config system", async () => {
  const schema: StandardSchemaV1<unknown, { timeoutMs: number }> = {
    "~standard": {
      version: 1 as const,
      vendor: "authorization-fixture",
      validate(input: unknown) {
        if (
          input &&
          typeof input === "object" &&
          "timeoutMs" in input &&
          typeof input.timeoutMs === "number" &&
          Number.isSafeInteger(input.timeoutMs) &&
          input.timeoutMs > 0
        )
          return { value: { timeoutMs: input.timeoutMs } };
        return { issues: [{ message: "Invalid timeout" }] };
      },
    },
  };
  const contract = definePluginConfig({ schema });
  const configured = bindConfig(contract, [valuesSource({ timeoutMs: 20 })], {
    id: "configured-authorization",
    setup: (_context, config) =>
      createAuthorization({ actions: ["read"], timeoutMs: config.timeoutMs }),
  });
  const app = await startApp({ plugins: [configured] });
  try {
    expect(
      (
        await app.get(configured).check({
          principal: null,
          action: "read",
          context: {},
          resource: { type: "note", id: "one", scope: { type: "app", id: "one" } },
        })
      ).code,
    ).toBe("DEFAULT_DENY");
  } finally {
    await app.stop();
  }
});
